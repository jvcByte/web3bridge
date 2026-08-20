//! Axum route handlers — REST and WebSocket.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::{
    extract::{Path, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    response::{Html, IntoResponse},
    Json,
};
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;

use crate::protocol::{ClientMsg, ServerMsg, DEFAULT_ROOM, MAX_NAME};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// REST
// ---------------------------------------------------------------------------

pub async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({ "status": "ok", "connected": state.connected() }))
}

pub async fn list_rooms(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.prune_rooms();
    let map = state.rooms.lock().unwrap();
    let rooms: Vec<_> = map
        .iter()
        .map(|(name, r)| json!({ "name": name, "online": r.tx.receiver_count() }))
        .collect();
    Json(rooms)
}

pub async fn room_info(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let map = state.rooms.lock().unwrap();
    match map.get(&name) {
        Some(r) => Json(json!({ "name": name, "online": r.tx.receiver_count() })).into_response(),
        None     => Json(json!({ "error": "room not found" })).into_response(),
    }
}

pub async fn list_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.list_names())
}

pub async fn index() -> Html<&'static str> {
    Html(crate::HTML)
}

// ---------------------------------------------------------------------------
// WebSocket upgrade
// ---------------------------------------------------------------------------

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// ---------------------------------------------------------------------------
// Per-connection logic
// ---------------------------------------------------------------------------

pub async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut my_name: Option<String> = None;
    let mut room_tx = state.get_or_create_room(DEFAULT_ROOM);
    let mut room_rx = room_tx.subscribe();

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(raw))) => {
                        if let Some(new_rx) = on_msg(
                            &raw, &mut my_name, &mut room_tx, &mut socket, &state,
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
                        let warn = ServerMsg::error(format!("you missed {n} messages"));
                        if socket.send(warn.to_ws_text()).await.is_err() { break; }
                    }
                    Err(RecvError::Closed) => {
                        // Room was destroyed — fall back to lobby.
                        room_tx = state.get_or_create_room(DEFAULT_ROOM);
                        room_rx = room_tx.subscribe();
                    }
                }
            }
        }
    }

    // --- teardown -----------------------------------------------------------
    // This path runs on ANY disconnect: clean close, tab closed, network drop,
    // or a panic in this task. A cleanup that only runs on Leave never runs.
    if let Some(name) = my_name.take() {
        state.remove_name(&name);
        let n = state.count.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);
        let msg = serde_to_string(&ServerMsg::Left { name, online: n });
        let _ = room_tx.send(msg);
        state.prune_rooms();
    }
}

async fn on_msg(
    raw: &str,
    my_name: &mut Option<String>,
    room_tx: &mut tokio::sync::broadcast::Sender<String>,
    socket: &mut WebSocket,
    state: &Arc<AppState>,
) -> Option<tokio::sync::broadcast::Receiver<String>> {
    let msg = match serde_json::from_str::<ClientMsg>(raw) {
        Ok(m)  => m,
        Err(_) => {
            let _ = socket.send(ServerMsg::error("invalid JSON").to_ws_text()).await;
            return None;
        }
    };

    match msg {
        ClientMsg::Join { name, room } => {
            if name.is_empty() || name.len() > MAX_NAME {
                let _ = socket.send(
                    ServerMsg::error(format!("name must be 1–{MAX_NAME} characters")).to_ws_text()
                ).await;
                return None;
            }
            if !state.register_name(&name) {
                let _ = socket.send(ServerMsg::error("name already taken").to_ws_text()).await;
                return None;
            }

            let room_name = room.unwrap_or_else(|| DEFAULT_ROOM.to_string());
            let tx = state.get_or_create_room(&room_name);
            let rx = tx.subscribe();
            let online = tx.receiver_count();

            state.count.fetch_add(1, Ordering::Relaxed);
            *my_name = Some(name.clone());
            *room_tx = tx.clone();

            let _ = tx.send(serde_to_string(&ServerMsg::Joined {
                name, room: room_name, online,
            }));

            return Some(rx);
        }

        ClientMsg::Message { text } => {
            let Some(name) = my_name.as_deref() else {
                let _ = socket.send(ServerMsg::error("join first").to_ws_text()).await;
                return None;
            };
            let msg = serde_to_string(&ServerMsg::Message {
                name: name.to_string(),
                text,
            });
            let _ = room_tx.send(msg);
        }

        ClientMsg::Switch { room } => {
            let Some(name) = my_name.as_deref() else { return None; };

            // Announce departure from old room.
            let n = room_tx.receiver_count().saturating_sub(1);
            let _ = room_tx.send(serde_to_string(&ServerMsg::Left {
                name: name.to_string(),
                online: n,
            }));

            let tx = state.get_or_create_room(&room);
            let rx = tx.subscribe();
            let online = tx.receiver_count();
            *room_tx = tx.clone();

            let _ = tx.send(serde_to_string(&ServerMsg::RoomChanged {
                room, online,
            }));
            state.prune_rooms();
            return Some(rx);
        }

        ClientMsg::Leave => {}
    }

    None
}

fn serde_to_string<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap()
}
