use axum::middleware::from_fn_with_state;
use axum::routing::{get, post, put};
use axum::Router;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub mod error;
pub mod handlers;
pub mod middleware;
pub mod model;

use handlers::{
    create_book, delete_book, get_book, health, list_books, method_not_allowed, not_found,
    replace_book, search_books, update_book,
};
use middleware::{log_requests, require_api_key};
use model::{now_rfc3339, Book};

pub const DEFAULT_API_KEY: &str = "dev-secret-key";

pub struct Store {
    pub books: HashMap<u64, Book>,
    pub next_id: u64,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<Store>>,
    pub request_counter: Arc<AtomicU64>,
    pub api_key: Arc<String>,
}

impl AppState {
    pub fn new(api_key: String) -> Self {
        Self {
            store: Arc::new(Mutex::new(seed_store())),
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

fn seed_store() -> Store {
    let added_at = now_rfc3339();
    let mut books = HashMap::new();

    books.insert(
        1,
        Book {
            id: 1,
            title: "The Rust Programming Language".to_string(),
            author: "Steve Klabnik".to_string(),
            genre: "Technical".to_string(),
            available: true,
            added_at: added_at.clone(),
        },
    );
    books.insert(
        2,
        Book {
            id: 2,
            title: "Programming Rust".to_string(),
            author: "Jim Blandy".to_string(),
            genre: "Technical".to_string(),
            available: false,
            added_at,
        },
    );

    Store { books, next_id: 3 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_data_and_next_id_match_the_assignment() {
        let store = seed_store();

        assert_eq!(store.books.len(), 2);
        assert_eq!(store.next_id, 3);
        assert_eq!(
            store.books.get(&1).map(|book| book.title.as_str()),
            Some("The Rust Programming Language")
        );
        assert_eq!(store.books.get(&2).map(|book| book.available), Some(false));
    }
}
