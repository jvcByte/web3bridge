//! Week 4 · Day 5 — Blog API, routing table.
//!
//! The entire URL surface of the service in one file. No business logic here —
//! if you want to know what the API *exposes*, this is the only file to read.

use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, get, patch, post, put},
    Router,
};

use crate::handlers::{health, posts};
use crate::middleware::{log_requests, require_api_key};
use crate::state::AppState;

/// Builds the fully-wired application.
///
/// Returns a `Router`, not a running server. That separation is what makes the
/// integration tests in `tests/api.rs` possible: they drive this value directly
/// with `oneshot`, no TcpListener and no port, so tests run in parallel and never
/// collide.
pub fn app(state: Arc<AppState>) -> Router {
    // Public. Anyone may read.
    let public = Router::new()
        .route("/", get(health::root))
        .route("/health", get(health::health))
        .route("/about", get(health::about))
        .route("/posts", get(posts::list))
        .route("/posts/{id}", get(posts::get));

    // Protected. Every mutation needs the API key.
    //
    // `route_layer` rather than `layer`: it applies only to requests that actually
    // matched a route in *this* router. With plain `layer`, a request to an
    // unknown path would still run the auth check and come back 401 instead of
    // 404 — leaking which routes exist to unauthenticated callers.
    let protected = Router::new()
        .route("/posts", post(posts::create))
        .route("/posts/{id}", put(posts::replace))
        .route("/posts/{id}", patch(posts::update))
        .route("/posts/{id}", delete(posts::delete))
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            require_api_key,
        ));

    public
        // Both routers define `/posts` and `/posts/{id}`. Merging combines their
        // method routers, so GET lands on the public handler while POST/PUT/PATCH/
        // DELETE go through auth. Merging two routers with the *same method* on
        // the same path would panic at startup instead — a loud failure, by design.
        .merge(protected)
        .fallback(not_found)
        // Added last, so it is outermost: it sees the request first and the
        // response last, and therefore logs the 401s that the auth layer
        // generates. Move this above `.merge(protected)` and those disappear
        // from the console — worth demonstrating.
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            log_requests,
        ))
        .with_state(state)
}

/// Unmatched routes. Axum's built-in 404 has an empty body, which would break the
/// "every error is JSON with a `kind`" contract the client codes against.
async fn not_found() -> crate::errors::ApiError {
    crate::errors::ApiError::NotFound("route".into())
}
