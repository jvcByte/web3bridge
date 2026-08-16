//! CRUD handlers for `Post`.
//!
//! Each one: take the lock, do one thing, drop the lock. No `.await` inside a
//! locked scope anywhere in this file — which is why `std::sync::Mutex` is the
//! right choice over `tokio::sync::Mutex`.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;

use crate::errors::{ApiError, ApiResult};
use crate::models::{CreatePost, Post, UpdatePost};
use crate::state::{now_epoch_secs, AppState};

/// `GET /posts` query string. Every field optional, so a bare `/posts` still works.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    author: Option<String>,
    q: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_limit() -> usize {
    50
}

const MAX_LIMIT: usize = 200;

/// `GET /posts` — filter, paginate, sort.
///
/// `HashMap` iteration order is randomised, so the sort is mandatory rather than
/// cosmetic: without it the same data comes back shuffled every request.
pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListQuery>,
) -> ApiResult<Json<Vec<Post>>> {
    // Clamp rather than reject: a client asking for 10,000 rows gets 200, not a
    // 400. Leaving it unbounded lets one request try to serialise the entire store.
    let limit = params.limit.min(MAX_LIMIT);
    let needle = params.q.map(|q| q.to_lowercase());

    let store = state.store()?;

    let mut posts: Vec<Post> = store
        .posts
        .values()
        .filter(|p| match &params.author {
            Some(author) => p.author.eq_ignore_ascii_case(author),
            None => true,
        })
        .filter(|p| match &needle {
            Some(needle) => {
                p.title.to_lowercase().contains(needle)
                    || p.body.to_lowercase().contains(needle)
            }
            None => true,
        })
        .cloned()
        .collect();

    posts.sort_by_key(|p| p.id);

    Ok(Json(
        posts.into_iter().skip(params.offset).take(limit).collect(),
    ))
}

/// `GET /posts/{id}`
pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> ApiResult<Json<Post>> {
    let store = state.store()?;

    store
        .posts
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("post {id}")))
}

/// `POST /posts` — 201 on success.
pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreatePost>,
) -> ApiResult<(StatusCode, Json<Post>)> {
    // Validate before taking the lock. Rejecting a bad request should not make
    // other requests queue behind it.
    payload.validate()?;

    let mut store = state.store()?;

    if store.title_exists(&payload.title, None) {
        return Err(ApiError::Conflict(format!("a post titled {:?}", payload.title.trim())));
    }

    // Allocating the id and inserting happen under the same guard, so two
    // concurrent creates cannot collide on an id.
    let id = store.allocate_id();

    let post = Post {
        id,
        title: payload.title.trim().to_string(),
        body: payload.body,
        author: payload.author,
        created_at: now_epoch_secs(),
    };

    store.posts.insert(id, post.clone());

    Ok((StatusCode::CREATED, Json(post)))
}

/// `PUT /posts/{id}` — full replacement.
///
/// `id` and `created_at` survive: identity and creation time are not the client's
/// to rewrite. Because they are absent from `CreatePost`, that is enforced by the
/// type rather than by a check.
pub async fn replace(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(payload): Json<CreatePost>,
) -> ApiResult<Json<Post>> {
    payload.validate()?;

    let mut store = state.store()?;

    let created_at = store
        .posts
        .get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("post {id}")))?
        .created_at;

    // `excluding: Some(id)` — a post is allowed to keep its own title.
    if store.title_exists(&payload.title, Some(id)) {
        return Err(ApiError::Conflict(format!("a post titled {:?}", payload.title.trim())));
    }

    let updated = Post {
        id,
        title: payload.title.trim().to_string(),
        body: payload.body,
        author: payload.author,
        created_at,
    };

    store.posts.insert(id, updated.clone());

    Ok(Json(updated))
}

/// `PATCH /posts/{id}` — partial update.
pub async fn update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdatePost>,
) -> ApiResult<Json<Post>> {
    payload.validate()?;

    // `{}` changes nothing. A 200 here would tell the client a write happened
    // when none did.
    if payload.is_empty() {
        return Err(ApiError::Validation(
            "supply at least one of: title, body, author".into(),
        ));
    }

    let mut store = state.store()?;

    if !store.posts.contains_key(&id) {
        return Err(ApiError::NotFound(format!("post {id}")));
    }

    // The conflict check needs an immutable borrow of the whole map, and the
    // mutation needs a mutable borrow of one entry. They cannot overlap, so the
    // check happens first and its result is held as a plain `bool`. This is the
    // borrow checker steering the code, and it is worth pointing at in review.
    if let Some(title) = &payload.title {
        if store.title_exists(title, Some(id)) {
            return Err(ApiError::Conflict(format!("a post titled {:?}", title.trim())));
        }
    }

    let post = store
        .posts
        .get_mut(&id)
        .ok_or_else(|| ApiError::NotFound(format!("post {id}")))?;

    if let Some(title) = payload.title {
        post.title = title.trim().to_string();
    }
    if let Some(body) = payload.body {
        post.body = body;
    }
    if let Some(author) = payload.author {
        post.author = author;
    }

    Ok(Json(post.clone()))
}

/// `DELETE /posts/{id}` — 204, nothing left to return.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> ApiResult<StatusCode> {
    let mut store = state.store()?;

    store
        .posts
        .remove(&id)
        .ok_or_else(|| ApiError::NotFound(format!("post {id}")))?;

    Ok(StatusCode::NO_CONTENT)
}
