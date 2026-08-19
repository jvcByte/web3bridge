use axum::middleware::from_fn_with_state;
use axum::routing::{get, post, put};
use axum::Router;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub mod domain;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod requests;

use domain::Store;
use handlers::{
    create_book, delete_book, get_book, health, list_books, method_not_allowed, not_found,
    replace_book, search_books, update_book,
};
use middleware::{log_requests, require_api_key};

pub const DEFAULT_API_KEY: &str = "dev-secret-key";

#[derive(Clone)]
pub struct AppState {
    pub(crate) store: Arc<Mutex<Store>>,
    pub(crate) request_counter: Arc<AtomicU64>,
    pub(crate) api_key: Arc<String>,
}

impl AppState {
    pub fn new(api_key: String) -> Self {
        Self {
            store: Arc::new(Mutex::new(Store::seeded())),
            request_counter: Arc::new(AtomicU64::new(0)),
            api_key: Arc::new(api_key),
        }
    }
}

pub fn build_app(state: AppState) -> Router {
    let public_routes = Router::new()
        .route("/books", get(list_books))
        .route("/books/{id}", get(get_book))
        .route("/search", get(search_books))
        .route("/health", get(health));

    let write_routes = Router::new()
        .route("/books", post(create_book))
        .route(
            "/books/{id}",
            put(replace_book).patch(update_book).delete(delete_book),
        )
        .route_layer(from_fn_with_state(state.clone(), require_api_key));

    Router::new()
        .merge(public_routes)
        .merge(write_routes)
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(from_fn_with_state(state.clone(), log_requests))
        .with_state(state)
}
