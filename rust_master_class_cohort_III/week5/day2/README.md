# Week 5 · Day 2 — Shared State & Broadcast

> Multiple clients, one room. Everyone sees everyone.

---

## What you're building today

Extend Day 1's echo server so messages from one client reach all connected clients.

That requires two things:
1. **Shared state** — a place to keep the list of connected clients, accessible from every handler task.
2. **A broadcast channel** — so a message arriving on one WebSocket task can be sent to all others.

Both patterns are already familiar from Week 4: `Arc<Mutex<T>>` for shared state, and today you add `tokio::sync::broadcast` for fan-out.

---

## Session shape

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — why a broadcast channel fits this problem |
| 11:25–12:10 | Live coding — shared state, broadcast, the fan-out loop |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation |
| 1:35–2:00 | Code review |

---

## The design

```
client A  ──▶  ws handler  ──▶  broadcast::Sender  ──▶  client A receiver  ──▶  socket A
client B  ──▶  ws handler  ──▷                     ──▶  client B receiver  ──▶  socket B
client C  ──▶  ws handler  ──▷                     ──▶  client C receiver  ──▶  socket C
```

Each connection task:
1. Subscribes to the broadcast channel (`tx.subscribe()`) before announcing anything.
2. Splits into two concurrent loops using `tokio::select!`:
   - one reading from the WebSocket and publishing to the channel.
   - one reading from the broadcast receiver and writing to the WebSocket.

```rust
// Shared across all handlers via Axum state
#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.tx.subscribe();

    loop {
        tokio::select! {
            // inbound: client → room
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => { let _ = state.tx.send(text); }
                    _ => break,
                }
            }
            // outbound: room → client
            msg = rx.recv() => {
                match msg {
                    Ok(text) => { if socket.send(Message::Text(text)).await.is_err() { break; } }
                    Err(RecvError::Lagged(n)) => { /* missed n messages */ }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    }
}
```

**Why `broadcast` and not `Mutex<Vec<Sender>>`?**

With a shared vec you lock it, iterate, and `.await` each send while holding the lock.
One slow client stalls everyone. `broadcast` gives each client its own receiver buffer — a slow client falls behind alone.

---

## What's in this folder

```
day2/
├── Cargo.toml
└── src/
    └── main.rs
```

```bash
cargo run
# open two browser tabs at http://localhost:3000
# type in one — both tabs see it
```

---

## Talking points during code review

- Subscribe *before* announcing the join. Why? (A `broadcast::Receiver` only sees messages sent after it was created.)
- What does `RecvError::Lagged(n)` mean? Is it fatal? (No — the receiver is still valid. Tell the client they missed `n` messages.)
- The sender's own message comes back to them from the channel. Is that right? (For a chat room it's usually fine — you see your own message appear. Filter by sender ID if you don't want that.)
- Open three tabs. Close one abruptly. Does the server crash? Why not?

---

## Homework

- Track how many clients are connected with an `AtomicUsize` (same pattern as Week 4 Day 1's request counter).
- Prefix every message with a timestamp so clients can tell when things arrived.
