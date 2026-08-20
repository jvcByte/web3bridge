//! Week 5 · Day 4 — Rooms & REST + WebSocket on one Axum server
//!
//! Each room has its own broadcast channel. REST endpoints sit alongside
//! the WebSocket route — same Axum app, same shared state.
//!
//!   cargo run
//!   open http://localhost:3000
//!   curl http://localhost:3000/rooms
//!   curl http://localhost:3000/users

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::{
    extract::{
        Path, State,
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
const DEFAULT_ROOM: &str = "lobby";

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientMsg {
    Join    { name: String, room: Option<String> },
    Message { text: String },
    Switch  { room: String },
    Leave,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMsg<'a> {
    Joined      { name: &'a str, room: &'a str, online: usize },
    Left        { name: &'a str, online: usize },
    Message     { name: &'a str, text: &'a str },
    RoomChanged { room: &'a str, online: usize },
    Error       { text: &'a str },
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Per-room state: a broadcast sender and the count of current subscribers.
struct Room {
    tx: broadcast::Sender<String>,
}

struct AppState {
    /// Room name → Room. Created on demand, removed when empty.
    rooms: Mutex<HashMap<String, Room>>,
    /// All connected nicknames (for /users endpoint).
    names: Mutex<HashSet<String>>,
    /// Total connected clients.
    count: AtomicUsize,
}

impl AppState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            rooms: Mutex::new(HashMap::new()),
            names: Mutex::new(HashSet::new()),
            count: AtomicUsize::new(0),
        })
    }

    /// Returns the sender for `room`, creating the room if it does not exist.
    fn get_or_create_room(&self, room: &str) -> broadcast::Sender<String> {
        let mut map = self.rooms.lock().unwrap();
        if let Some(r) = map.get(room) {
            if r.tx.receiver_count() > 0 {
                return r.tx.clone();
            }
        }
        let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
        map.insert(room.to_string(), Room { tx: tx.clone() });
        tx
    }

    /// Remove rooms that have no subscribers.
    fn prune_rooms(&self) {
        self.rooms.lock().unwrap().retain(|_, r| r.tx.receiver_count() > 0);
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let state = AppState::new();

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/rooms", get(list_rooms))
        .route("/rooms/:name", get(room_info))
        .route("/users", get(list_users))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}

// ---------------------------------------------------------------------------
// REST handlers — same pattern as Week 4
// ---------------------------------------------------------------------------

async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "connected": state.count.load(Ordering::Relaxed)
    }))
}

async fn list_rooms(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.prune_rooms();
    let map = state.rooms.lock().unwrap();
    let rooms: Vec<_> = map
        .iter()
        .map(|(name, r)| json!({ "name": name, "online": r.tx.receiver_count() }))
        .collect();
    Json(rooms)
}

async fn room_info(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let map = state.rooms.lock().unwrap();
    match map.get(&name) {
        Some(r) => Json(json!({ "name": name, "online": r.tx.receiver_count() })).into_response(),
        None => Json(json!({ "error": "room not found" })).into_response(),
    }
}

async fn list_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let names: Vec<_> = state.names.lock().unwrap().iter().cloned().collect();
    Json(names)
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

async fn index() -> Html<&'static str> {
    Html(HTML)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut my_name: Option<String> = None;
    let mut room_tx: Option<broadcast::Sender<String>> = None;
    let mut room_rx = {
        // Start subscribed to lobby so select! always has a valid receiver.
        let tx = state.get_or_create_room(DEFAULT_ROOM);
        let rx = tx.subscribe();
        room_tx = Some(tx);
        rx
    };

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(raw))) => {
                        if let Some(new_rx) = on_client_msg(
                            &raw,
                            &mut my_name,
                            &mut room_tx,
                            &mut socket,
                            &state,
                        ).await {
                            room_rx = new_rx;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }

            broadcast = room_rx.recv() => {
                match broadcast {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        let warn = ser(&ServerMsg::Error {
                            text: &format!("you missed {n} messages"),
                        });
                        if socket.send(Message::Text(warn.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => {
                        // Room was destroyed (last subscriber left before us).
                        // Re-subscribe to lobby.
                        let tx = state.get_or_create_room(DEFAULT_ROOM);
                        room_rx = tx.subscribe();
                        room_tx = Some(tx);
                    }
                }
            }
        }
    }

    // Cleanup
    if let Some(name) = my_name.take() {
        state.names.lock().unwrap().remove(&name);
        let n = state.count.fetch_sub(1, Ordering::Relaxed) - 1;
        if let Some(tx) = &room_tx {
            let _ = tx.send(ser(&ServerMsg::Left { name: &name, online: n }));
        }
        state.prune_rooms();
    }
}

/// Handles one incoming client message. Returns a new `Receiver` when the
/// client switches rooms (the caller must replace `room_rx` with it).
async fn on_client_msg(
    raw: &str,
    my_name: &mut Option<String>,
    room_tx: &mut Option<broadcast::Sender<String>>,
    socket: &mut WebSocket,
    state: &Arc<AppState>,
) -> Option<broadcast::Receiver<String>> {
    let Ok(msg) = serde_json::from_str::<ClientMsg>(raw) else {
        let _ = socket.send(Message::Text(ser(&ServerMsg::Error { text: "invalid JSON" }).into())).await;
        return None;
    };

    match msg {
        ClientMsg::Join { name, room } => {
            if name.is_empty() || name.len() > 20 {
                let _ = socket.send(Message::Text(
                    ser(&ServerMsg::Error { text: "name must be 1–20 characters" }).into()
                )).await;
                return None;
            }

            let taken = {
                let mut names = state.names.lock().unwrap();
                if names.contains(&name) { true } else { names.insert(name.clone()); false }
            };

            if taken {
                let _ = socket.send(Message::Text(
                    ser(&ServerMsg::Error { text: "name already taken" }).into()
                )).await;
                return None;
            }

            let room_name = room.unwrap_or_else(|| DEFAULT_ROOM.to_string());
            let tx = state.get_or_create_room(&room_name);
            let rx = tx.subscribe();
            let online = tx.receiver_count();

            state.count.fetch_add(1, Ordering::Relaxed);
            *my_name = Some(name.clone());
            *room_tx = Some(tx.clone());

            let _ = tx.send(ser(&ServerMsg::Joined {
                name: &name,
                room: &room_name,
                online,
            }));

            return Some(rx);
        }

        ClientMsg::Message { text } => {
            let Some(name) = my_name.as_deref() else {
                let _ = socket.send(Message::Text(
                    ser(&ServerMsg::Error { text: "join first" }).into()
                )).await;
                return None;
            };
            if let Some(tx) = room_tx {
                let _ = tx.send(ser(&ServerMsg::Message { name, text: &text }));
            }
        }

        ClientMsg::Switch { room } => {
            let Some(name) = my_name.as_deref() else { return None; };

            // Leave current room.
            if let Some(old_tx) = room_tx.take() {
                let n = old_tx.receiver_count().saturating_sub(1);
                let _ = old_tx.send(ser(&ServerMsg::Left { name, online: n }));
            }

            let tx = state.get_or_create_room(&room);
            let rx = tx.subscribe();
            let online = tx.receiver_count();
            *room_tx = Some(tx.clone());

            let _ = tx.send(ser(&ServerMsg::RoomChanged { room: &room, online }));
            state.prune_rooms();
            return Some(rx);
        }

        ClientMsg::Leave => {}
    }

    None
}

fn ser<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap()
}

// ---------------------------------------------------------------------------
// Inline HTML
// ---------------------------------------------------------------------------

const HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Chat — Day 4</title></head>
<body>
<h2>Chat Rooms — Day 4</h2>
<div id="login">
  <input id="name" placeholder="your name" style="width:140px">
  <input id="room" placeholder="room (default: lobby)" style="width:160px">
  <button onclick="join()">Join</button>
</div>
<div id="chat" style="display:none">
  <input id="msg" placeholder="message" style="width:260px">
  <button onclick="send()">Send</button>
  &nbsp;
  <input id="newroom" placeholder="switch room" style="width:120px">
  <button onclick="switchRoom()">Switch</button>
</div>
<ul id="log" style="font-family:monospace"></ul>
<script>
  const ws = new WebSocket("ws://localhost:3000/ws");
  ws.onmessage = e => {
    const d = JSON.parse(e.data);
    const li = document.createElement("li");
    if (d.type === "message")      li.textContent = d.name + ": " + d.text;
    else if (d.type === "error")   li.textContent = "! " + d.text;
    else                           li.textContent = "* " + JSON.stringify(d);
    document.getElementById("log").prepend(li);
  };
  function join() {
    const name = document.getElementById("name").value;
    const room = document.getElementById("room").value || "lobby";
    ws.send(JSON.stringify({ type: "join", name, room }));
    document.getElementById("login").style.display = "none";
    document.getElementById("chat").style.display  = "block";
  }
  function send() {
    const el = document.getElementById("msg");
    ws.send(JSON.stringify({ type: "message", text: el.value }));
    el.value = "";
  }
  function switchRoom() {
    const el = document.getElementById("newroom");
    ws.send(JSON.stringify({ type: "switch", room: el.value }));
    el.value = "";
  }
  document.getElementById("msg")?.addEventListener("keydown", e => {
    if (e.key === "Enter") send();
  });
</script>
</body>
</html>"#;
