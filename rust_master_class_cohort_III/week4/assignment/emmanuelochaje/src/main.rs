use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize, Deserialize)]
struct Book {
    id: u32,
    title: String,
    author: String,
}

type BookStore = Arc<Mutex<Vec<Book>>>;

#[tokio::main]
async fn main() {
    // Simple in-memory storage
    let books: BookStore = Arc::new(Mutex::new(Vec::new()));

    let app = Router::new()
        .route("/", get(home))
        .route("/books", get(get_books).post(add_book))
        .route(
            "/books/{id}",
            get(get_book).put(update_book).delete(delete_book),
        )
        .with_state(books);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    println!("Server running on http://127.0.0.1:3000");

    axum::serve(listener, app).await.unwrap();
}

// Home route
async fn home() -> &'static str {
    "Book Library API"
}

// GET /books
async fn get_books(State(books): State<BookStore>) -> Json<Vec<Book>> {
    let books = books.lock().unwrap();

    Json(books.clone())
}

// GET /books/:id
async fn get_book(
    Path(id): Path<u32>,
    State(books): State<BookStore>,
) -> Result<Json<Book>, StatusCode> {
    let books = books.lock().unwrap();

    let book = books.iter().find(|book| book.id == id);

    match book {
        Some(book) => Ok(Json(book.clone())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

// POST /books
async fn add_book(
    State(books): State<BookStore>,
    Json(mut book): Json<Book>,
) -> (StatusCode, Json<Book>) {
    let mut books = books.lock().unwrap();

    // Give the book an ID automatically
    book.id = books.len() as u32 + 1;

    books.push(book.clone());

    (StatusCode::CREATED, Json(book))
}

// PUT /books/:id
async fn update_book(
    Path(id): Path<u32>,
    State(books): State<BookStore>,
    Json(new_book): Json<Book>,
) -> Result<Json<Book>, StatusCode> {
    let mut books = books.lock().unwrap();

    let book = books.iter_mut().find(|book| book.id == id);

    match book {
        Some(book) => {
            book.title = new_book.title;
            book.author = new_book.author;

            Ok(Json(book.clone()))
        }

        None => Err(StatusCode::NOT_FOUND),
    }
}

// DELETE /books/:id
async fn delete_book(
    Path(id): Path<u32>,
    State(books): State<BookStore>,
) -> StatusCode {
    let mut books = books.lock().unwrap();

    let old_length = books.len();

    books.retain(|book| book.id != id);

    if books.len() < old_length {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}