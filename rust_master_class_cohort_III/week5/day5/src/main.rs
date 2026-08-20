//! Week 5 · Day 5 — complete Axum WebSocket chat server
//!
//!   cargo run                  # serve on 127.0.0.1:3000
//!   cargo run -- demo          # scripted 3-client session
//!   cargo run -- storm 20      # 20 concurrent clients, invariants checked
//!   cargo test

mod handler;
mod protocol;
mod state;

use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;

use axum::{routing::get, Router};
use tokio::net::TcpListener;

use crate::handler::*;
use crate::protocol::ClientMsg;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// HTML served at GET /
// ---------------------------------------------------------------------------

pub const HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Chat — Day 5</title></head>
<body>
<h2>Chat — Day 5</h2>
<div id="login">
  <input id="name" placeholder="your name" style="width:130px">
  <input id="room" placeholder="room (default: lobby)" style="width:160px">
  <button onclick="join()">Join</button>
</div>
<div id="chat" style="display:none">
  <input id="msg" placeholder="message" style="width:240px">
  <button onclick="send()">Send</button>
  &nbsp;
  <input id="newroom" placeholder="switch room" style="width:110px">
  <button onclick="switchRoom()">Switch</button>
</div>
<ul id="log" style="font-family:monospace;max-height:400px;overflow:auto"></ul>
<script>
  const ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onmessage = e => {
    const d = JSON.parse(e.data);
    const li = document.createElement("li");
    if      (d.type === "message")      li.textContent = d.name + ": " + d.text;
    else if (d.type === "error")        li.textContent = "! " + d.text;
    else if (d.type === "joined")       li.textContent = "* " + d.name + " joined " + d.room;
    else if (d.type === "left")         li.textContent = "* " + d.name + " left (" + d.online + " online)";
    else if (d.type === "room_changed") li.textContent = "* moved to " + d.room;
    else                                li.textContent = JSON.stringify(d);
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
    if (!el.value) return;
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

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str);

    match mode {
        None | Some("serve") => {
            let addr = args.get(1).map(String::as_str).unwrap_or("127.0.0.1:3000");
            serve(addr).await;
        }
        Some("demo") => demo().await,
        Some("storm") => {
            let n = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(20);
            let ok = storm(n).await;
            std::process::exit(if ok { 0 } else { 1 });
        }
        Some(other) => {
            eprintln!("unknown mode {other}; try: serve [addr] | demo | storm [n]");
            std::process::exit(1);
        }
    }
}

async fn serve(addr: &str) {
    let state = AppState::new();
    let app = build_router(state);
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/rooms", get(list_rooms))
        .route("/rooms/{name}", get(room_info))
        .route("/users", get(list_users))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Scripted demo
// ---------------------------------------------------------------------------

async fn demo() {
    let state = AppState::new();
    let app = build_router(Arc::clone(&state));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());

    println!("demo server on {addr}\n");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut alice = DemoClient::connect(addr, "alice", "rust").await;
    let mut bob = DemoClient::connect(addr, "bob", "rust").await;
    let mut carol = DemoClient::connect(addr, "carol", "general").await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    println!("1. alice and bob are in #rust, carol is in #general");

    alice.send_msg("hello from rust").await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    println!("   bob received: {:?}", bob.drain().await);
    println!("   carol received: {:?}", carol.drain().await); // should be empty

    println!("\n2. carol switches to #rust");
    carol.switch("rust").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    carol.drain().await;

    alice.send_msg("now carol can see this").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    println!("   carol received: {:?}", carol.drain().await);

    println!("\n3. check REST state");
    let body = reqwest_get(format!("http://{addr}/users")).await;
    println!("   GET /users -> {body}");
    let body = reqwest_get(format!("http://{addr}/rooms")).await;
    println!("   GET /rooms -> {body}");

    println!("\ndone.");
}

async fn reqwest_get(url: String) -> String {
    // Use tokio's built-in HTTP for a simple GET — no reqwest dependency needed.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr: std::net::SocketAddr = url
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let path = url
        .trim_start_matches("http://")
        .splitn(2, '/')
        .nth(1)
        .unwrap_or("");
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!("GET /{path} HTTP/1.0\r\nHost: {addr}\r\n\r\n");
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let resp = String::from_utf8_lossy(&buf);
    resp.splitn(2, "\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Storm: N concurrent clients, invariants checked at the end
// ---------------------------------------------------------------------------

async fn storm(n: usize) -> bool {
    let state = AppState::new();
    let app = build_router(Arc::clone(&state));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());
    tokio::time::sleep(Duration::from_millis(50)).await;

    println!("storm: {n} clients connecting...");

    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        let name = format!("user{i}");
        handles.push(tokio::spawn(storm_client(addr, name, n)));
    }

    let mut all_ok = true;
    for (i, h) in handles.into_iter().enumerate() {
        match h.await {
            Ok(true) => {}
            Ok(false) => {
                eprintln!("  user{i}: FAIL");
                all_ok = false;
            }
            Err(e) => {
                eprintln!("  user{i}: panic: {e}");
                all_ok = false;
            }
        }
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // After all disconnect, users and rooms should be empty.
    let remaining_users = state.list_names().len();
    let remaining_rooms = state
        .rooms
        .lock()
        .unwrap()
        .values()
        .filter(|r| r.tx.receiver_count() > 0)
        .count();

    if remaining_users != 0 {
        eprintln!("FAIL: {remaining_users} names not cleaned up");
        all_ok = false;
    }
    if remaining_rooms != 0 {
        eprintln!("FAIL: {remaining_rooms} rooms not cleaned up");
        all_ok = false;
    }

    if all_ok {
        println!("PASS ✓  ({n} clients, all invariants held)");
    }
    all_ok
}

/// One storm client: connects, joins, sends a message, waits briefly, disconnects.
/// Returns true if it received at least one message back (meaning broadcast worked).
async fn storm_client(addr: std::net::SocketAddr, name: String, _total: usize) -> bool {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message as TMsg};

    let url = format!("ws://{addr}/ws");
    let Ok((mut ws, _)) = connect_async(&url).await else {
        return false;
    };

    // Join
    let join = serde_json::to_string(&ClientMsg::Join {
        name: name.clone(),
        room: Some("storm".to_string()),
    })
    .unwrap();
    if ws.send(TMsg::Text(join.into())).await.is_err() {
        return false;
    }

    // Send one message
    let msg = serde_json::to_string(&ClientMsg::Message {
        text: format!("hello from {name}"),
    })
    .unwrap();
    if ws.send(TMsg::Text(msg.into())).await.is_err() {
        return false;
    }

    // Wait for at least one message back.
    let received = tokio::time::timeout(Duration::from_millis(500), ws.next()).await;
    let ok = matches!(received, Ok(Some(Ok(_))));

    let _ = ws.close(None).await;
    ok
}

// ---------------------------------------------------------------------------
// Demo helper client (raw tokio-tungstenite)
// ---------------------------------------------------------------------------

struct DemoClient {
    ws: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

impl DemoClient {
    async fn connect(addr: std::net::SocketAddr, name: &str, room: &str) -> Self {
        use tokio_tungstenite::connect_async;
        let (ws, _) = connect_async(format!("ws://{addr}/ws")).await.unwrap();
        let mut c = Self { ws };
        c.send_raw(
            &serde_json::to_string(&ClientMsg::Join {
                name: name.to_string(),
                room: Some(room.to_string()),
            })
            .unwrap(),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        c.drain().await;
        c
    }

    async fn send_raw(&mut self, s: &str) {
        use futures::SinkExt;
        use tokio_tungstenite::tungstenite::Message as TMsg;
        let _ = self.ws.send(TMsg::Text(s.to_string().into())).await;
    }

    async fn send_msg(&mut self, text: &str) {
        let s = serde_json::to_string(&ClientMsg::Message {
            text: text.to_string(),
        })
        .unwrap();
        self.send_raw(&s).await;
    }

    async fn switch(&mut self, room: &str) {
        let s = serde_json::to_string(&ClientMsg::Switch {
            room: room.to_string(),
        })
        .unwrap();
        self.send_raw(&s).await;
    }

    async fn drain(&mut self) -> Vec<String> {
        use futures::StreamExt;
        use tokio_tungstenite::tungstenite::Message as TMsg;
        let mut out = Vec::new();
        while let Ok(Some(Ok(TMsg::Text(t)))) =
            tokio::time::timeout(Duration::from_millis(100), self.ws.next()).await
        {
            out.push(t.to_string());
        }
        out
    }
}
