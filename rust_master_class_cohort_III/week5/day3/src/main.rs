//! Week 5 · Day 3 — Nicknames & JSON messages
//!
//! Adds named users and structured JSON messages to Day 2's broadcast server.
//!
//!   cargo run
//!   open http://localhost:3000, enter a name, chat

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
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
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast::{self, error::RecvError};

const CHANNEL_CAPACITY: usize = 64;

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

/// Messages the client sends to the server.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientMsg {
    Join    { name: String },
    Message { text: String },
    Leave,
}

/// Messages the server sends to clients (broadcast as JSON strings).
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ServerMsg<'a> {
    Joined  { name: &'a str, online: usize },
    Left    { name: &'a str, online: usize },
    Message { name: &'a str, text: &'a str },
    Error   { text: &'a str },
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct AppState {
    tx:    broadcast::Sender<String>,
    names: Mutex<HashSet<String>>,
    count: AtomicUsize,
}

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
    let state = Arc::new(AppState {
        tx,
        names: Mutex::new(HashSet::new()),
        count: AtomicUsize::new(0),
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
    let mut rx = state.tx.subscribe();
    // The client's chosen name — None until a Join message arrives.
    let mut my_name: Option<String> = None;

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(raw))) => {
                        handle_client_msg(&raw, &mut my_name, &mut socket, &state).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }

            broadcast = rx.recv() => {
                match broadcast {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        let warn = serde_json::to_string(&ServerMsg::Error {
                            text: &format!("you missed {n} messages"),
                        }).unwrap();
                        if socket.send(Message::Text(warn.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }

    // Clean up on disconnect.
    if let Some(name) = my_name.take() {
        state.names.lock().unwrap().remove(&name);
        let n = state.count.fetch_sub(1, Ordering::Relaxed) - 1;
        let msg = serde_json::to_string(&ServerMsg::Left { name: &name, online: n }).unwrap();
        let _ = state.tx.send(msg);
    }
}

/// Handles one parsed message from the client.
async fn handle_client_msg(
    raw: &str,
    my_name: &mut Option<String>,
    socket: &mut WebSocket,
    state: &Arc<AppState>,
) {
    let Ok(msg) = serde_json::from_str::<ClientMsg>(raw) else {
        let err = serde_json::to_string(&ServerMsg::Error { text: "invalid JSON" }).unwrap();
        let _ = socket.send(Message::Text(err.into())).await;
        return;
    };

    match msg {
        ClientMsg::Join { name } => {
            if name.is_empty() || name.len() > 20 {
                let err = serde_json::to_string(&ServerMsg::Error {
                    text: "name must be 1–20 characters",
                }).unwrap();
                let _ = socket.send(Message::Text(err.into())).await;
                return;
            }

            // Check-and-insert under one lock so two simultaneous joins cannot
            // both see the name as free.
            let taken = {
                let mut names = state.names.lock().unwrap();
                if names.contains(&name) {
                    true
                } else {
                    names.insert(name.clone());
                    false
                }
            };

            if taken {
                let err = serde_json::to_string(&ServerMsg::Error {
                    text: "name already taken",
                }).unwrap();
                let _ = socket.send(Message::Text(err.into())).await;
                return;
            }

            let n = state.count.fetch_add(1, Ordering::Relaxed) + 1;
            *my_name = Some(name.clone());

            let msg = serde_json::to_string(&ServerMsg::Joined { name: &name, online: n }).unwrap();
            let _ = state.tx.send(msg);
        }

        ClientMsg::Message { text } => {
            let Some(name) = my_name.as_deref() else {
                let err = serde_json::to_string(&ServerMsg::Error {
                    text: "send /join first",
                }).unwrap();
                let _ = socket.send(Message::Text(err.into())).await;
                return;
            };

            let msg = serde_json::to_string(&ServerMsg::Message { name, text: &text }).unwrap();
            let _ = state.tx.send(msg);
        }

        ClientMsg::Leave => {}
    }
}

// ---------------------------------------------------------------------------
// Inline HTML
// ---------------------------------------------------------------------------

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Chat — Day 3</title></head>
<body>
<h2>Chat Room — Day 3</h2>
<div id="login">
  <input id="name" placeholder="your name" style="width:200px">
  <button onclick="join()">Join</button>
</div>
<div id="chat" style="display:none">
  <input id="msg" placeholder="message" style="width:300px">
  <button onclick="send()">Send</button>
</div>
<ul id="log" style="font-family:monospace"></ul>
<script>
  const ws = new WebSocket("ws://localhost:3000/ws");
  ws.onmessage = e => {
    const data = JSON.parse(e.data);
    const li = document.createElement("li");
    if (data.type === "message")  li.textContent = data.name + ": " + data.text;
    else if (data.type === "error") li.textContent = "! " + data.text;
    else li.textContent = "* " + JSON.stringify(data);
    document.getElementById("log").prepend(li);
  };
  function join() {
    const name = document.getElementById("name").value;
    ws.send(JSON.stringify({ type: "join", name }));
    document.getElementById("login").style.display = "none";
    document.getElementById("chat").style.display = "block";
  }
  function send() {
    const el = document.getElementById("msg");
    ws.send(JSON.stringify({ type: "message", text: el.value }));
    el.value = "";
  }
  document.getElementById("msg")?.addEventListener("keydown", e => {
    if (e.key === "Enter") send();
  });
</script>
</body>
</html>"#;
