# Week 5 · Day 2 — Async TCP & Buffers

> Phase Two: Applied Rust & Systems Engineering
> Master curriculum: [`Phase_Two_Daily_Curriculum_Weeks_4-6.md`](../../../Phase_Two_Daily_Curriculum_Weeks_4-6.md)

---

## From the master curriculum

**Day 2 — Async TCP & Buffers**
- Pre-class: read the Tokio "Spawning" tutorial chapter.
- Topics: `tokio::net::TcpListener`, spawning a task per connection, `read()`/`write()`/`flush()`, buffer/`Bytes` basics.
- Build: convert Day 1's echo server to async/multi-client using `tokio::spawn`.
- Resources: Tokio tutorial (Spawning): https://tokio.rs/tokio/tutorial/spawning

---

## Session shape (11:00 AM – 2:00 PM)

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — what changes and what doesn't when you go async |
| 11:25–12:10 | Live coding — the Day 1 → Day 2 conversion, line by line |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation — convert their own Day 1 server, then load-test it |
| 1:35–2:00 | Code review + debugging |

---

## What's in this folder

```
day2/
├── Cargo.toml
└── src/
    └── main.rs
```

```bash
cargo run                    # the async echo server
cargo run -- flood 1000      # open 1000 concurrent connections against it
cargo run -- raw             # read() into a raw buffer, no line abstraction
```

---

## The conversion, side by side

This is the entire diff from Day 1. Put it on the projector.

| Day 1 (`std::net`) | Day 2 (`tokio::net`) |
|---|---|
| `use std::net::TcpListener` | `use tokio::net::TcpListener` |
| `TcpListener::bind(addr)?` | `TcpListener::bind(addr).await?` |
| `for stream in listener.incoming()` | `loop { let (stream, _) = listener.accept().await?; }` |
| `thread::spawn(move \|\| ...)` | `tokio::spawn(async move { ... })` |
| `std::io::BufReader` | `tokio::io::BufReader` |
| `reader.lines()` (an `Iterator`) | `reader.lines()` + `.next_line().await` |
| `writer.write_all(b)?` | `writer.write_all(b).await?` |
| `use std::io::{Read, Write}` | `use tokio::io::{AsyncReadExt, AsyncWriteExt}` |

Sprinkle `.await`, swap `thread::spawn` for `tokio::spawn`, import the `Async*Ext` traits. The
*shape* of the code is unchanged — that is the design achievement of async/await, and it is
worth saying out loud, because the payoff is enormous:

```
Day 1 threaded : 1000 clients = 1000 OS threads ≈ 8 GB of reserved stack
Day 2 async    : 1000 clients = 1000 tasks      ≈ a few hundred KB
```

Prove it in class:

```bash
# terminal 1
cargo run

# terminal 2
cargo run -- flood 1000
```

Then check the thread and socket count while the flood is running:

```bash
PID=$(pgrep -x day2-async-echo | head -1)
ps -o nlwp= -p $PID          # OS threads
ls /proc/$PID/fd | wc -l     # open file descriptors, mostly sockets
```

Use `pgrep -x` (exact name), not `pgrep -f` — the latter also matches your own shell command
line and hands you the wrong PID.

Measured on an 8-core box, 800-connection flood:

```
idle          : 9 threads,   4 fds
mid-flood     : 9 threads, 629 fds
after         : 9 threads,  10 fds
```

Nine threads — one per core plus the main thread — holding six hundred–odd live sockets, and
the count does not budge under load. Day 1's threaded server would have needed 800 threads for
the same work. That is the entire argument, in two numbers.

---

## Concept notes for the 11:00 block

**The `Async*Ext` import is the part everyone forgets.** `AsyncReadExt` and `AsyncWriteExt`
provide `read`, `write_all`, `read_line` and friends as extension methods on the base traits.
Forget the import and you get "no method named `write_all`" on a type that obviously has it. It
is the single most common day-two compile error; name it before the lab, not after.

**`tokio::spawn` needs `Send + 'static`, `thread::spawn` needs the same.** This is not new — it
is the same bound for the same reason as Week 4 Day 1. The work may move between worker
threads, and it may outlive the caller. Students who internalised that in Week 4 get this for
free.

**Blocking inside async poisons the whole runtime.** This is the day's real hazard, and it does
not exist in the Day 1 model at all. Under the multi-thread scheduler, a `std::thread::sleep`
or a synchronous file read inside a task stalls that entire worker thread — every other task
scheduled on it is frozen. The `flood` mode makes this visible: the server has a commented-out
`std::thread::sleep` in `handle_client`; uncomment it, re-run the flood, and throughput
collapses.

For genuinely blocking work, `tokio::task::spawn_blocking` moves it to a separate pool. Mention
it; they will need it in Phase Three when they touch synchronous crypto libraries.

**Buffers and the `raw` mode.** `read(&mut buf)` fills as much as is available and returns how
many bytes it got — which is the honest interface TCP actually offers. `BufReader::lines()` is
a convenience layered on top that hides the reassembly. Run `cargo run -- raw` and watch a
single logical message arrive across several reads. This is Day 3's problem stated one day
early, deliberately: today they see it, tomorrow they solve it properly.

**`read` returning `Ok(0)` still means EOF.** Same as Day 1, and still the disconnect signal.
Async did not change TCP semantics; it changed how you wait for them.

**Why `flush` still matters.** `BufWriter` buffers the same way it did yesterday. Async does
not flush for you.

---

## Talking points during code review (1:35)

- How many OS threads is the server using during the flood? Why that number?
- What would `std::thread::sleep(Duration::from_secs(1))` inside `handle_client` do to the
  other 999 clients? Try it.
- `join!` or `spawn` for connection handling — which, and why? (Spawn: connections are
  independent and unbounded in number.)
- What happens to an in-flight task when `main` returns? (Week 4 Day 1, demo 5.)
- In `raw` mode, is `n == 0` an error or a normal event?
- Why do we still need `try_clone`/`split` to read and write at the same time?

---

## Homework (standing)

- Read the official docs for today's topic — `tokio::net::TcpListener`, `tokio::io`.
- Read one crate's source for 15–20 minutes. Today: `tokio::io::BufReader`.
- Solve or review one Rustlings exercise.
- Refactor yesterday's code.
- Write at least one unit test.

Today specifically: add a per-connection idle timeout with `tokio::time::timeout` — notice it
is one wrapping call, versus the `set_read_timeout` awkwardness of Day 1. Then add a graceful
shutdown on Ctrl-C that lets in-flight clients finish, using `tokio::select!`.
