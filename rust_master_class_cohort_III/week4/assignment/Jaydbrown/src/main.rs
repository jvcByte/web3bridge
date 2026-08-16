use axum::{
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Book {
    id: u64,
    title: String,
    author: String,
    genre: String,
    available: bool,
    added_at: String,
}

#[derive(Debug, Deserialize)]
struct NewBook {
    title: String,
    author: String,
    genre: String,
}

#[derive(Debug, Deserialize)]
struct PutBook {
    title: String,
    author: String,
    genre: String,
    available: bool,
}

#[derive(Debug, Deserialize)]
struct PatchBook {
    title: Option<String>,
    author: Option<String>,
    genre: Option<String>,
    available: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FilterParams {
    genre: Option<String>,
    available: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: Option<String>,
    limit: Option<usize>,
}

fn validate_title(title: &str) -> Result<(), ApiError> {
    if title.trim().is_empty() {
        return Err(ApiError::Validation("title must not be empty".to_string()));
    }
    if title.chars().count() > 150 {
        return Err(ApiError::Validation(
            "title must be at most 150 characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_nonempty(field: &str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::Validation(format!("{field} must not be empty")));
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("book {0} not found")]
    NotFound(u64),
    #[error("route not found")]
    RouteNotFound,
    #[error("{0}")]
    Validation(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("{0}")]
    Conflict(String),
    #[error("internal error")]
    Internal(String),
}

impl<T> From<std::sync::PoisonError<T>> for ApiError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        ApiError::Internal("shared state lock was poisoned".to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, kind, message) = match self {
            ApiError::NotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("book {id} not found"),
            ),
            ApiError::RouteNotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "route not found".to_string(),
            ),
            ApiError::Validation(message) => (StatusCode::BAD_REQUEST, "validation_failed", message),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "missing or invalid API key".to_string(),
            ),
            ApiError::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            ApiError::Internal(message) => {
                eprintln!("internal error: {message}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_string(),
                )
            }
        };

        let body = Json(json!({
            "error": { "kind": kind, "message": message }
        }));
        (status, body).into_response()
    }
}

fn now_rfc3339() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs();
    let millis = since_epoch.subsec_millis();
    let days = (secs / 86_400) as i64;
    let time_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3600;
    let minute = (time_of_day % 3600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

struct Store {
    books: HashMap<u64, Book>,
    next_id: u64,
}

type SharedStore = Arc<Mutex<Store>>;

fn seed_store() -> Store {
    let mut books = HashMap::new();
    books.insert(
        1,
        Book {
            id: 1,
            title: "The Rust Programming Language".to_string(),
            author: "Steve Klabnik".to_string(),
            genre: "Technical".to_string(),
            available: true,
            added_at: now_rfc3339(),
        },
    );
    books.insert(
        2,
        Book {
            id: 2,
            title: "Programming Rust".to_string(),
            author: "Jim Blandy".to_string(),
            genre: "Technical".to_string(),
            available: false,
            added_at: now_rfc3339(),
        },
    );
    Store { books, next_id: 3 }
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
}
