use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{Method, StatusCode, Uri};
use axum::Json;
use serde::Serialize;

use crate::error::ApiError;
use crate::model::{
    now_rfc3339, validate_author, validate_genre, validate_title, Book, FilterParams, NewBook,
    ReplaceBook, SearchParams, UpdateBook,
};
use crate::{AppState, Store};

const DEFAULT_SEARCH_LIMIT: usize = 10;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    books: usize,
}

pub async fn list_books(
    State(state): State<AppState>,
    query: Result<Query<FilterParams>, QueryRejection>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let Query(filters) = query?;
    let store = lock_store(&state)?;
    let mut books: Vec<Book> = store
        .books
        .values()
        .filter(|book| {
            filters
                .genre
                .as_ref()
                .map(|genre| book.genre.eq_ignore_ascii_case(genre))
                .unwrap_or(true)
        })
        .filter(|book| {
            filters
                .available
                .map(|available| book.available == available)
                .unwrap_or(true)
        })
        .cloned()
        .collect();

    books.sort_by_key(|book| book.id);
    Ok(Json(books))
}

pub async fn get_book(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
) -> Result<Json<Book>, ApiError> {
    let Path(id) = path?;
    let store = lock_store(&state)?;
    let book = store.books.get(&id).ok_or(ApiError::NotFound(id))?;

    Ok(Json(book.clone()))
}

pub async fn search_books(
    State(state): State<AppState>,
    query: Result<Query<SearchParams>, QueryRejection>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let Query(params) = query?;
    let query = params.q.unwrap_or_default().to_lowercase();
    let limit = params.limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
    let store = lock_store(&state)?;
    let mut books: Vec<Book> = store
        .books
        .values()
        .filter(|book| {
            book.title.to_lowercase().contains(&query)
                || book.author.to_lowercase().contains(&query)
        })
        .cloned()
        .collect();

    books.sort_by_key(|book| book.id);
    books.truncate(limit);
    Ok(Json(books))
}

pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, ApiError> {
    let store = lock_store(&state)?;

    Ok(Json(HealthResponse {
        status: "ok",
        books: store.books.len(),
    }))
}

pub async fn create_book(
    State(state): State<AppState>,
    payload: Result<Json<NewBook>, JsonRejection>,
) -> Result<(StatusCode, Json<Book>), ApiError> {
    let Json(payload) = payload?;

    validate_title(&payload.title)?;
    validate_author(&payload.author)?;
    validate_genre(&payload.genre)?;

    let mut store = lock_store(&state)?;
    ensure_unique_title(&store, &payload.title, None)?;

    let id = store.next_id;
    store.next_id += 1;

    let book = Book {
        id,
        title: payload.title,
        author: payload.author,
        genre: payload.genre,
        available: payload.available.unwrap_or(true),
        added_at: now_rfc3339(),
    };

    store.books.insert(id, book.clone());
    Ok((StatusCode::CREATED, Json(book)))
}

pub async fn replace_book(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
    payload: Result<Json<ReplaceBook>, JsonRejection>,
) -> Result<Json<Book>, ApiError> {
    let Path(id) = path?;
    let Json(payload) = payload?;

    validate_title(&payload.title)?;
    validate_author(&payload.author)?;
    validate_genre(&payload.genre)?;

    let mut store = lock_store(&state)?;
    if !store.books.contains_key(&id) {
        return Err(ApiError::NotFound(id));
    }
    ensure_unique_title(&store, &payload.title, Some(id))?;

    let book = store.books.get_mut(&id).ok_or(ApiError::NotFound(id))?;
    book.title = payload.title;
    book.author = payload.author;
    book.genre = payload.genre;
    book.available = payload.available;

    Ok(Json(book.clone()))
}

pub async fn update_book(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
    payload: Result<Json<UpdateBook>, JsonRejection>,
) -> Result<Json<Book>, ApiError> {
    let Path(id) = path?;
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

    let mut store = lock_store(&state)?;
    if !store.books.contains_key(&id) {
        return Err(ApiError::NotFound(id));
    }
    if let Some(title) = &payload.title {
        ensure_unique_title(&store, title, Some(id))?;
    }

    let book = store.books.get_mut(&id).ok_or(ApiError::NotFound(id))?;
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
    path: Result<Path<u64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let Path(id) = path?;
    let mut store = lock_store(&state)?;

    store.books.remove(&id).ok_or(ApiError::NotFound(id))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn not_found(uri: Uri) -> Result<(), ApiError> {
    Err(ApiError::RouteNotFound(uri.path().to_string()))
}

pub async fn method_not_allowed(method: Method, uri: Uri) -> Result<(), ApiError> {
    Err(ApiError::MethodNotAllowed(format!(
        "{method} is not allowed on {}",
        uri.path()
    )))
}

fn lock_store(state: &AppState) -> Result<std::sync::MutexGuard<'_, Store>, ApiError> {
    state
        .store
        .lock()
        .map_err(|error| ApiError::Internal(format!("book store lock was poisoned: {error}")))
}

fn ensure_unique_title(
    store: &Store,
    title: &str,
    ignored_id: Option<u64>,
) -> Result<(), ApiError> {
    let duplicate = store
        .books
        .values()
        .any(|book| book.title == title && Some(book.id) != ignored_id);

    if duplicate {
        return Err(ApiError::Conflict(format!(
            "a book titled \"{title}\" already exists"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn store_with_book() -> Store {
        let book = Book {
            id: 1,
            title: "Existing".to_string(),
            author: "Author".to_string(),
            genre: "Technical".to_string(),
            available: true,
            added_at: "2024-01-01T00:00:00Z".to_string(),
        };

        Store {
            books: HashMap::from([(book.id, book)]),
            next_id: 2,
        }
    }

    #[test]
    fn duplicate_check_can_ignore_the_book_being_updated() {
        let store = store_with_book();

        assert!(ensure_unique_title(&store, "Existing", None).is_err());
        assert!(ensure_unique_title(&store, "Existing", Some(1)).is_ok());
    }
}
