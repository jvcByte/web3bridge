//! Week 4 · Day 4 — Error Handling & Middleware
//!
//! Yesterday's CRUD worked but failed badly: every error path was a bare
//! `StatusCode` with no body. Today errors become a real type, and two middleware
//! layers go on top — request logging and `X-API-KEY` auth.
//!
//! Still one file, on purpose. Day 5 splits it into modules, and that refactor
//! only teaches something if there is something to split.
//!
//! Run with: `cargo run`   (or `API_KEY=hunter2 cargo run`)

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::{
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
    Extension, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// One error type for the whole application.
///
/// `thiserror` generates `Display` from the `#[error(...)]` attributes and `From`
/// impls from `#[from]` fields. It does not generate the HTTP mapping — that is
/// the `IntoResponse` impl below, and keeping those two separate is the point:
/// the enum describes *what went wrong*, the impl decides *how to say it over
/// HTTP*.
#[derive(Debug, Error)]
enum ApiError {
    #[error("{0} not found")]
    NotFound(String),

    #[error("{0}")]
    Validation(String),

    #[error("missing or invalid X-API-KEY")]
    Unauthorized,

    /// The catch-all. Its message is logged but never sent to the client.
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    fn status(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Validation(_) => StatusCode::BAD_REQUEST,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// A stable, machine-readable slug. Clients should branch on this, not on the
    /// human-readable message, which you want to stay free to reword.
    fn kind(&self) -> &'static str {
        match self {
            ApiError::NotFound(_) => "not_found",
            ApiError::Validation(_) => "validation_failed",
            ApiError::Unauthorized => "unauthorized",
            ApiError::Internal(_) => "internal_error",
        }
    }
}

/// This impl is what makes `Result<T, ApiError>` a valid handler return type, and
/// therefore what makes `?` usable in every handler.
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // Security habit of the day: log the real cause, return a generic message.
        //
        // `ApiError::Internal` typically wraps a database error, a file path, or a
        // panic message. Putting any of that in the response body is an
        // information-disclosure bug. The client gets a slug and nothing else.
        let client_message = match &self {
            ApiError::Internal(detail) => {
                eprintln!("  !! internal error: {detail}");
                "internal server error".to_string()
            }
            other => other.to_string(),
        };

        let body = Json(json!({
            "error": {
                "kind": self.kind(),
                "message": client_message,
            }
        }));

        (self.status(), body).into_response()
    }
}

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

#[derive(Debug, Deserialize)]
struct CreatePost {
    title: String,
    body: String,
    #[serde(default = "anonymous")]
    author: String,
}

#[derive(Debug, Deserialize)]
struct UpdatePost {
    title: Option<String>,
    body: Option<String>,
    author: Option<String>,
}

fn anonymous() -> String {
    "anonymous".to_string()
}

/// Validation lives on the request type, not scattered through the handlers.
///
/// Returning `Result<(), ApiError>` rather than `bool` means the reason travels
/// with the failure, so the client gets "title must not be empty" instead of a
/// naked 400.
impl CreatePost {
    fn validate(&self) -> Result<(), ApiError> {
        if self.title.trim().is_empty() {
            return Err(ApiError::Validation("title must not be empty".into()));
        }
        if self.title.chars().count() > 120 {
            return Err(ApiError::Validation(
                "title must be 120 characters or fewer".into(),
            ));
        }
        if self.body.trim().is_empty() {
            return Err(ApiError::Validation("body must not be empty".into()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

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

struct AppState {
    store: Mutex<Store>,
    /// The expected `X-API-KEY`. Read from the environment at startup — a real
    /// deployment would never hardcode this, and the default exists only so the
    /// class can run `cargo run` with no setup.
    api_key: String,
    /// Monotonic counter for request ids. An `AtomicU64` rather than a `Mutex<u64>`
    /// because a bare increment is exactly what atomics are for — no lock needed.
    request_counter: AtomicU64,
}

/// Injected into every request by the logging middleware, read back out by
/// handlers via the `Extension` extractor.
#[derive(Debug, Clone, Copy)]
struct RequestId(u64);

// ---------------------------------------------------------------------------
// Middleware
//
// Both of these are plain `async fn(Request, Next) -> Response`, turned into a
// `tower::Layer` by `middleware::from_fn` / `from_fn_with_state`.
//
// `Next` is the rest of the stack — every remaining layer plus the handler.
// Calling `next.run(req).await` runs it; *not* calling it short-circuits.
// ---------------------------------------------------------------------------

/// Logs every request and injects a `RequestId`.
///
/// Everything before `next.run()` happens on the way in; everything after happens
/// on the way out, with the response in hand. That in/out sandwich is the shape
/// of all middleware.
async fn log_requests(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let id = RequestId(state.request_counter.fetch_add(1, Ordering::Relaxed));
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Extensions are a typed map on the request — the channel from middleware to
    // handler. Anything inserted here can be pulled out downstream with the
    // `Extension<T>` extractor. This is how real auth middleware passes the
    // authenticated user down to handlers.
    req.extensions_mut().insert(id);

    let started = Instant::now();
    let response = next.run(req).await;
    let elapsed = started.elapsed();

    println!(
        "[req {:>4}] {:<6} {:<24} -> {} ({:.2?})",
        id.0,
        method.as_str(),
        path,
        response.status().as_u16(),
        elapsed,
    );

    response
}

/// Rejects requests without a valid `X-API-KEY`.
///
/// Note that it returns `Result<Response, ApiError>` — the error path never calls
/// `next`, so the handler is never reached. That short-circuit is the entire
/// mechanism behind auth, rate limiting, and caching middleware.
async fn require_api_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let provided = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Unauthorized)?;

    if !constant_time_eq(provided.as_bytes(), state.api_key.as_bytes()) {
        return Err(ApiError::Unauthorized);
    }

    Ok(next.run(req).await)
}

/// Compares two byte strings without short-circuiting on the first difference.
///
/// A plain `==` on strings returns as soon as it finds a mismatch, so an attacker
/// can measure response time to learn how many leading bytes they guessed right,
/// turning an infeasible brute force into a byte-at-a-time one. For a classroom
/// demo this is arguably paranoid — noticing that it matters is the habit worth
/// building. In production, reach for the `subtle` crate.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // Fold every byte into an accumulator; only zero if all pairs matched.
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ---------------------------------------------------------------------------
// Handlers
//
// Every one returns `Result<T, ApiError>`, so `?` works throughout and no handler
// contains response-formatting code.
// ---------------------------------------------------------------------------

/// `Extension<RequestId>` reads what the logging middleware inserted.
///
/// This resolves at *runtime*, not compile time: if this route were not covered
/// by `log_requests`, the extractor would fail with a 500 rather than a compile
/// error. Worth demonstrating once by removing the layer.
async fn list_posts(
    State(state): State<Arc<AppState>>,
    Extension(RequestId(req_id)): Extension<RequestId>,
) -> Result<Json<Vec<Post>>, ApiError> {
    let store = state
        .store
        .lock()
        .map_err(|e| ApiError::Internal(format!("store lock poisoned: {e}")))?;

    let mut posts: Vec<Post> = store.posts.values().cloned().collect();
    posts.sort_by_key(|p| p.id);

    println!("         (req {req_id} listed {} posts)", posts.len());

    Ok(Json(posts))
}

async fn create_post(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreatePost>,
) -> Result<(StatusCode, Json<Post>), ApiError> {
    // `?` converts nothing here — validate already returns ApiError. But the same
    // `?` would convert via `From` if it returned a different error type, which is
    // what `#[from]` on a thiserror variant buys you.
    payload.validate()?;

    let mut store = state
        .store
        .lock()
        .map_err(|e| ApiError::Internal(format!("store lock poisoned: {e}")))?;

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

    Ok((StatusCode::CREATED, Json(post)))
}

async fn get_post(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Result<Json<Post>, ApiError> {
    let store = state
        .store
        .lock()
        .map_err(|e| ApiError::Internal(format!("store lock poisoned: {e}")))?;

    store
        .posts
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("post {id}")))
}

async fn replace_post(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(payload): Json<CreatePost>,
) -> Result<Json<Post>, ApiError> {
    payload.validate()?;

    let mut store = state
        .store
        .lock()
        .map_err(|e| ApiError::Internal(format!("store lock poisoned: {e}")))?;

    let existing = store
        .posts
        .get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("post {id}")))?;

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

async fn update_post(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
    Json(payload): Json<UpdatePost>,
) -> Result<Json<Post>, ApiError> {
    // Validate the *supplied* fields only — that is what makes this a PATCH.
    if let Some(title) = &payload.title {
        if title.trim().is_empty() {
            return Err(ApiError::Validation("title must not be empty".into()));
        }
    }
    if let Some(body) = &payload.body {
        if body.trim().is_empty() {
            return Err(ApiError::Validation("body must not be empty".into()));
        }
    }

    let mut store = state
        .store
        .lock()
        .map_err(|e| ApiError::Internal(format!("store lock poisoned: {e}")))?;

    let post = store
        .posts
        .get_mut(&id)
        .ok_or_else(|| ApiError::NotFound(format!("post {id}")))?;

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

async fn delete_post(
    State(state): State<Arc<AppState>>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    let mut store = state
        .store
        .lock()
        .map_err(|e| ApiError::Internal(format!("store lock poisoned: {e}")))?;

    store
        .posts
        .remove(&id)
        .ok_or_else(|| ApiError::NotFound(format!("post {id}")))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Deliberately fails, so the class can see the split between what gets logged
/// and what the client receives.
async fn boom() -> Result<Json<Post>, ApiError> {
    Err(ApiError::Internal(
        "connection to postgres://user:hunter2@db.internal:5432 refused".into(),
    ))
}

/// Unmatched routes. Without this, axum's default 404 has an empty body, which
/// breaks the "every error is JSON" contract the client is relying on.
async fn fallback() -> ApiError {
    ApiError::NotFound("route".into())
}

// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let api_key = std::env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string());

    let state = Arc::new(AppState {
        store: Mutex::new(Store::seeded()),
        api_key: api_key.clone(),
        request_counter: AtomicU64::new(1),
    });

    // Write routes, isolated so the auth layer can apply to these and only these.
    let write_routes = Router::new()
        .route("/posts", axum::routing::post(create_post))
        .route(
            "/posts/{id}",
            axum::routing::put(replace_post)
                .patch(update_post)
                .delete(delete_post),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ));

    // Public read routes.
    let read_routes = Router::new()
        .route("/posts", get(list_posts))
        .route("/posts/{id}", get(get_post))
        .route("/boom", get(boom));

    let app = read_routes
        // Merging two routers that both define `/posts` is fine — axum combines
        // the method routers, so GET goes to the public handler and POST goes
        // through the auth layer.
        .merge(write_routes)
        .fallback(fallback)
        // Added LAST, so it is the OUTERMOST layer.
        //
        // Layers wrap inside-out: the last `.layer()` sees the request first and
        // the response last. That is what lets this log the 401s that
        // `require_api_key` generates — if the order were flipped, the auth
        // rejection would never reach the logger. Swap these in class and watch
        // 401s vanish from the console.
        .layer(middleware::from_fn_with_state(state.clone(), log_requests))
        .with_state(state);

    let addr = "127.0.0.1:3000";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    println!("listening on http://{addr}");
    println!("api key: {api_key}   (override with API_KEY=... cargo run)");
    println!();
    println!("  curl localhost:3000/posts                      # public");
    println!("  curl -i -X POST localhost:3000/posts \\         # 401, no key");
    println!("       -H 'content-type: application/json' -d '{{\"title\":\"a\",\"body\":\"b\"}}'");
    println!("  curl -i -X POST localhost:3000/posts \\         # 201");
    println!("       -H 'content-type: application/json' -H 'x-api-key: {api_key}' \\");
    println!("       -d '{{\"title\":\"a\",\"body\":\"b\"}}'");
    println!("  curl -i localhost:3000/posts/999               # 404 as JSON");
    println!("  curl -i localhost:3000/boom                    # 500 — compare console vs response");

    axum::serve(listener, app).await.unwrap();
}

