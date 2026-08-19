# Week 5 · Day 3 — Framing & Protocol Design

> Phase Two: Applied Rust & Systems Engineering
> Master curriculum: [`Phase_Two_Daily_Curriculum_Weeks_4-6.md`](../../../Phase_Two_Daily_Curriculum_Weeks_4-6.md)

---

## From the master curriculum

**Day 3 — Framing & Protocol Design**
- Pre-class: skim `tokio_util::codec` docs.
- Topics: why TCP is a byte stream and not a message stream, delimiter framing vs
  length-prefix framing, `LinesCodec`, designing a tiny line protocol.
- Build: a client/server that exchanges structured commands (`/nick`, `/join`, `/msg`).
- Resources: `tokio_util::codec`: https://docs.rs/tokio-util/latest/tokio_util/codec/

---

## Session shape (11:00 AM – 2:00 PM)

| Time | Block |
|---|---|
| 11:00–11:25 | Concept — the byte-stream problem, and the two ways out |
| 11:25–12:10 | Live coding — hand-rolled length-prefix codec, then `LinesCodec` |
| 12:10–12:25 | Break |
| 12:25–1:35 | Student implementation — design and implement their own command protocol |
| 1:35–2:00 | Code review + debugging |

---

## What's in this folder

```
day3/
├── Cargo.toml
└── src/
    ├── main.rs        # demo runner
    ├── protocol.rs    # the command/event types + parsing
    └── framing.rs     # hand-rolled length-prefix codec
```

```bash
cargo run -- broken      # why naive framing fails, reproducibly
cargo run -- lines       # LinesCodec: delimiter framing done properly
cargo run -- length      # hand-rolled length-prefix codec
cargo run -- protocol    # the command protocol, parsed round-trip
cargo test               # the framing + parsing test suite
```

---

## The core idea

Day 1 showed the symptom. Today is the cure.

TCP guarantees that **bytes arrive in order and uncorrupted**. It guarantees nothing about
*grouping*. `write_all(b"hello\n")` on one side does not imply one `read` on the other. You may
get `hel`, then `lo\n`. You may get `hello\nworld\n` in a single read. Both are correct TCP.

So every stream protocol must carry its own message boundaries. There are exactly two
mechanisms in wide use:

**Delimiter framing.** Pick a byte that cannot appear in the payload — usually `\n` — and split
on it. Simple, human-readable, debuggable with `nc` and `telnet`. HTTP/1.1 headers, SMTP, IRC,
and Redis's inline commands all work this way.

The catch: the delimiter must be impossible in the payload, or you must escape it. A chat
message containing a newline breaks a naive `\n`-delimited protocol. And a peer that never
sends the delimiter can make you buffer forever — `LinesCodec::new_with_max_length` exists for
exactly that reason, and using plain `new()` on a public socket is a memory-exhaustion bug.

**Length-prefix framing.** Write the payload length first, as a fixed-width integer, then the
payload. The reader reads the length, then reads exactly that many bytes. Payload can be
arbitrary binary — no escaping, no forbidden bytes. This is what gRPC, most database wire
protocols, and Ethereum's own devp2p do.

The catch: not human-readable, and you must bound the length before you allocate. A peer
claiming `u32::MAX` bytes must be rejected, not honoured with a 4 GiB allocation. `framing.rs`
enforces this and there is a test for it.

Rule of thumb: text protocol for humans and debugging, length-prefix for binary and
performance. Chat is text, so Days 4 and 5 use `LinesCodec`.

---

## `cargo run -- broken` — reproduce the bug on purpose

The naive framing bug is hard to hit locally, because on loopback with small messages you
usually get one write per read and everything appears to work. It then fails in production.

So `broken` mode forces it: the sender splits a single logical message across two writes with a
delay, and the "one read equals one message" reader mangles it. Same input, correct framing,
correct output. Run it before the concept talk — a bug they have watched happen is worth more
than a bug they have been warned about.

---

## Concept notes for the 11:00 block

**`Framed` turns a socket into a `Stream` + `Sink`.** Once you wrap a `TcpStream` in
`Framed::new(stream, LinesCodec::new())`, you stop thinking in bytes and start thinking in
messages. `framed.next().await` yields a `String`; `framed.send(line).await` sends one. This is
the abstraction Day 4's broadcast loop is built on, and it is why Day 4's code stays readable.

`Stream` is the async analogue of `Iterator`; `Sink` is the async analogue of "something you
push into". Both come from the `futures` crate, which is why `tokio-util`'s codecs require it
and why you need `use futures::{SinkExt, StreamExt}` to get `.send()` and `.next()`.

**`split()` on a `Framed`.** `framed.split()` gives a sink half and a stream half that can move
to separate tasks. This is the same move as Day 2's `into_split`, one level up the stack. Day 4
uses it: one task pumps broadcasts into the sink, another reads commands from the stream.

**Codec errors are not connection errors.** `LinesCodec` yields
`Err(LinesCodecError::MaxLineLengthExceeded)` for an over-long line. That is a *protocol*
error — the connection is still fine, and the right response is usually to send the client an
error message and keep going, not to drop them. Conflating the two is how you get a server that
disconnects people for typos. Day 4's error handling makes this distinction explicit.

**Design the protocol before writing the parser.** Ask the room to specify the chat protocol on
the whiteboard before anyone opens an editor: what commands exist, what arguments each takes,
what the server sends back, what happens on malformed input. Write it down. Then implement it.
`protocol.rs` is one such answer, and its doc comment is the specification.

The parsing decisions worth making explicit:

- `/nick` with no argument: error, not a panic. Every command parser needs a "missing argument"
  path, and `split_whitespace` + `next()` gives you `Option`, which forces you to handle it.
- `/msg alice hello world` — the message is everything after the nick, spaces included. Use
  `splitn(3, ' ')`, not `split_whitespace().collect()`, or you will silently collapse spacing.
- Unknown commands: report them. Silently ignoring input is the least debuggable failure mode
  there is.
- Bare text with no leading `/` is a message to the current room. Most chat protocols work this
  way and it is what users expect.

**Where this goes in Phase Three.** Ethereum's devp2p is length-prefixed and RLP-encoded; the
JSON-RPC they will use in Week 7 is newline-delimited JSON over a socket or HTTP body. Both
mechanisms from today, in the wild. Say so — it makes the day feel less like an exercise.

---

## Talking points during code review (1:35)

- Your protocol gets a message containing a newline. What happens? What should?
- A client sends a 10 MB line. What does your server do? What should it do?
- A client sends a length prefix of `0xFFFFFFFF`. What does your reader do?
- Why is `LinesCodec::new()` a vulnerability on a public port and `new_with_max_length` not?
- Half-open connections: a client that sends a length prefix and then nothing. How long do you
  wait? (Answer: a timeout, `tokio::time::timeout`. Day 2's homework.)
- Would you use text or length-prefix framing for a file transfer? For a chat? For a game?

---

## Homework (standing)

- Read the official docs for today's topic — `tokio_util::codec`, `futures::Stream`.
- Read one crate's source for 15–20 minutes. Today: `tokio_util::codec::LinesCodec` — it is
  about 100 lines and repays reading in full.
- Solve or review one Rustlings exercise.
- Refactor yesterday's code.
- Write at least one unit test.

Today specifically: implement a `Decoder`/`Encoder` for your own protocol so the socket yields
`Command` values directly instead of `String`s you then parse. That is the natural next step and
it makes Day 4 shorter.
