use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::error::ApiError;
use crate::AppState;

pub const API_KEY_HEADER: &str = "x-api-key";
pub const DEFAULT_API_KEY: &str = "dev-secret-key";

pub async fn log_requests(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let number = state.requests.fetch_add(1, Ordering::Relaxed) + 1;
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    let started = Instant::now();
    let response = next.run(request).await;
    let elapsed = started.elapsed();

    println!(
        "[req {:>4}] {:<6} {:<24} -> {} ({:.2}ms)",
        number,
        method.as_str(),
        path,
        response.status().as_u16(),
        elapsed.as_secs_f64() * 1000.0
    );

    response
}

pub async fn require_api_key(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let supplied = request
        .headers()
        .get(API_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");

    if !keys_match(supplied.as_bytes(), state.api_key.as_bytes()) {
        return ApiError::Unauthorized.into_response();
    }

    next.run(request).await
}

fn keys_match(supplied: &[u8], expected: &[u8]) -> bool {
    if supplied.len() != expected.len() {
        return false;
    }

    let mut difference = 0u8;
    for i in 0..expected.len() {
        difference |= supplied[i] ^ expected[i];
    }

    difference == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_exact_key() {
        assert!(keys_match(b"dev-secret-key", b"dev-secret-key"));

        assert!(!keys_match(b"", b"dev-secret-key"));
        assert!(!keys_match(b"wrong", b"dev-secret-key"));
        assert!(!keys_match(b"dev-secret-ke", b"dev-secret-key"));
        assert!(!keys_match(b"dev-secret-keyy", b"dev-secret-key"));
        assert!(!keys_match(b"Dev-secret-key", b"dev-secret-key"));
    }
}
