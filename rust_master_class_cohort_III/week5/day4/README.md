# Week 5 · Day 4 — Rooms & REST + WebSocket Together

> Multiple rooms. The REST API from Week 4 lives alongside the WebSocket server.

---

## What you're building today

Two additions on top of Day 3:

1. **Rooms** — clients join a named room. Messages only reach clients in the same room.
2. **REST + WebSocket on one server** — the `/rooms` and `/users` REST endpoints from your Week 4 work sit alongside the WebSocket `/ws` route in the same Axum app.

This is the payoff for building on Axum from the start: you just add routes.

---

## Session shape

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — one broadcast channel per room vs one channel with a tag |
| 11:25–12:10 | Live coding — room map, join routing, REST endpoints |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation |
| 1:35–2:00 | Code review |

---

## One channel per room

The obvious alternative is one global channel with a `room` field, filtering on
arrival. The problem: a hundred messages in `#general` push older messages off the
buffer of someone sitting quietly in `#help` — they get `Lagged` for traffic that
was never meant for them. Rooms stop isolating traffic, which is most of what a
room is for.

**One `broadcast::Sender` per room.** Each room entry in the map holds its own
channel. Joining means subscribing to that room's sender.

```rust
type RoomMap = Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>;
```

When a room has no subscribers left, `send()` returns `Err(SendError)` because
there are no receivers. That is the signal to remove it from the map — rooms are
created on demand and destroyed when they empty.

```rust
fn get_or_create_room(rooms: &RoomMap, name: &str) -> broadcast::Sender<String> {
    let mut map = rooms.lock().unwrap();
    if let Some(tx) = map.get(name) {
        if tx.receiver_count() > 0 {
            return tx.clone();
        }
    }
    let (tx, _) = broadcast::channel(64);
    map.insert(name.to_string(), tx.clone());
    tx
}
```

---

## REST endpoints (same pattern as Week 4)

| Method | Path | Returns |
|---|---|---|
| `GET` | `/rooms` | `[{ "name": "rust", "online": 3 }, …]` |
| `GET` | `/users` | `["alice", "bob", "carol"]` |
| `GET` | `/health` | `{ "status": "ok", "connected": 5 }` |

These read from the same `Arc<AppState>` the WebSocket handlers use. No new
concepts — just Axum state extraction, same as Week 4.

---

## Updated message protocol

Add `room` to the `Join` message:

```json
{ "type": "join", "name": "alice", "room": "rust" }
```

Server → client, add a `room_changed` event:

```json
{ "type": "room_changed", "room": "rust", "online": 4 }
```

---

## What's in this folder

```
day4/
├── Cargo.toml
└── src/
    └── main.rs
```

```bash
cargo run
# open two tabs at http://localhost:3000
# join different rooms — they cannot see each other
# join the same room — they can
# GET http://localhost:3000/rooms
# GET http://localhost:3000/users
```

---

## Talking points during code review

- A client joins `#rust`, then joins `#general`. The old `broadcast::Receiver` is
  just dropped — the sender's `receiver_count` drops by one. Is that enough cleanup?
- Two clients join a room simultaneously. Can both see the room as empty and both
  try to create it? (Lock covers the check-and-create as one step.)
- `GET /rooms` reads the room map under a lock. Is holding that lock across a
  JSON serialisation a problem? (No — it is pure computation, no `.await`.)
- What happens to a room when the last client leaves? When does it get removed
  from the map?

---

## Homework

- Add a `GET /rooms/:name/history` endpoint that returns the last 20 messages for
  a room. You will need to store messages — a `VecDeque<String>` per room, capped
  at 20, updated on every broadcast.
