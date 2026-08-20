# Week 5 · Day 5 — Simulation & Code Review

> **Format: exam day — no new material.**
>
> Everyone connects at once and chats. Students debug live issues as they surface.
> Deliverable due: the complete Axum WebSocket chat server from Days 1–4.

---

## What runs today

```bash
cargo run                        # serve on 127.0.0.1:3000
cargo run -- demo                # scripted 3-client session, annotated output
cargo run -- storm 20            # 20 clients connect at once, invariants checked
cargo test                       # all tests
```

---

## What's in this folder

```
day5/
├── Cargo.toml
└── src/
    ├── main.rs        # entry point + demo + storm harness
    ├── state.rs       # AppState, room map, name registry
    ├── protocol.rs    # ClientMsg / ServerMsg types
    └── handler.rs     # ws_handler, handle_socket, REST handlers
```

This is Days 1–4 refactored into modules. The logic is identical — the point of
today is to read it under load and find what breaks.

---

## Session shape

| Time | Block |
|---|---|
| 11:00–11:15 | Everyone connects — does the room stay coherent? |
| 11:15–12:00 | Break it on purpose (see scenarios below) |
| 12:00–12:25 | Break |
| 12:25–1:35 | Code review against the checklist |
| 1:35–2:00 | Retro |

---

## Break it on purpose

Each of these reliably exposes a named decision in the code:

- **Close a tab without `/leave`** then check `GET /users`. Is the name still listed?
  (Cleanup must happen in the socket handler's `Drop`/teardown path, not only on a
  Leave message.)
- **Join the same name from two tabs at the same time.** Only one should succeed.
  (The check-and-insert in `state.rs` must be one locked operation.)
- **Join a room, be the only one, open a second tab in the same room, close the
  first.** Does the second tab still receive messages? (Room should survive while any
  subscriber exists.)
- **Send a message that is not valid JSON.** Does the server crash, or does the
  client get an error and stay connected?
- **Open 20 tabs at once** (`cargo run -- storm 20`). Do all 20 connect cleanly?
  Does `/users` report exactly 20 after they all join?

---

## Code review checklist

Work through someone else's Day 4 code, not your own.

**Correctness**
- [ ] Does a client that closes abruptly (no Leave) get cleaned up from the name
      registry?
- [ ] Is the name check-and-insert one critical section, or two separate operations
      that could race?
- [ ] Is `RecvError::Lagged` handled, or does it silently drop the client?
- [ ] Does an invalid JSON message produce an error reply, or a silent disconnect?
- [ ] When a client switches rooms, is the old room receiver dropped before
      subscribing to the new one?

**Resource safety**
- [ ] Is the broadcast channel capacity bounded?
- [ ] Are empty rooms pruned, or do they accumulate forever?
- [ ] Can a name contain whitespace or special characters that would break the UI?

**Structure**
- [ ] Is any `Mutex` lock held across an `.await`? (It must not be.)
- [ ] Could two tasks write to the same WebSocket simultaneously? (They must not.)
- [ ] Does `GET /rooms` reflect the live state, or a stale snapshot?

---

## `cargo run -- storm N`

Spawns N concurrent WebSocket clients, each joining with a unique name, sending
one message, and disconnecting. Checks:

1. All N clients connected (names in `/users` peaked at N).
2. All N clients' messages were broadcast (each client received the others').
3. After all disconnect, `/users` is empty and `/rooms` shows 0 online.

Exit code is non-zero on failure — safe to run in CI.

---

## Retro prompts

- Which bug cost you the most time this week, and what would have caught it faster?
- The REST endpoints and WebSocket handlers share state via `Arc<AppState>`. What
  would break if you used two separate state objects?
- A room with 1000 clients. Every message is cloned N times by `broadcast`. Is
  there a design that avoids the clones? (One channel of `Arc<str>` — subscribers
  share ownership of the string.)
- Where would you add authentication? (An `X-API-KEY` header on the WebSocket
  upgrade request — same pattern as Week 4's write-route guard.)

---

## Carried forward to Week 6

- `Arc<Mutex<T>>` vs `tokio::sync::RwLock` — reads can proceed in parallel, writes
  block. The room map is read far more than written; `RwLock` is a natural upgrade.
- The REST + WebSocket pattern is the foundation of the Week 6 integration project.
- Bounded queues, `Lagged` handling, and cleanup in the teardown path are all
  patterns the Week 6 service reuses.
