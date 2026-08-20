# Week 5 · Day 3 — Nicknames & JSON Messages

> Named users. Structured messages. The chat starts to look real.

---

## What you're building today

Two additions on top of Day 2:

1. **Nicknames** — clients send a `{"type":"join","name":"alice"}` message when they connect. The server stores the name and prepends it to every chat message.
2. **JSON messages** — replace raw strings with a typed message format using Serde. Both client → server and server → client messages are JSON.

No new WebSocket mechanics. This is about data modelling with Serde — the same skill from Week 4.

---

## Session shape

| Time | Block |
|---|---|
| 11:00–11:20 | Recap — current state, what's missing |
| 11:20–12:10 | Live coding — message types, JSON framing, per-client name storage |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation |
| 1:35–2:00 | Code review |

---

## Message types

Client → server:

```json
{ "type": "join",    "name": "alice" }
{ "type": "message", "text": "hello" }
{ "type": "leave" }
```

Server → client:

```json
{ "type": "joined",   "name": "alice", "online": 3 }
{ "type": "left",     "name": "alice", "online": 2 }
{ "type": "message",  "name": "alice", "text": "hello" }
{ "type": "error",    "text": "name already taken" }
```

In Rust, model these as enums with `#[serde(tag = "type", rename_all = "lowercase")]`:

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientMsg {
    Join { name: String },
    Message { text: String },
    Leave,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ServerMsg {
    Joined { name: String, online: usize },
    Left    { name: String, online: usize },
    Message { name: String, text: String },
    Error   { text: String },
}
```

---

## Per-client state

Each connection needs to know its current nickname. That is local to the handler task — not shared state. A simple `Option<String>` local variable is enough.

The *set of taken names* is shared. Use the same `Arc<Mutex<HashSet<String>>>` pattern from Week 4.

```rust
struct AppState {
    tx:    broadcast::Sender<String>,  // still broadcasts serialised JSON
    names: Mutex<HashSet<String>>,
    count: AtomicUsize,
}
```

---

## What's in this folder

```
day3/
├── Cargo.toml
└── src/
    └── main.rs
```

```bash
cargo run
# open http://localhost:3000, enter a name, chat
```

---

## Talking points during code review

- What happens if a client sends a `message` before a `join`? (Return a JSON error, don't broadcast.)
- What happens if two clients try the same name at the same time? (The `Mutex<HashSet>` makes the check-and-insert one operation — no race.)
- The broadcast channel still carries `String` (serialised JSON). Could it carry `ServerMsg` directly? What would that require? (`Clone + Send`)
- A client that sends invalid JSON — error or disconnect? (Error is friendlier.)

---

## Homework

- Add a `/users` REST endpoint (no WebSocket) that returns the current list of names as JSON — same pattern as Week 4's `GET /posts`.
- Validate the name: reject empty strings and names longer than 20 characters.
