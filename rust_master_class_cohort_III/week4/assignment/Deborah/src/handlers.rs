use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde_json::json;

use crate::error::ApiError;
use crate::models::{
    validate_author, validate_genre, validate_title, Book, CreateBookRequest, ListParams,
    PatchBookRequest, ReplaceBookRequest, SearchParams,
};
use crate::state::AppState;

fn title_taken(books: &std::collections::BTreeMap<u64, Book>, title: &str, exclude_id: Option<u64>) -> bool {
    books
        .values()
        .any(|b| b.title == title && Some(b.id) != exclude_id)
}

pub async fn list_books(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let store = state.lock()?;
    let books = store
        .books
        .values()
        .filter(|b| params.genre.as_deref().is_none_or(|g| b.genre == g))
        .filter(|b| params.available.is_none_or(|a| b.available == a))
        .cloned()
        .collect();
    Ok(Json(books))
}

pub async fn get_book(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<Book>, ApiError> {
    let store = state.lock()?;
    store
        .books
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))
}

pub async fn search_books(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let q = params.q.unwrap_or_default().to_lowercase();
    let limit = params.limit.unwrap_or(10);

    let store = state.lock()?;
    let books = store
        .books
        .values()
        .filter(|b| b.title.to_lowercase().contains(&q) || b.author.to_lowercase().contains(&q))
        .take(limit)
        .cloned()
        .collect();
    Ok(Json(books))
}

pub async fn health(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let store = state.lock()?;
    Ok(Json(json!({ "status": "ok", "books": store.books.len() })))
}

pub async fn create_book(
    State(state): State<AppState>,
    Json(payload): Json<CreateBookRequest>,
) -> Result<(StatusCode, Json<Book>), ApiError> {
    validate_title(&payload.title)?;
    validate_author(&payload.author)?;
    validate_genre(&payload.genre)?;

    let mut store = state.lock()?;

    if title_taken(&store.books, &payload.title, None) {
        return Err(ApiError::Conflict(format!(
            "a book titled '{}' already exists",
            payload.title
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
        added_at: Utc::now().to_rfc3339(),
    };
    store.books.insert(id, book.clone());

    Ok((StatusCode::CREATED, Json(book)))
}

pub async fn replace_book(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(payload): Json<ReplaceBookRequest>,
) -> Result<Json<Book>, ApiError> {
    validate_title(&payload.title)?;
    validate_author(&payload.author)?;
    validate_genre(&payload.genre)?;

    let mut store = state.lock()?;

    if !store.books.contains_key(&id) {
        return Err(ApiError::NotFound(format!("book {id} not found")));
    }
    if title_taken(&store.books, &payload.title, Some(id)) {
        return Err(ApiError::Conflict(format!(
            "a book titled '{}' already exists",
            payload.title
        )));
    }

    let added_at = store.books.get(&id).unwrap().added_at.clone();
    let book = Book {
        id,
        title: payload.title,
        author: payload.author,
        genre: payload.genre,
        available: payload.available,
        added_at,
    };
    store.books.insert(id, book.clone());

    Ok(Json(book))
}

pub async fn patch_book(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(payload): Json<PatchBookRequest>,
) -> Result<Json<Book>, ApiError> {
    if let Some(title) = &payload.title {
        validate_title(title)?;
    }
    if let Some(author) = &payload.author {
        validate_author(author)?;
    }
    if let Some(genre) = &payload.genre {
        validate_genre(genre)?;
    }

    let mut store = state.lock()?;

    if !store.books.contains_key(&id) {
        return Err(ApiError::NotFound(format!("book {id} not found")));
    }
    if let Some(title) = &payload.title {
        if title_taken(&store.books, title, Some(id)) {
            return Err(ApiError::Conflict(format!(
                "a book titled '{title}' already exists"
            )));
        }
    }

    let book = store.books.get_mut(&id).unwrap();
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

    Ok(Json(book.clone()))
}

pub async fn delete_book(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    let mut store = state.lock()?;
    if store.books.remove(&id).is_none() {
        return Err(ApiError::NotFound(format!("book {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn not_found() -> ApiError {
    ApiError::NotFound("route not found".to_string())
}
