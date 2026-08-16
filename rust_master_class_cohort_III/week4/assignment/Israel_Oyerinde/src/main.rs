use std::env;
use week4_assignment::{build_app, AppState, DEFAULT_API_KEY};

#[tokio::main]
async fn main() {
    let api_key = env::var("API_KEY").unwrap_or_else(|_| DEFAULT_API_KEY.to_string());
    let app = build_app(AppState::new(api_key));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind to port 3000");

    println!("Book Library API listening on http://localhost:3000");
    axum::serve(listener, app)
        .await
        .expect("Book Library API server failed");
}
