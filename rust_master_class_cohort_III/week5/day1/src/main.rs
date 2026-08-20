//! Week 5 · Day 1 — Axum WebSocket echo server
//!
//! Builds on Week 4's Axum foundation. Three routes:
//!   GET /         — HTML chat page
//!   GET /health   — JSON health check (same as Week 4)
//!   GET /ws       — WebSocket upgrade; echoes every message back
//!
//!   cargo run
//!   open http://localhost:3000

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use serde_json::json;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/ws", get(ws_handler));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

/// Serves the chat UI as an inline HTML string.
/// No files, no templates — just enough to test the WebSocket from a browser.
async fn index() -> Html<&'static str> {
    Html(HTML)
}

/// Accepts the WebSocket upgrade and hands the live socket to `handle_socket`.
///
/// `WebSocketUpgrade` is an Axum extractor. It validates the upgrade headers and
/// gives you an object you can call `.on_upgrade()` on. Axum sends the 101
/// Switching Protocols response; after that the connection is a WebSocket.
async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

/// Echoes every text message back to the same client.
///
/// `socket.recv()` yields `Ok(Message)` or `None` on clean close.
/// Breaking on send error covers the case where the client closed without
/// sending a proper Close frame (e.g. tab closed, network dropped).
async fn handle_socket(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(_) | Message::Binary(_) => {
                if socket.send(msg).await.is_err() {
                    break; // client gone
                }
            }
            Message::Close(_) => break,
            // Axum replies to Ping automatically; nothing for us to do.
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Inline HTML — just enough for a demo
// ---------------------------------------------------------------------------

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>WS Echo</title></head>
<body>
<h2>WebSocket Echo — Day 1</h2>
<input id="msg" placeholder="type something" style="width:300px">
<button onclick="send()">Send</button>
<ul id="log"></ul>
<script>
  const ws = new WebSocket("ws://localhost:3000/ws");
  ws.onmessage = e => log("← " + e.data);
  function send() {
    const v = document.getElementById("msg").value;
    ws.send(v);
    log("→ " + v);
    document.getElementById("msg").value = "";
  }
  function log(msg) {
    const li = document.createElement("li");
    li.textContent = msg;
    document.getElementById("log").prepend(li);
  }
</script>
</body>
</html>"#;
