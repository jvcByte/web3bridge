//! Week 4 · Day 5 — Blog API integration tests.
//!
//! These are the acceptance criteria for the Week 4 deliverable, executable.
//!
//! No server is started and no port is bound. `oneshot` drives the `Router`
//! value directly, so every test gets a fresh, isolated `AppState` and the whole
//! suite runs in parallel in milliseconds.
//!
//! Run with: `cargo test`

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use day5_blog_api::{app, AppState};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

const KEY: &str = "test-key";

/// One request against a fresh app. Returns status and parsed body.
async fn send(req: Request<Body>) -> (StatusCode, Value) {
    let state = Arc::new(AppState::new(KEY));
    send_to(&app(state), req).await
}

/// One request against an existing app, so a test can do several in sequence and
/// keep the state between them.
async fn send_to(router: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(req).await.expect("router failed");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();

    // 204 and friends have no body; represent that as JSON null rather than
    // failing to parse.
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, body)
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn authed(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-api-key", KEY)
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn unauthed(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Reads are public
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_reports_seeded_post_count() {
    let (status, body) = send(get("/health")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["posts"], 2);
}

#[tokio::test]
async fn about_exposes_version() {
    let (status, body) = send(get("/about")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "day5-blog-api");
}

#[tokio::test]
async fn list_returns_seeded_posts_in_id_order() {
    let (status, body) = send(get("/posts")).await;
    assert_eq!(status, StatusCode::OK);

    let posts = body.as_array().unwrap();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0]["id"], 1);
    assert_eq!(posts[1]["id"], 2);
}

#[tokio::test]
async fn fields_are_camel_case_on_the_wire() {
    let (_, body) = send(get("/posts/1")).await;
    // `created_at` in Rust, `createdAt` in JSON — serde decoupling the two.
    assert!(body.get("createdAt").is_some());
    assert!(body.get("created_at").is_none());
}

#[tokio::test]
async fn get_missing_post_is_404_with_json_body() {
    let (status, body) = send(get("/posts/999")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["kind"], "not_found");
}

#[tokio::test]
async fn unparseable_id_is_rejected_by_the_extractor() {
    // Axum's Path extractor fails before any handler code runs.
    let (status, _) = send(get("/posts/not-a-number")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_route_is_404_with_json_body() {
    let (status, body) = send(get("/does-not-exist")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["kind"], "not_found");
}

// ---------------------------------------------------------------------------
// Filtering and pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn filters_by_author_case_insensitively() {
    let (status, body) = send(get("/posts?author=ada")).await;
    assert_eq!(status, StatusCode::OK);

    let posts = body.as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["author"], "Ada");
}

#[tokio::test]
async fn full_text_filter_matches_body() {
    let (_, body) = send(get("/posts?q=interleaved")).await;
    assert_eq!(body.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn limit_and_offset_paginate() {
    let (_, body) = send(get("/posts?limit=1")).await;
    assert_eq!(body.as_array().unwrap().len(), 1);

    let (_, body) = send(get("/posts?limit=1&offset=1")).await;
    let posts = body.as_array().unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["id"], 2);

    let (_, body) = send(get("/posts?offset=99")).await;
    assert!(body.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn oversized_limit_is_clamped_not_rejected() {
    let (status, _) = send(get("/posts?limit=100000")).await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

#[tokio::test]
async fn writes_without_a_key_are_401() {
    let (status, body) =
        send(unauthed("POST", "/posts", json!({"title":"a","body":"b"}))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["kind"], "unauthorized");
}

#[tokio::test]
async fn a_wrong_key_is_401() {
    let req = Request::builder()
        .method("POST")
        .uri("/posts")
        .header("content-type", "application/json")
        .header("x-api-key", "wrong-key-xx")
        .body(Body::from(json!({"title":"a","body":"b"}).to_string()))
        .unwrap();

    let (status, _) = send(req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_key_prefix_is_not_accepted() {
    let req = Request::builder()
        .method("POST")
        .uri("/posts")
        .header("content-type", "application/json")
        .header("x-api-key", "test")
        .body(Body::from(json!({"title":"a","body":"b"}).to_string()))
        .unwrap();

    let (status, _) = send(req).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn unknown_routes_are_404_not_401() {
    // Regression test for `route_layer` vs `layer`. With plain `layer`, an
    // unauthenticated request to a nonexistent path would return 401 and thereby
    // reveal nothing exists there — inconsistent, and a small information leak.
    let (status, _) = send(unauthed("POST", "/nope", json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn empty_title_is_400() {
    let (status, body) =
        send(authed("POST", "/posts", json!({"title":"  ","body":"b"}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["kind"], "validation_failed");
    assert!(body["error"]["message"].as_str().unwrap().contains("title"));
}

#[tokio::test]
async fn overlong_title_is_400() {
    let (status, _) = send(authed(
        "POST",
        "/posts",
        json!({"title": "x".repeat(121), "body": "b"}),
    ))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn missing_required_field_is_422_from_the_extractor() {
    // No `body` field at all. Serde cannot construct `CreatePost`, so axum's Json
    // extractor rejects it before the handler runs. 422, not our 400.
    let (status, _) = send(authed("POST", "/posts", json!({"title": "only a title"}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn author_defaults_to_anonymous() {
    let (status, body) =
        send(authed("POST", "/posts", json!({"title":"No author","body":"b"}))).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["author"], "anonymous");
}

// ---------------------------------------------------------------------------
// Full lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_read_update_delete_round_trip() {
    let state = Arc::new(AppState::new(KEY));
    let router = app(state);

    // CREATE
    let (status, created) = send_to(
        &router,
        authed("POST", "/posts", json!({"title":"Original","body":"First","author":"Ada"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["id"], 3);
    let created_at = created["createdAt"].clone();

    // READ — it persisted, which is the whole point of Day 3 onwards
    let (status, fetched) = send_to(&router, get("/posts/3")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["title"], "Original");

    // PUT — full replace, id and createdAt preserved
    let (status, replaced) = send_to(
        &router,
        authed("PUT", "/posts/3", json!({"title":"Replaced","body":"Second","author":"Grace"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replaced["title"], "Replaced");
    assert_eq!(replaced["author"], "Grace");
    assert_eq!(replaced["id"], 3);
    assert_eq!(replaced["createdAt"], created_at);

    // PATCH — only the supplied field changes
    let (status, patched) =
        send_to(&router, authed("PATCH", "/posts/3", json!({"title":"Patched"}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["title"], "Patched");
    assert_eq!(patched["body"], "Second", "body must survive a PATCH");
    assert_eq!(patched["author"], "Grace", "author must survive a PATCH");

    // DELETE
    let (status, _) = send_to(&router, authed("DELETE", "/posts/3", json!({}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = send_to(&router, get("/posts/3")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn duplicate_titles_conflict() {
    let state = Arc::new(AppState::new(KEY));
    let router = app(state);

    let (status, _) = send_to(
        &router,
        authed("POST", "/posts", json!({"title":"Unique","body":"b"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // Same title, different case — still a conflict.
    let (status, body) = send_to(
        &router,
        authed("POST", "/posts", json!({"title":"unique","body":"b"})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["kind"], "conflict");
}

#[tokio::test]
async fn a_post_may_keep_its_own_title_on_replace() {
    // The `excluding: Some(id)` case. Without it, PUT-ing a post without changing
    // its title would conflict with itself.
    let state = Arc::new(AppState::new(KEY));
    let router = app(state);

    let (status, _) = send_to(
        &router,
        authed("PUT", "/posts/1", json!({"title":"Futures are lazy","body":"edited","author":"Ada"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn empty_patch_is_rejected() {
    let (status, body) = send(authed("PATCH", "/posts/1", json!({}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["kind"], "validation_failed");
}

#[tokio::test]
async fn mutating_a_missing_post_is_404() {
    for method in ["PUT", "PATCH", "DELETE"] {
        let (status, _) = send(authed(
            method,
            "/posts/999",
            json!({"title":"x","body":"y"}),
        ))
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} on a missing post");
    }
}

// ---------------------------------------------------------------------------
// Concurrency — the real assertion of Week 4
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_creates_never_collide_on_an_id() {
    let state = Arc::new(AppState::new(KEY));
    let router = app(state);

    // 50 creates issued at once, each with a distinct title so none conflict.
    let mut handles = Vec::new();
    for i in 0..50 {
        let router = router.clone();
        handles.push(tokio::spawn(async move {
            let req = authed(
                "POST",
                "/posts",
                json!({"title": format!("concurrent {i}"), "body": "b"}),
            );
            router.oneshot(req).await.unwrap().status()
        }));
    }

    for handle in handles {
        assert_eq!(handle.await.unwrap(), StatusCode::CREATED);
    }

    // 2 seeded + 50 created. A lost update or a duplicated id shows up here as a
    // count below 52.
    let (_, body) = send_to(&router, get("/posts?limit=200")).await;
    let posts = body.as_array().unwrap();
    assert_eq!(posts.len(), 52);

    let mut ids: Vec<u64> = posts.iter().map(|p| p["id"].as_u64().unwrap()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 52, "ids must be unique");
}
