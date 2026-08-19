use serde::{Deserialize, Serialize};

use crate::error::ApiError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub id: u64,
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: bool,
    pub added_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateBookRequest {
    pub title: String,
    pub author: String,
    pub genre: String,
    #[serde(default)]
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceBookRequest {
    pub title: String,
    pub author: String,
    pub genre: String,
    pub available: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct PatchBookRequest {
    pub title: Option<String>,
    pub author: Option<String>,
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub genre: Option<String>,
    pub available: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

pub fn validate_title(title: &str) -> Result<(), ApiError> {
    if title.is_empty() {
        return Err(ApiError::Validation("title must not be empty".into()));
    }
    if title.chars().count() > 150 {
        return Err(ApiError::Validation(
            "title must be at most 150 characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_author(author: &str) -> Result<(), ApiError> {
    if author.is_empty() {
        return Err(ApiError::Validation("author must not be empty".into()));
    }
    Ok(())
}

pub fn validate_genre(genre: &str) -> Result<(), ApiError> {
    if genre.is_empty() {
        return Err(ApiError::Validation("genre must not be empty".into()));
    }
    Ok(())
}
