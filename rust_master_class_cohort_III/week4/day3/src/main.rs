//! Week 4 · Day 3 — CRUD + Shared State
//!
//! Yesterday's API forgot everything the moment a request ended. Today the same
//! routes are backed by one `HashMap` that every handler on every worker thread
//! can see: `Arc<Mutex<_>>`.
//!
//! Run with: `cargo run`

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Post {
    id: u64,
    title: String,
    body: String,
    author: String,
    created_at: String,
}

/// `POST` and `PUT` body. Every field required — a full representation.
#[derive(Debug, Deserialize)]
struct CreatePost {
    title: String,
    body: String,
    #[serde(default = "anonymous")]
    author: String,
}

/// `PATCH` body. Every field optional, because a partial update means "change
/// only what I sent". This is the entire structural difference between PUT and
/// PATCH, and it shows up in the type.
///
/// Note the ambiguity this type cannot express: `None` means "not supplied", and
/// there is no way for a client to say "set this to null". For that you need a
/// nested `Option<Option<T>>` with `#[serde(deserialize_with = ...)]`. Mention it,
/// don't build it.
#[derive(Debug, Deserialize)]
struct UpdatePost {
    title: Option<String>,
    body: Option<String>,
    author: Option<String>,
}

fn anonymous() -> String {
    "anonymous".to_string()
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// The store. `next_id` lives *inside* the same struct as `posts` so that one
/// lock covers both.
///
/// If the counter were a separate `Mutex<u64>`, two concurrent creates could each
/// lock it, both read `3`, both release, and both insert with id 3 — one post
/// silently overwriting the other. Putting the counter under the same lock as the
/// map makes "read the id, then insert" a single atomic step.
#[derive(Debug)]
struct Store {
    posts: HashMap<u64, Post>,
    next_id: u64,
}

impl Store {
    fn seeded() -> Self {
        let mut posts = HashMap::new();
        posts.insert(
            1,
            Post {
                id: 1,
                title: "Futures are lazy".into(),
                body: "Calling an async fn runs none of its body.".into(),
                author: "Ada".into(),
                created_at: "2026-08-03T09:00:00Z".into(),
            },
        );
        posts.insert(
            2,
            Post {
                id: 2,
                title: "join! vs spawn".into(),
                body: "One task interleaved, versus N tasks needing Send + 'static.".into(),
                author: "Grace".into(),
                created_at: "2026-08-04T09:00:00Z".into(),
            },
        );
        Store { posts, next_id: 3 }
    }
}

/// Everything shared across requests.
///
/// `Arc` = shared ownership, so N handler tasks can hold the same value.
/// `Mutex` = exclusive access, so only one of them mutates at a time.
/// Two distinct problems, two distinct tools. You need both.
///
/// `std::sync::Mutex` (not `tokio::sync::Mutex`) because we never hold the guard
/// across an `.await`. Its guard is `!Send`, so if you tried, the compiler would
/// stop you — that error is a deadlock caught at build time.
struct AppState {
    store: Mutex<Store>,
}

// ---------------------------------------------------------------------------
// Handlers
//
// Every handler takes `State<Arc<AppState>>` as its first argument. Note the
// ordering rule from yesterday still applies: `Json` consumes the body, so it
// goes last.
// ---------------------------------------------------------------------------

/// READ (collection).
///
/// `HashMap` iteration order is deliberately randomised, so sort before
/// responding — otherwise the same data comes back shuffled on every request and
/// students will think they have a bug.
async fn list_posts(State(state): State<Arc<AppState>>) -> Json<Vec<Post>> {
    let store = state.store.lock().unwrap();

    let mut posts: Vec<Post> = store.posts.values().cloned().collect();
    posts.sort_by_key(|p| p.id);

    Json(posts)
}

/// CREATE.
///
/// The whole read-counter-then-insert sequence happens under one lock, so
/// concurrent creates cannot collide on an id.
async fn create_post(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreatePost>,
) -> (StatusCode, Json<Post>) {
    let mut store = state.store.lock().unwrap();

    let id = store.next_id;
    store.next_id += 1;

    let post = Post {
        id,
        title: payload.title,
        body: payload.body,
        author: payload.author,
        created_at: "2026-08-08T11:30:00Z".into(),
    };

    store.posts.insert(id, post.clone());

    (StatusCode::CREATED, Json(post))
}

/// READ (single).
async fn get_post(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Result<Json<Post>, StatusCode> {
    let store = state.store.lock().unwrap();

    store
        .posts
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// UPDATE — full replacement. Every field must be supplied.
///
/// `id` and `created_at` are preserved from the existing post: identity and
/// creation time are not the client's to rewrite.
async fn replace_post(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(payload): Json<CreatePost>,
) -> Result<Json<Post>, StatusCode> {
    let mut store = state.store.lock().unwrap();

    let existing = store.posts.get(&id).ok_or(StatusCode::NOT_FOUND)?;

    let updated = Post {
        id,
        title: payload.title,
        body: payload.body,
        author: payload.author,
        created_at: existing.created_at.clone(),
    };

    store.posts.insert(id, updated.clone());

    Ok(Json(updated))
}

/// UPDATE — partial. Only the supplied fields change.
async fn update_post(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdatePost>,
) -> Result<Json<Post>, StatusCode> {
    let mut store = state.store.lock().unwrap();

    let post = store.posts.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;

    // `if let Some(x) = ...` is the whole PATCH semantic: absent stays untouched.
    if let Some(title) = payload.title {
        post.title = title;
    }
    if let Some(body) = payload.body {
        post.body = body;
    }
    if let Some(author) = payload.author {
        post.author = author;
    }

    Ok(Json(post.clone()))
}

/// DELETE.
///
/// 204 No Content on success: the deletion succeeded and there is nothing
/// meaningful left to return. Returning the deleted object with a 200 is a
/// defensible alternative — worth a minute of argument in class.
async fn delete_post(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Result<StatusCode, StatusCode> {
    let mut store = state.store.lock().unwrap();

    match store.posts.remove(&id) {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // Built once, before the server starts. Every request handler gets a cheap
    // `Arc` clone of this exact value — one refcount bump, no data copied.
    let state = Arc::new(AppState {
        store: Mutex::new(Store::seeded()),
    });

    let app = Router::new()
        .route("/posts", get(list_posts).post(create_post))
        .route(
            "/posts/{id}",
            get(get_post)
                .put(replace_post)
                .patch(update_post)
                .delete(delete_post),
        )
        // `.with_state` is what makes the `State<Arc<AppState>>` extractor
        // resolve. Forget it and you get a trait-bound error on `Router`, not a
        // helpful "you forgot the state" message — worth showing on purpose.
        .with_state(state);

    let addr = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    println!("listening on http://{addr}");
    println!();
    println!("  curl -X POST localhost:3000/posts -H 'content-type: application/json' \\");
    println!("       -d '{{\"title\":\"Shared state\",\"body\":\"Arc<Mutex<T>>\"}}'");
    println!("  curl localhost:3000/posts        # it persists now");
    println!();
    println!("concurrency check — 20 parallel creates, expect 22 posts total:");
    println!("  seq 1 20 | xargs -P 20 -I{{}} curl -s -o /dev/null -X POST \\");
    println!("    localhost:3000/posts -H 'content-type: application/json' -d '{{\"title\":\"p\",\"body\":\"b\"}}'");

    axum::serve(listener, app).await.unwrap();
}
