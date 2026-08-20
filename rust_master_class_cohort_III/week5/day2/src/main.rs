//! Week 5 · Day 2 — Broadcast chat
//!
//! Adds shared state and a broadcast channel to Day 1's echo server.
//! Every message from any client reaches all connected clients.
//!
//!   cargo run
//!   open two browser tabs at http://localhost:3000 — type in one, both see it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use serde_json::json;
use tokio::sync::broadcast::{self, error::RecvError};

const CHANNEL_CAPACITY: usize = 64;

#[derive(Clone)]
struct AppState {
    /// Every connected client subscribes to this.
    tx: broadcast::Sender<String>,
    /// Simple connection counter — no lock needed.
    count: Arc<AtomicUsize>,
}

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
    let state = Arc::new(AppState {
        tx,
        count: Arc::new(AtomicUsize::new(0)),
    });

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "connected": state.count.load(Ordering::Relaxed)
    }))
}

async fn index() -> Html<&'static str> {
    Html(HTML)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// ---------------------------------------------------------------------------
// Per-connection logic
// ---------------------------------------------------------------------------

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    // Subscribe *before* announcing the join — the receiver only sees messages
    // published after it is created.
    let mut rx = state.tx.subscribe();
    let n = state.count.fetch_add(1, Ordering::Relaxed) + 1;

    let joined = format!("* a user joined ({n} online)");
    let _ = state.tx.send(joined);

    loop {
        tokio::select! {
            // --- inbound: socket → broadcast channel -----------------------
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        // Publish to everybody, including this client.
                        // Their own message comes back through the rx branch
                        // below — so they see it too.
                        let _ = state.tx.send(text.to_string());
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }

            // --- outbound: broadcast channel → socket ----------------------
            broadcast = rx.recv() => {
                match broadcast {
                    Ok(text) => {
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break; // client gone
                        }
                    }
                    // This client fell more than CHANNEL_CAPACITY messages behind.
                    // Not fatal — tell them and carry on.
                    Err(RecvError::Lagged(n)) => {
                        let warn = format!("* you missed {n} messages (fell behind)");
                        if socket.send(Message::Text(warn.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }

    let n = state.count.fetch_sub(1, Ordering::Relaxed) - 1;
    let left = format!("* a user left ({n} online)");
    let _ = state.tx.send(left);
}

// ---------------------------------------------------------------------------
// Inline HTML
// ---------------------------------------------------------------------------

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Chat — Day 2</title></head>
<body>
<h2>Chat Room — Day 2</h2>
<p><em>Open this page in multiple tabs. Type in one; all see it.</em></p>
<input id="msg" placeholder="message" style="width:300px">
<button onclick="send()">Send</button>
<ul id="log" style="font-family:monospace"></ul>
<script>
  const ws = new WebSocket("ws://localhost:3000/ws");
  ws.onmessage = e => {
    const li = document.createElement("li");
    li.textContent = e.data;
    document.getElementById("log").prepend(li);
  };
  function send() {
    const el = document.getElementById("msg");
    ws.send(el.value);
    el.value = "";
  }
  document.getElementById("msg").addEventListener("keydown", e => {
    if (e.key === "Enter") send();
  });
</script>
</body>
</html>"#;
