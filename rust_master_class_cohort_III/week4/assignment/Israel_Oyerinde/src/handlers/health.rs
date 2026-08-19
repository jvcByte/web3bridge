use axum::extract::State;
use axum::Json;
use serde::Serialize;

use super::lock_store;
use crate::error::ApiError;
use crate::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    books: usize,
}

pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, ApiError> {
    let store = lock_store(&state)?;

    Ok(Json(HealthResponse {
        status: "ok",
        books: store.book_count(),
    }))
}
