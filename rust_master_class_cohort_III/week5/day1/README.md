# Week 5 · Day 1 — TCP Fundamentals

> Phase Two: Applied Rust & Systems Engineering
> Master curriculum: [`Phase_Two_Daily_Curriculum_Weeks_4-6.md`](../../../Phase_Two_Daily_Curriculum_Weeks_4-6.md)

---

## From the master curriculum

**Day 1 — TCP Fundamentals**
- Pre-class: read `std::net` docs + skim the first sections of Beej's Guide.
- Topics: sockets, ports, streams, `TcpListener`/`TcpStream`, the accept loop, blocking I/O.
- Build: a blocking single-client echo server with `std::net`.
- Resources:
  - std::net docs: https://doc.rust-lang.org/std/net/
  - Beej's Guide to Network Programming: https://beej.us/guide/bgnet/

---

## Session shape (11:00 AM – 2:00 PM)

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — what a socket is; what "stream" means and what it doesn't |
| 11:25–12:10 | Live coding — `TcpListener`, accept loop, read/write |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation — the echo server, then the blocking experiment |
| 1:35–2:00 | Code review + debugging |

**Zero dependencies today.** `std::net` only. Tomorrow tokio makes this easy; today they should
feel why it needed making easy.

---

## What's in this folder

```
day1/
├── Cargo.toml       (no dependencies — deliberately)
└── src/
    └── main.rs
```

Three binaries in one file, selected by argument:

```bash
cargo run                 # or `cargo run -- serve` — the blocking echo server
cargo run -- threaded     # same, one thread per client
cargo run -- client       # a minimal test client
```

The default `serve` mode handles **one client at a time**, on purpose. That is not a bug to fix
today; it is the observation the whole week is built on.

---

## Running it

Terminal 1:

```bash
cargo run
```

Terminal 2:

```bash
nc localhost 7878
hello
HELLO
```

Now the experiment that makes the point. Open a **third** terminal while terminal 2 is still
connected:

```bash
nc localhost 7878
# type something. nothing comes back.
```

The third client's TCP connection is accepted by the kernel — it sits in the listen backlog —
but the server never calls `accept()` for it because it is blocked inside `handle_client` for
terminal 2. Close terminal 2 and terminal 3 springs to life instantly.

**Have every student do this.** It is the single most important thing they see today.

Then try the threaded mode:

```bash
cargo run -- threaded
```

Now both clients work. Ask: how many clients before this falls over? What is the cost per
client? (Default 8 MiB of virtual stack reservation per thread on Linux, plus a kernel
scheduling entity. 10,000 clients is 10,000 threads.) That question is what tokio answers
tomorrow.

---

## Concept notes for the 11:00 block

**A socket is a file descriptor.** On Unix, `TcpStream` wraps an integer the kernel hands you.
Reading from a socket is the same syscall family as reading from a file. This is why
`std::io::Read` and `Write` are the traits used here rather than anything network-specific, and
why `io::copy` works between a file and a socket unchanged.

**"Stream" means there are no messages.** This is the day's most important idea and it takes
until Day 3 to fully land, so start it now. TCP guarantees that the bytes you send arrive, in
order, without duplication. It guarantees **nothing** about how they are grouped. Send
`"hello\n"` and `"world\n"` in two `write` calls; the receiver may get:

- `"hello\nworld\n"` in one read
- `"hello\n"` then `"world\n"` in two reads
- `"hel"`, then `"lo\nwo"`, then `"rld\n"` in three

All three are correct TCP. Nagle's algorithm coalesces small writes; MTU limits split large
ones. If your code assumes one `read()` equals one message, it works on localhost and breaks in
production — which is the worst possible failure mode. Day 3 fixes this properly with framing.

Demonstrate it: `printf 'a' ; sleep 0.1 ; printf 'b\n'` piped into `nc` versus `printf 'ab\n'`.

**`read()` returning `Ok(0)` means EOF, not "nothing to read".** A zero-length read is the peer
having closed its end. This is *the* disconnect signal, and forgetting to treat it as a
terminating condition gives you the classic infinite loop spinning at 100% CPU. There is a
`debug_assert`-worthy comment on this in the code.

**Ports below 1024 need root.** 7878 is arbitrary and fine. `SO_REUSEADDR` is why you can
restart the server immediately instead of waiting out `TIME_WAIT` — `std::net` does not expose
it, which is one of several reasons real servers reach for `socket2` or tokio.

**Blocking is not inherently bad.** One thread per connection is simple, easy to debug, and
correct for a few hundred concurrent clients. It stops scaling when connections are numerous and
mostly idle — which is exactly what a chat server is. Do not let them conclude that threads are
wrong; let them conclude that threads are wrong *for this shape of workload*.

---

## Talking points during code review (1:35)

- Where exactly does the server block? Name the syscall. (`accept`, and `read`.)
- Why does `Ok(0)` end the loop? What happens if you `continue` instead?
- What is in the listen backlog while client 1 is connected?
- The threaded version never joins its handles. Is that a leak? What happens at shutdown?
- Why does the echo need `flush()`? What is `BufWriter` holding onto?
- If two clients connect to the threaded server, do they share anything at all? (No — which is
  why it cannot be a chat server yet. That is Day 3.)

---

## Homework (standing)

- Read the official docs for today's topic — `std::net::TcpListener`, `std::io::Read`.
- Read one crate's source for 15–20 minutes. Today: `std::io::BufReader::read_line`.
- Solve or review one Rustlings exercise.
- Refactor yesterday's code.
- Write at least one unit test.

Today specifically: make the threaded server keep a `Vec<JoinHandle>` and shut down cleanly on
a `/quit` command. Then use `set_read_timeout` to drop a client that sends nothing for 30
seconds, and notice how awkward timeouts are in the blocking model — tomorrow they become one
line.
