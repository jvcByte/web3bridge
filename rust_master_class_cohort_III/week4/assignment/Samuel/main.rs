mod error;
mod handlers;
mod middleware;
mod models;
mod state;

use axum::{
    Router,
    routing::{get, post, put},
};
use std::sync::{Arc, atomic::AtomicU64, Mutex};

use handlers::*;
use state::{AppState, SharedState};

#[tokio::main]
async fn main() {
    let state: SharedState = Arc::new(AppState {
        store: Mutex::new(state::seed_books()),
        request_counter: AtomicU64::new(0),
    });

    let public_routes = Router::new()
        .route("/books", get(list_books))
        .route("/books/{id}", get(get_book))
        .route("/search", get(search_books))
        .route("/health", get(health));

    let write_routes = Router::new()
        .route("/books", post(create_book))
        .route("/books/{id}", put(update_book).patch(patch_book).delete(delete_book))
        .layer(axum::middleware::from_fn(middleware::auth_middleware));

    let app = Router::new()
        .merge(public_routes)
        .merge(write_routes)
        .fallback(fallback)
        .layer(axum::middleware::from_fn_with_state(state.clone(), middleware::log_requests))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
