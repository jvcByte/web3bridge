use axum::routing::{get, post, put};
use axum::Router;
use std::collections::HashMap;
use std::env;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

mod book;
mod error;
mod handlers;
mod middleware;

use book::Book;
use handlers::{
    create_book, delete_book, get_book, health, list_books, method_not_allowed, not_found,
    replace_book, search_books, update_book,
};
use middleware::{log_requests, require_api_key, DEFAULT_API_KEY};

pub struct Store {
    pub books: HashMap<u64, Book>,
    pub next_id: u64,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Mutex<Store>>,
    pub requests: Arc<AtomicU64>,
    pub api_key: Arc<String>,
}

#[tokio::main]
async fn main() {
    let api_key = env::var("API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY.to_string());

    let state = AppState {
        store: Arc::new(Mutex::new(seed_store())),
        requests: Arc::new(AtomicU64::new(0)),
        api_key: Arc::new(api_key),
    };

    let public = Router::new()
        .route("/books", get(list_books))
        .route("/books/{id}", get(get_book))
        .route("/search", get(search_books))
        .route("/health", get(health));

    let writes = Router::new()
        .route("/books", post(create_book))
        .route(
            "/books/{id}",
            put(replace_book).patch(update_book).delete(delete_book),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ));

    let app = Router::new()
        .merge(public)
        .merge(writes)
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            log_requests,
        ))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("could not bind to port 3000");

    println!("Book Library API listening on http://localhost:3000");

    axum::serve(listener, app).await.expect("server failed");
}

fn seed_store() -> Store {
    let mut books = HashMap::new();

    books.insert(
        1,
        Book {
            id: 1,
            title: String::from("The Rust Programming Language"),
            author: String::from("Steve Klabnik"),
            genre: String::from("Technical"),
            available: true,
            added_at: book::now_rfc3339(),
        },
    );

    books.insert(
        2,
        Book {
            id: 2,
            title: String::from("Programming Rust"),
            author: String::from("Jim Blandy"),
            genre: String::from("Technical"),
            available: false,
            added_at: book::now_rfc3339(),
        },
    );

    Store { books, next_id: 3 }
}
