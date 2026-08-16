use std::{
    env,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        Path, Query, Request, State,
    },
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use serde::Serialize;
use tokio::net::TcpListener;

mod error;
mod models;
mod store;

use crate::{
    error::ApiError,
    models::{Book, CreateBook, FilterParams, ReplaceBook, SearchParams, UpdateBook},
    store::{SharedState, Store},
};

#[tokio::main]
async fn main() {
    // Create the shared in-memory store.
    let store = Store::seeded();
    let state = Arc::new(Mutex::new(store));

    // These routes are available without an API key.
    let public_routes = Router::new()
        .route("/health", get(health))
        .route("/books", get(get_books))
        .route("/books/{id}", get(get_book))
        .route("/search", get(search_books));

    // These routes require the configured API key.
    let api_key = env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string());
    let write_routes = Router::new()
        .route("/books", post(create_book))
        .route(
            "/books/{id}",
            put(replace_book).patch(update_book).delete(delete_book),
        )
        .route_layer(middleware::from_fn_with_state(api_key, require_api_key));

    // Merge every route before applying the request logger.
    let request_counter = Arc::new(AtomicU64::new(0));
    let app = public_routes
        .merge(write_routes)
        .fallback(route_not_found)
        .layer(middleware::from_fn_with_state(
            request_counter,
            log_requests,
        ))
        .with_state(state);

    // Start the server.
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Return the server status and current book count.
async fn health(State(state): State<SharedState>) -> Result<Json<HealthResponse>, ApiError> {
    let books = {
        let store = state.lock()?;
        store.books.len()
    };

    Ok(Json(HealthResponse {
        status: "ok".to_string(),
        books,
    }))
}

// Return all books after applying optional filters.
async fn get_books(
    State(state): State<SharedState>,
    filters: Result<Query<FilterParams>, QueryRejection>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let filters = extract_query(filters)?;

    let mut books = {
        let store = state.lock()?;
        store.books.values().cloned().collect::<Vec<Book>>()
    };

    if let Some(genre) = filters.genre {
        books.retain(|book| book.genre.eq_ignore_ascii_case(&genre));
    }

    if let Some(available) = filters.available {
        books.retain(|book| book.available == available);
    }

    books.sort_by_key(|book| book.id);
    Ok(Json(books))
}

// Return one book by its id.
async fn get_book(
    State(state): State<SharedState>,
    path: Result<Path<u64>, PathRejection>,
) -> Result<Json<Book>, ApiError> {
    let id = extract_path(path)?;

    let book = {
        let store = state.lock()?;
        store
            .books
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))?
    };

    Ok(Json(book))
}

// Create a new book with server-controlled fields.
async fn create_book(
    State(state): State<SharedState>,
    payload: Result<Json<CreateBook>, JsonRejection>,
) -> Result<(StatusCode, Json<Book>), ApiError> {
    let payload = extract_json(payload)?;
    validate_book_fields(&payload.title, &payload.author, &payload.genre)?;

    let book = {
        let mut store = state.lock()?;

        if store.books.values().any(|book| book.title == payload.title) {
            return Err(ApiError::Conflict(
                "a book with this title already exists".to_string(),
            ));
        }

        let id = store.next_id;
        let book = Book {
            id,
            title: payload.title,
            author: payload.author,
            genre: payload.genre,
            available: true,
            added_at: chrono::Utc::now().to_rfc3339(),
        };

        store.books.insert(id, book.clone());
        store.next_id += 1;
        book
    };

    Ok((StatusCode::CREATED, Json(book)))
}

// Delete one book by its id.
async fn delete_book(
    State(state): State<SharedState>,
    path: Result<Path<u64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let id = extract_path(path)?;

    {
        let mut store = state.lock()?;
        store
            .books
            .remove(&id)
            .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))?;
    }

    Ok(StatusCode::NO_CONTENT)
}

// Replace every editable field on a book.
async fn replace_book(
    State(state): State<SharedState>,
    path: Result<Path<u64>, PathRejection>,
    payload: Result<Json<ReplaceBook>, JsonRejection>,
) -> Result<Json<Book>, ApiError> {
    let id = extract_path(path)?;
    let payload = extract_json(payload)?;
    validate_book_fields(&payload.title, &payload.author, &payload.genre)?;

    let book = {
        let mut store = state.lock()?;

        let existing_book = store
            .books
            .get(&id)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))?;

        if store
            .books
            .values()
            .any(|book| book.id != id && book.title == payload.title)
        {
            return Err(ApiError::Conflict(
                "a book with this title already exists".to_string(),
            ));
        }

        let new_book = Book {
            id: existing_book.id,
            title: payload.title,
            author: payload.author,
            genre: payload.genre,
            available: payload.available,
            added_at: existing_book.added_at,
        };

        store.books.insert(id, new_book.clone());
        new_book
    };

    Ok(Json(book))
}

// Update only the fields supplied by the client.
async fn update_book(
    State(state): State<SharedState>,
    path: Result<Path<u64>, PathRejection>,
    payload: Result<Json<UpdateBook>, JsonRejection>,
) -> Result<Json<Book>, ApiError> {
    let id = extract_path(path)?;
    let payload = extract_json(payload)?;

    if let Some(title) = payload.title.as_ref() {
        validate_title(title)?;
    }
    if let Some(author) = payload.author.as_ref() {
        validate_author(author)?;
    }
    if let Some(genre) = payload.genre.as_ref() {
        validate_genre(genre)?;
    }

    let updated_book = {
        let mut store = state.lock()?;

        if !store.books.contains_key(&id) {
            return Err(ApiError::NotFound(format!("book {id} not found")));
        }

        if let Some(title) = payload.title.as_ref() {
            if store
                .books
                .values()
                .any(|book| book.id != id && book.title == *title)
            {
                return Err(ApiError::Conflict(
                    "a book with this title already exists".to_string(),
                ));
            }
        }

        let book = store
            .books
            .get_mut(&id)
            .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))?;

        if let Some(title) = payload.title {
            book.title = title;
        }
        if let Some(author) = payload.author {
            book.author = author;
        }
        if let Some(genre) = payload.genre {
            book.genre = genre;
        }
        if let Some(available) = payload.available {
            book.available = available;
        }

        book.clone()
    };

    Ok(Json(updated_book))
}

// Search titles and authors without case sensitivity.
async fn search_books(
    State(state): State<SharedState>,
    params: Result<Query<SearchParams>, QueryRejection>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let params = extract_query(params)?;

    if params.q.trim().is_empty() {
        return Err(ApiError::ValidationFailed(
            "search query must not be empty".to_string(),
        ));
    }

    let query = params.q.to_lowercase();
    let limit = params.limit.unwrap_or(10);

    let mut books = {
        let store = state.lock()?;
        store.books.values().cloned().collect::<Vec<Book>>()
    };

    books.retain(|book| {
        book.title.to_lowercase().contains(&query) || book.author.to_lowercase().contains(&query)
    });
    books.sort_by_key(|book| book.id);
    books.truncate(limit);

    Ok(Json(books))
}

// Require the API key on write routes.
async fn require_api_key(
    State(expected_key): State<String>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let provided_key = request
        .headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if !constant_time_equal(provided_key.as_bytes(), expected_key.as_bytes()) {
        return Err(ApiError::Unauthorized(
            "missing or invalid API key".to_string(),
        ));
    }

    Ok(next.run(request).await)
}

// Log every request after its response is ready.
async fn log_requests(
    State(counter): State<Arc<AtomicU64>>,
    request: Request,
    next: Next,
) -> Response {
    let request_number = counter.fetch_add(1, Ordering::Relaxed) + 1;
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let started_at = Instant::now();

    let response = next.run(request).await;
    let status = response.status().as_u16();
    let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;

    println!(
        "[req {:>4}] {:<6} {:<24} -> {} ({:.2}ms)",
        request_number, method, path, status, elapsed_ms
    );

    response
}

// Return the normal JSON error for unknown routes.
async fn route_not_found(request: Request) -> Result<StatusCode, ApiError> {
    Err(ApiError::NotFound(format!(
        "route {} not found",
        request.uri().path()
    )))
}

// Convert extractor failures into the normal JSON error shape.
fn extract_json<T>(payload: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    let Json(payload) = payload.map_err(|error| ApiError::ValidationFailed(error.to_string()))?;
    Ok(payload)
}

fn extract_query<T>(params: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    let Query(params) = params.map_err(|error| ApiError::ValidationFailed(error.to_string()))?;
    Ok(params)
}

fn extract_path(path: Result<Path<u64>, PathRejection>) -> Result<u64, ApiError> {
    let Path(id) = path.map_err(|error| ApiError::ValidationFailed(error.to_string()))?;
    Ok(id)
}

// Validate all required book fields.
fn validate_book_fields(title: &str, author: &str, genre: &str) -> Result<(), ApiError> {
    validate_title(title)?;
    validate_author(author)?;
    validate_genre(genre)?;
    Ok(())
}

fn validate_title(title: &str) -> Result<(), ApiError> {
    if title.trim().is_empty() {
        return Err(ApiError::ValidationFailed(
            "title must not be empty".to_string(),
        ));
    }

    if title.chars().count() > 150 {
        return Err(ApiError::ValidationFailed(
            "title must not exceed 150 characters".to_string(),
        ));
    }

    Ok(())
}

fn validate_author(author: &str) -> Result<(), ApiError> {
    if author.trim().is_empty() {
        return Err(ApiError::ValidationFailed(
            "author must not be empty".to_string(),
        ));
    }

    Ok(())
}

fn validate_genre(genre: &str) -> Result<(), ApiError> {
    if genre.trim().is_empty() {
        return Err(ApiError::ValidationFailed(
            "genre must not be empty".to_string(),
        ));
    }

    Ok(())
}

// Compare API keys without returning early on different bytes.
fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let max_length = left.len().max(right.len());
    let mut difference = left.len() ^ right.len();

    for index in 0..max_length {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }

    difference == 0
}

#[derive(Debug, Clone, Serialize)]
struct HealthResponse {
    status: String,
    books: usize,
}
