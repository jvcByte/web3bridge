//! Wire types for client ↔ server communication.

use serde::{Deserialize, Serialize};
pub const MAX_NAME: usize = 20;
pub const DEFAULT_ROOM: &str = "lobby";

// ---------------------------------------------------------------------------
// Client → server
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMsg {
    Join    { name: String, room: Option<String> },
    Message { text: String },
    Switch  { room: String },
    Leave,
}

// ---------------------------------------------------------------------------
// Server → client
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Joined      { name: String, room: String, online: usize },
    Left        { name: String, online: usize },
    Message     { name: String, text: String },
    RoomChanged { room: String, online: usize },
    Error       { text: String },
}

impl ServerMsg {
    pub fn error(text: impl Into<String>) -> Self {
        Self::Error { text: text.into() }
    }

    pub fn to_ws_text(&self) -> axum::extract::ws::Message {
        axum::extract::ws::Message::Text(
            serde_json::to_string(self).unwrap().into()
        )
    }
}
