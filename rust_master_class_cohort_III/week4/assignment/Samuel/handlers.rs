use axum::{
    extract::{Json, Path, Query, State},
    http::{StatusCode, Uri},
    response::IntoResponse,
};

use crate::error::ApiError;
use crate::models::*;
use crate::state::{SharedState, now_rfc3339};

fn validate_title(title: &str) -> Result<(), ApiError> {
    if title.is_empty() || title.len() > 150 {
        return Err(ApiError::ValidationFailed("title must be 1-150 characters".into()));
    }
    Ok(())
}

fn validate_not_empty(field: &str, name: &str) -> Result<(), ApiError> {
    if field.is_empty() {
        return Err(ApiError::ValidationFailed(format!("{name} must not be empty")));
    }
    Ok(())
}

fn check_duplicate_title(
    state: &crate::state::BookStore,
    title: &str,
    exclude_id: Option<u64>,
) -> Result<(), ApiError> {
    let exists = state.books.values().any(|b| {
        exclude_id.map_or(true, |eid| b.id != eid) && b.title.eq_ignore_ascii_case(title)
    });
    if exists {
        return Err(ApiError::Conflict(format!("a book with title '{title}' already exists")));
    }
    Ok(())
}

pub async fn health(State(state): State<SharedState>) -> Result<impl IntoResponse, ApiError> {
    let count = state.store.lock().unwrap().books.len();
    Ok(Json(serde_json::json!({ "status": "ok", "books": count })))
}

pub async fn list_books(
    State(state): State<SharedState>,
    Query(filters): Query<BookFilters>,
) -> Result<impl IntoResponse, ApiError> {
    let store = state.store.lock().unwrap();
    let mut books: Vec<&Book> = store.books.values().collect();

    if let Some(ref genre) = filters.genre {
        books.retain(|b| b.genre.eq_ignore_ascii_case(genre));
    }
    if let Some(available) = filters.available {
        books.retain(|b| b.available == available);
    }

    books.sort_by_key(|b| b.id);
    let books: Vec<Book> = books.into_iter().cloned().collect();
    Ok(Json(books))
}

pub async fn get_book(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> Result<impl IntoResponse, ApiError> {
    let store = state.store.lock().unwrap();
    let book = store.books.get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))?;
    Ok(Json(book.clone()))
}

pub async fn search_books(
    State(state): State<SharedState>,
    Query(params): Query<SearchParams>,
) -> Result<impl IntoResponse, ApiError> {
    let store = state.store.lock().unwrap();
    let query = params.q.unwrap_or_default().to_lowercase();
    let limit = params.limit.unwrap_or(10);

    let mut results: Vec<&Book> = store.books.values()
        .filter(|b| {
            b.title.to_lowercase().contains(&query)
                || b.author.to_lowercase().contains(&query)
        })
        .collect();

    results.sort_by_key(|b| b.id);
    let results: Vec<Book> = results.into_iter().take(limit).cloned().collect();
    Ok(Json(results))
}

pub async fn create_book(
    State(state): State<SharedState>,
    Json(input): Json<CreateBook>,
) -> Result<impl IntoResponse, ApiError> {
    validate_title(&input.title)?;
    validate_not_empty(&input.author, "author")?;
    validate_not_empty(&input.genre, "genre")?;

    let mut store = state.store.lock().unwrap();
    check_duplicate_title(&store, &input.title, None)?;

    let id = store.next_id;
    store.next_id += 1;

    let book = Book {
        id,
        title: input.title,
        author: input.author,
        genre: input.genre,
        available: input.available.unwrap_or(true),
        added_at: now_rfc3339(),
    };

    store.books.insert(id, book.clone());
    Ok((StatusCode::CREATED, Json(book)))
}

pub async fn update_book(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
    Json(input): Json<UpdateBook>,
) -> Result<impl IntoResponse, ApiError> {
    validate_title(&input.title)?;
    validate_not_empty(&input.author, "author")?;
    validate_not_empty(&input.genre, "genre")?;

    let mut store = state.store.lock().unwrap();
    let added_at = store.books.get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))?
        .added_at.clone();

    check_duplicate_title(&store, &input.title, Some(id))?;

    let updated = Book {
        id,
        title: input.title,
        author: input.author,
        genre: input.genre,
        available: input.available,
        added_at,
    };

    store.books.insert(id, updated.clone());
    Ok(Json(updated))
}

pub async fn patch_book(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
    Json(input): Json<PatchBook>,
) -> Result<impl IntoResponse, ApiError> {
    let mut store = state.store.lock().unwrap();
    let book = store.books.get(&id)
        .ok_or_else(|| ApiError::NotFound(format!("book {id} not found")))?
        .clone();

    if let Some(ref title) = input.title {
        validate_title(title)?;
        check_duplicate_title(&store, title, Some(id))?;
    }
    if let Some(ref author) = input.author {
        validate_not_empty(author, "author")?;
    }
    if let Some(ref genre) = input.genre {
        validate_not_empty(genre, "genre")?;
    }

    let patched = Book {
        id: book.id,
        title: input.title.unwrap_or(book.title),
        author: input.author.unwrap_or(book.author),
        genre: input.genre.unwrap_or(book.genre),
        available: input.available.unwrap_or(book.available),
        added_at: book.added_at,
    };

    store.books.insert(id, patched.clone());
    Ok(Json(patched))
}

pub async fn delete_book(
    State(state): State<SharedState>,
    Path(id): Path<u64>,
) -> Result<impl IntoResponse, ApiError> {
    let mut store = state.store.lock().unwrap();
    if store.books.remove(&id).is_none() {
        return Err(ApiError::NotFound(format!("book {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn fallback(uri: Uri) -> impl IntoResponse {
    ApiError::NotFound(format!("route {} not found", uri.path()))
}
