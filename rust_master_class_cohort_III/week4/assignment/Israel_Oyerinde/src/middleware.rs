use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::error::ApiError;
use crate::AppState;

const API_KEY_HEADER: &str = "x-api-key";

pub async fn require_api_key(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let supplied_key = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if !constant_time_eq(supplied_key.as_bytes(), state.api_key.as_bytes()) {
        return ApiError::Unauthorized.into_response();
    }

    next.run(request).await
}

pub async fn log_requests(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let request_number = state.request_counter.fetch_add(1, Ordering::Relaxed) + 1;
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let started_at = Instant::now();

    let response = next.run(request).await;
    let elapsed_ms = started_at.elapsed().as_secs_f64() * 1_000.0;

    println!(
        "[req {:>4}] {:<6} {:<24} -> {} ({elapsed_ms:.2}ms)",
        request_number,
        method.as_str(),
        path,
        response.status().as_u16()
    );

    response
}

fn constant_time_eq(supplied: &[u8], expected: &[u8]) -> bool {
    let mut difference = supplied.len() ^ expected.len();

    for (index, expected_byte) in expected.iter().enumerate() {
        let supplied_byte = supplied.get(index).copied().unwrap_or_default();
        difference |= usize::from(supplied_byte ^ expected_byte);
    }

    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_keys_must_match_exactly() {
        assert!(constant_time_eq(b"dev-secret-key", b"dev-secret-key"));
        assert!(!constant_time_eq(b"wrong", b"dev-secret-key"));
        assert!(!constant_time_eq(b"dev-secret-ke", b"dev-secret-key"));
        assert!(!constant_time_eq(
            b"dev-secret-key-extra",
            b"dev-secret-key"
        ));
        assert!(!constant_time_eq(b"Dev-secret-key", b"dev-secret-key"));
    }
}
