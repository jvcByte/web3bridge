//! Week 4 · Day 5 — Blog API binary.
//!
//! Deliberately thin. Everything real lives in the library so `tests/api.rs` can
//! reach it; this file only reads config, binds a port, and serves.

use std::sync::Arc;

use day5_blog_api::{app, AppState};

#[tokio::main]
async fn main() {
    let api_key = std::env::var("API_KEY").unwrap_or_else(|_| "dev-secret-key".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("127.0.0.1:{port}");

    let state = Arc::new(AppState::new(api_key.clone()));

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("failed to bind {addr}: {e}");
            eprintln!("is something already using port {port}? try PORT=3001 cargo run");
            std::process::exit(1);
        }
    };

    println!("Web3Bridge Blog API — Week 4 deliverable");
    println!("listening on http://{addr}");
    println!("api key: {api_key}   (override with API_KEY=... cargo run)");
    println!();
    println!("  curl localhost:{port}/health");
    println!("  curl 'localhost:{port}/posts?author=Ada&limit=10'");
    println!("  curl -X POST localhost:{port}/posts \\");
    println!("       -H 'content-type: application/json' -H 'x-api-key: {api_key}' \\");
    println!("       -d '{{\"title\":\"Hello\",\"body\":\"World\"}}'");
    println!();
    println!("  cargo test    # 20 integration tests, no port needed");

    // `with_graceful_shutdown` lets in-flight requests finish instead of having
    // their sockets cut mid-response on Ctrl-C. Week 5 needs the same idea for
    // chat clients, so it is worth seeing here first.
    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    println!("\nshut down cleanly");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl-C handler");
    println!("\nCtrl-C received, draining in-flight requests...");
}
