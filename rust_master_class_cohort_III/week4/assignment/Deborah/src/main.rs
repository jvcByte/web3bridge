mod error;
mod handlers;
mod middleware;
mod models;
mod state;

use axum::{
    middleware::from_fn,
    routing::{get, post},
    Router,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let state = state::seeded_state();

    let public_routes = Router::new()
        .route("/books", get(handlers::list_books))
        .route("/books/{id}", get(handlers::get_book))
        .route("/search", get(handlers::search_books))
        .route("/health", get(handlers::health));

    let write_routes = Router::new()
        .route("/books", post(handlers::create_book))
        .route(
            "/books/{id}",
            axum::routing::put(handlers::replace_book)
                .patch(handlers::patch_book)
                .delete(handlers::delete_book),
        )
        .layer(from_fn(middleware::require_api_key));

    // log_requests is layered on the fully merged router, on the outside, so
    // it wraps public_routes AND write_routes AND the fallback. If it were
    // layered on public_routes before merging write_routes in, it would only
    // wrap the endpoints public_routes already had at that point — write
    // requests would reach require_api_key and the handlers but skip logging
    // entirely, since merge() combines route tables rather than nesting one
    // router's middleware stack inside the other's.
    let app = Router::new()
        .merge(public_routes)
        .merge(write_routes)
        .fallback(handlers::not_found)
        .layer(from_fn(middleware::log_requests))
        .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
