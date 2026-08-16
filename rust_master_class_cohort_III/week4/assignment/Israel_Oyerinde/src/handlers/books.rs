use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;

use super::lock_store;
use crate::domain::Book;
use crate::error::ApiError;
use crate::requests::{
    CreateBookRequest, ListBooksQuery, ReplaceBookRequest, SearchBooksQuery, UpdateBookRequest,
};
use crate::AppState;

pub async fn list_books(
    State(state): State<AppState>,
    query: Result<Query<ListBooksQuery>, QueryRejection>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let Query(query) = query?;
    let store = lock_store(&state)?;
    Ok(Json(store.list_books(&query)))
}

pub async fn get_book(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
) -> Result<Json<Book>, ApiError> {
    let Path(id) = path?;
    let store = lock_store(&state)?;
    Ok(Json(store.get_book(id)?))
}

pub async fn search_books(
    State(state): State<AppState>,
    query: Result<Query<SearchBooksQuery>, QueryRejection>,
) -> Result<Json<Vec<Book>>, ApiError> {
    let Query(query) = query?;
    let store = lock_store(&state)?;
    Ok(Json(store.search_books(&query)))
}

pub async fn create_book(
    State(state): State<AppState>,
    payload: Result<Json<CreateBookRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<Book>), ApiError> {
    let Json(payload) = payload?;
    let mut store = lock_store(&state)?;
    let book = store.create_book(payload)?;

    Ok((StatusCode::CREATED, Json(book)))
}

pub async fn replace_book(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
    payload: Result<Json<ReplaceBookRequest>, JsonRejection>,
) -> Result<Json<Book>, ApiError> {
    let Path(id) = path?;
    let Json(request) = payload?;
    let mut store = lock_store(&state)?;

    Ok(Json(store.replace_book(id, request)?))
}

pub async fn update_book(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
    payload: Result<Json<UpdateBookRequest>, JsonRejection>,
) -> Result<Json<Book>, ApiError> {
    let Path(id) = path?;
    let Json(request) = payload?;
    let mut store = lock_store(&state)?;

    Ok(Json(store.update_book(id, request)?))
}

pub async fn delete_book(
    State(state): State<AppState>,
    path: Result<Path<u64>, PathRejection>,
) -> Result<StatusCode, ApiError> {
    let Path(id) = path?;
    let mut store = lock_store(&state)?;
    store.delete_book(id)?;

    Ok(StatusCode::NO_CONTENT)
}
