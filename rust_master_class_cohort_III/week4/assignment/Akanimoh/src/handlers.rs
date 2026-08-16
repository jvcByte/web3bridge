use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::book::{
    validate_author, validate_genre, validate_title, Book, FilterParams, NewBook, ReplaceBook,
    SearchParams, UpdateBook,
};
use crate::error::ApiError;
use crate::{AppState, Store};

const DEFAULT_SEARCH_LIMIT: usize = 10;

pub async fn list_books(
    State(state): State<AppState>,
    Query(filters): Query<FilterParams>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let store = lock(&state)?;

    let mut books: Vec<Book> = store
        .books
        .values()
        .filter(|book| match &filters.genre {
            Some(genre) => book.genre.eq_ignore_ascii_case(genre),
            None => true,
        })
        .filter(|book| match filters.available {
            Some(available) => book.available == available,
            None => true,
        })
        .cloned()
        .collect();

    books.sort_by_key(|book| book.id);

    Ok(Json(books))
}

pub async fn get_book(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Book>, ApiError> {
    let store = lock(&state)?;
    let book = store.books.get(&id).ok_or(ApiError::NotFound(id))?;

    Ok(Json(book.clone()))
}

pub async fn search_books(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let needle = params.q.unwrap_or_default().to_lowercase();
    let limit = params.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
    let store = lock(&state)?;

    let mut books: Vec<Book> = store
        .books
        .values()
        .filter(|book| {
            book.title.to_lowercase().contains(&needle)
                || book.author.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();

    books.sort_by_key(|book| book.id);
    books.truncate(limit);

    Ok(Json(books))
}

pub async fn health(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let store = lock(&state)?;

    Ok(Json(json!({ "status": "ok", "books": store.books.len() })))
}

pub async fn create_book(
    State(state): State<AppState>,
    payload: Result<Json<NewBook>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<Book>), ApiError> {
    let Json(payload) = payload?;

    validate_title(&payload.title)?;
    validate_author(&payload.author)?;
    validate_genre(&payload.genre)?;

    let mut store = lock(&state)?;

    if let Some(clash) = find_by_title(&store, &payload.title, None) {
        return Err(ApiError::Conflict(format!(
            "a book titled \"{}\" already exists with id {}",
            payload.title, clash
        )));
    }

    let id = store.next_id;
    store.next_id += 1;

    let book = Book {
        id,
        title: payload.title,
        author: payload.author,
        genre: payload.genre,
        available: payload.available.unwrap_or(true),
        added_at: crate::book::now_rfc3339(),
    };

    store.books.insert(id, book.clone());

    Ok((StatusCode::CREATED, Json(book)))
}

pub async fn replace_book(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    payload: Result<Json<ReplaceBook>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Book>, ApiError> {
    let Json(payload) = payload?;

    validate_title(&payload.title)?;
    validate_author(&payload.author)?;
    validate_genre(&payload.genre)?;

    let mut store = lock(&state)?;

    if !store.books.contains_key(&id) {
        return Err(ApiError::NotFound(id));
    }

    if let Some(clash) = find_by_title(&store, &payload.title, Some(id)) {
        return Err(ApiError::Conflict(format!(
            "a book titled \"{}\" already exists with id {}",
            payload.title, clash
        )));
    }

    let existing = store.books.get_mut(&id).ok_or(ApiError::NotFound(id))?;

    existing.title = payload.title;
    existing.author = payload.author;
    existing.genre = payload.genre;
    existing.available = payload.available.unwrap_or(true);

    Ok(Json(existing.clone()))
}

pub async fn update_book(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    payload: Result<Json<UpdateBook>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Book>, ApiError> {
    let Json(payload) = payload?;

    if let Some(title) = &payload.title {
        validate_title(title)?;
    }
    if let Some(author) = &payload.author {
        validate_author(author)?;
    }
    if let Some(genre) = &payload.genre {
        validate_genre(genre)?;
    }

    let mut store = lock(&state)?;

    if !store.books.contains_key(&id) {
        return Err(ApiError::NotFound(id));
    }

    if let Some(title) = &payload.title {
        if let Some(clash) = find_by_title(&store, title, Some(id)) {
            return Err(ApiError::Conflict(format!(
                "a book titled \"{}\" already exists with id {}",
                title, clash
            )));
        }
    }

    let existing = store.books.get_mut(&id).ok_or(ApiError::NotFound(id))?;

    if let Some(title) = payload.title {
        existing.title = title;
    }
    if let Some(author) = payload.author {
        existing.author = author;
    }
    if let Some(genre) = payload.genre {
        existing.genre = genre;
    }
    if let Some(available) = payload.available {
        existing.available = available;
    }

    Ok(Json(existing.clone()))
}

pub async fn delete_book(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    let mut store = lock(&state)?;

    store.books.remove(&id).ok_or(ApiError::NotFound(id))?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn not_found(uri: axum::http::Uri) -> ApiError {
    ApiError::NoRoute(format!("no route for {}", uri.path()))
}

pub async fn method_not_allowed(method: axum::http::Method, uri: axum::http::Uri) -> ApiError {
    ApiError::MethodNotAllowed(format!("{} is not allowed on {}", method, uri.path()))
}

fn lock(state: &AppState) -> Result<std::sync::MutexGuard<'_, Store>, ApiError> {
    state
        .store
        .lock()
        .map_err(|e| ApiError::Internal(format!("store lock poisoned: {}", e)))
}

fn find_by_title(store: &Store, title: &str, skip_id: Option<u64>) -> Option<u64> {
    store
        .books
        .values()
        .find(|book| book.title == title && Some(book.id) != skip_id)
        .map(|book| book.id)
}
