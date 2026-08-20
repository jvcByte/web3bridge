# Week 5 · Day 1 — Axum & HTTP Recap

> Building on Week 4: same Axum stack, now adding a WebSocket upgrade path.

---

## What you're building today

A minimal Axum server with three things:

1. A `GET /health` route that returns JSON — same pattern as Week 4.
2. A `GET /` route that serves a plain HTML chat page (hardcoded string, no files).
3. A `GET /ws` route that accepts a WebSocket upgrade and echoes every message back.

No broadcast yet. No shared state. Just the WebSocket handshake and the echo loop.

---

## Session shape

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — what a WebSocket upgrade is; how it differs from a normal HTTP response |
| 11:25–12:10 | Live coding — the echo server |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation |
| 1:35–2:00 | Code review |

---

## The WebSocket upgrade in Axum

A WebSocket connection starts life as an HTTP `GET` with two special headers:

```
Upgrade: websocket
Connection: Upgrade
```

Axum handles the HTTP side for you. Your handler receives an `axum::extract::WebSocketUpgrade`
extractor. You call `.on_upgrade(|socket| async move { … })` on it, and Axum sends the 101
response and hands you the live socket.

```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;

async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(Ok(msg)) = socket.recv().await {
        if socket.send(msg).await.is_err() {
            break;
        }
    }
}
```

`socket.recv()` yields `Message` values. The variants you care about today:
- `Message::Text(s)` — a UTF-8 string frame.
- `Message::Binary(b)` — raw bytes.
- `Message::Close(_)` — the peer is done. Stop the loop.
- `Message::Ping(_)` — Axum handles pong automatically; you can ignore these.

---

## What's in this folder

```
day1/
├── Cargo.toml
└── src/
    └── main.rs
```

```bash
cargo run
# then open http://localhost:3000 in a browser — the chat page loads
# type something — it echoes back
```

Or with `websocat` (a CLI WebSocket client):

```bash
websocat ws://localhost:3000/ws
hello
hello        # echoed back
```

---

## Talking points during code review

- Where does the HTTP connection end and the WebSocket connection begin?
- What does `on_upgrade` return, and why does it need to be `impl IntoResponse`?
- The socket loop breaks on a send error. What causes that? (The client closed.)
- Why does `Message::Ping` not need special handling here?
- What happens if two browser tabs open at the same time? (They each get their own socket — no shared state yet, so they cannot see each other. That is Day 2.)

---

## Cargo.toml

```toml
[dependencies]
axum      = { version = "0.8", features = ["ws"] }
tokio     = { version = "1",   features = ["full"] }
serde     = { version = "1",   features = ["derive"] }
serde_json = "1"
```

---

## Homework

- Add a `/ws` query param `?name=alice` that the echo server uses to prefix its replies: `alice: hello`.
- Read the Axum `WebSocketUpgrade` docs: https://docs.rs/axum/latest/axum/extract/ws/index.html
- What is the difference between `Message::Text` and `Message::Binary`? When would you use each?
