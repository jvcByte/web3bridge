# Week 4 · Day 1 — Async Rust Foundations

> Phase Two: Applied Rust & Systems Engineering
> Master curriculum: [`Phase_Two_Daily_Curriculum_Weeks_4-6.md`](../../../Phase_Two_Daily_Curriculum_Weeks_4-6.md)

---

## From the master curriculum

**Day 1 — Async Rust Foundations**
- Pre-class: Rust Async Book intro + Tokio tutorial intro.
- Topics: sync vs async, Futures, `async fn`, the Tokio runtime, `.await`, tasks, `spawn`.
- Live coding: `tokio::spawn()`, `async fn`, `sleep()`, `join!`.
- Lab: build `download_file()`, `download_image()`, `download_json()` (simulated with `sleep`) running concurrently via `join!`/`spawn`.
- Resources:
  - Rust Async Book: https://rust-lang.github.io/async-book/part-guide/async-await.html
  - Tokio tutorial: https://tokio.rs/tokio/tutorial
  - Video — Let's Get Rusty, "Async Programming in Rust": https://youtu.be/ThjvMReOXYM

---

## Session shape (11:00 AM – 2:00 PM)

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — why async exists, not just how to use it |
| 11:25–12:10 | Live coding — `tokio::spawn`, `async fn`, `sleep`, `join!` |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation — the three downloaders |
| 1:35–2:00 | Code review + debugging |

Open with a 5-minute check-in on the pre-class reading.

---

## What's in this folder

```
day1/
├── Cargo.toml
└── src/
    └── main.rs    # five demos, run top to bottom
```

`src/main.rs` walks through five stages in order. Each prints its own wall-clock time so
the difference is visible, not theoretical.

1. **`demo_lazy_futures`** — an `async fn` that is called but never awaited. Nothing happens.
2. **`demo_sequential`** — three downloads, each `.await`ed in turn. ~6s.
3. **`demo_join`** — the same three via `tokio::join!`. ~3s, one task.
4. **`demo_spawn`** — the same three via `tokio::spawn`, moved onto the runtime. ~3s, three tasks.
5. **`demo_spawn_racing`** — spawn without awaiting the handles, to show tasks are
   detached and `main` returning kills them.

---

## Running it

```bash
cargo run
```

To watch the runtime scheduling decisions:

```bash
cargo run 2>&1 | ts '%.s'   # if you have moreutils; otherwise just read the printed elapsed times
```

---

## Concept notes for the 11:00 block

**Why async at all.** A thread parked on a blocking `read()` costs ~8KB–2MB of stack and a
kernel scheduling slot to do nothing. A future parked on `.await` costs a few bytes in a state
machine. For an I/O-bound server — which is every server in this phase — that is the whole
argument. Async is not faster at computing; it is cheaper at waiting.

**Futures are lazy.** This is the single most common day-one bug. In JavaScript, calling an
`async` function starts the work. In Rust, calling an `async fn` builds a value that
implements `Future` and does nothing at all. Demo 1 exists to make students feel this before
they hit it in their own code. Live edit worth doing: delete the `let result = future.await;`
line at the end of `demo_lazy_futures` and rebuild. The compiler now emits
`unused_must_use: futures do nothing unless you '.await' or poll them` — read that message out
loud, because it is the compiler explaining the entire concept in one sentence.

**What `.await` actually does.** It hands control back to the executor and says "poll me again
when my waker fires." The `async fn` body is compiled into a state machine; each `.await` is a
state boundary where locals are stored so the whole thing can be resumed later. Ask the class:
where does the state machine live?

**`join!` vs `spawn`.** `join!` polls its futures concurrently on the *current* task — one task,
interleaved. `spawn` hands each future to the runtime as an independent task, so they can land
on *different threads* under the multi-thread scheduler. Hence `spawn` requires
`Send + 'static` and `join!` does not. Demo 3 and 4 finish in the same wall-clock time for very
different reasons, and that distinction is the point of the day.

**`std::thread::sleep` inside async is a bug.** It blocks the OS thread and stalls every other
task on that worker. Always `tokio::time::sleep`. Worth writing on the board.

---

## Talking points during code review (1:35)

- Why does `demo_spawn` need `String` or `&'static str` and not a borrowed local?
- What happens if you `join!` two futures and one panics?
- Is `join!` parallelism or concurrency? (Concurrency. Only `spawn` on the multi-thread
  runtime buys parallelism.)
- What does `#[tokio::main]` expand to? Have someone run `cargo expand` if it's installed.

---

## Homework (standing, every day this phase)

- Read the official docs for today's topic — `tokio::time`, `tokio::task`.
- Read one crate's source for 15–20 minutes. Today: `tokio::time::sleep`.
- Solve or review one Rustlings exercise.
- Refactor yesterday's code.
- Write at least one unit test.

Today specifically: add a `download_with_timeout()` using `tokio::time::timeout` that returns
`Result` and fails one of the three downloads on purpose.

Research order, in priority: **official docs → docs.rs → crate source → search engines/LLMs last.**
