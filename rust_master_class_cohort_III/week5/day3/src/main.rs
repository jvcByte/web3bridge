//! Week 5 · Day 3 — Framing & Protocol Design
//!
//!   cargo run -- broken      # reproduce the naive-framing bug on purpose
//!   cargo run -- lines       # LinesCodec — delimiter framing done properly
//!   cargo run -- length      # the hand-rolled length-prefix codec over a socket
//!   cargo run -- protocol    # the command protocol, parsed round-trip
//!   cargo test               # 22 tests covering both codecs and the parser

mod framing;
mod protocol;

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LinesCodec};

use framing::LengthPrefixed;
use protocol::{Command, Event, MAX_LINE};

#[tokio::main]
async fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "broken".into());

    match mode.as_str() {
        "broken" => demo_broken().await,
        "lines" => demo_lines().await,
        "length" => demo_length().await,
        "protocol" => demo_protocol(),
        other => {
            eprintln!("unknown mode {other:?}; try: broken | lines | length | protocol");
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// 1. The bug, reproduced deliberately.
// ---------------------------------------------------------------------------

/// Naive framing — "one read is one message" — with a sender that splits a
/// message across two writes.
///
/// On loopback with small payloads this usually appears to work, which is exactly
/// what makes it dangerous: it passes local testing and fails in production. So
/// this forces the failure with an explicit delay between writes.
async fn demo_broken() {
    println!("=== 1. Naive framing: one read == one message ===\n");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 512];
        let mut received = Vec::new();

        loop {
            match stream.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    // THE BUG: treating each read as exactly one message.
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    println!("  server read {n:>3} bytes -> treated as one message: {chunk:?}");
                    received.push(chunk);
                }
                Err(_) => break,
            }
        }

        received
    });

    let mut client = TcpStream::connect(addr).await.unwrap();

    // One logical message, split across two writes with a pause. TCP delivers
    // the bytes correctly; the reader's assumption is what breaks.
    println!("  client writes \"hel\", pauses 150ms, writes \"lo\\n\"");
    client.write_all(b"hel").await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    client.write_all(b"lo\n").await.unwrap();
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    // Three logical messages in a single write.
    println!("  client writes \"one\\ntwo\\nthree\\n\" in ONE write");
    client.write_all(b"one\ntwo\nthree\n").await.unwrap();
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;
    drop(client);

    let received = server.await.unwrap();

    println!("\n  the server saw {} \"messages\":", received.len());
    for (i, msg) in received.iter().enumerate() {
        println!("    {i}: {msg:?}");
    }
    println!("\n  It should have seen 4: \"hello\", \"one\", \"two\", \"three\".");
    println!("  One message arrived split; three arrived merged. Both are correct TCP.");
    println!("  The reader's assumption is the bug — and it is invisible on a fast");
    println!("  local link with small payloads, which is why it ships.\n");
}

// ---------------------------------------------------------------------------
// 2. LinesCodec — delimiter framing.
// ---------------------------------------------------------------------------

/// The same traffic, through `Framed` + `LinesCodec`.
///
/// `Framed` turns the socket into a `Stream` of `String`s and a `Sink` you can
/// push `String`s into. Reassembly stops being your problem.
async fn demo_lines() {
    println!("=== 2. LinesCodec: delimiter framing ===\n");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();

        // `new_with_max_length`, never bare `new()`, on anything a stranger can
        // connect to: a peer that never sends a newline would otherwise make the
        // codec buffer until the process dies.
        let mut framed = Framed::new(stream, LinesCodec::new_with_max_length(MAX_LINE));

        let mut received = Vec::new();
        while let Some(result) = framed.next().await {
            match result {
                Ok(line) => {
                    println!("  server got one complete line: {line:?}");
                    received.push(line);
                }
                Err(e) => {
                    // A protocol error, not a connection error. The socket is
                    // still healthy — for a chat server the right move is to tell
                    // the client and keep going, not to disconnect them.
                    println!("  codec error (connection still fine): {e}");
                }
            }
        }
        received
    });

    let mut client = TcpStream::connect(addr).await.unwrap();

    println!("  client writes exactly the same bytes as demo 1");
    client.write_all(b"hel").await.unwrap();
    client.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    client.write_all(b"lo\n").await.unwrap();
    client.write_all(b"one\ntwo\nthree\n").await.unwrap();
    client.flush().await.unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;
    drop(client);

    let received = server.await.unwrap();

    println!("\n  the server saw {} messages: {received:?}", received.len());
    println!("  Split writes reassembled, merged writes separated. Same bytes,");
    println!("  correct framing.\n");
}

// ---------------------------------------------------------------------------
// 3. The hand-rolled length-prefix codec, over a real socket.
// ---------------------------------------------------------------------------

async fn demo_length() {
    println!("=== 3. Length-prefix framing (hand-rolled Decoder/Encoder) ===\n");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, LengthPrefixed);

        let mut received = Vec::new();
        while let Some(result) = framed.next().await {
            match result {
                Ok(msg) => {
                    println!("  server got a {}-byte frame: {msg:?}", msg.len());
                    received.push(msg);
                }
                Err(e) => println!("  frame error: {e}"),
            }
        }
        received
    });

    let stream = TcpStream::connect(addr).await.unwrap();
    let mut framed = Framed::new(stream, LengthPrefixed);

    framed.send("hello".to_string()).await.unwrap();

    // The payoff over delimiter framing: the content can contain the byte a line
    // protocol would frame on, with no escaping.
    framed
        .send("a message\nwith embedded\nnewlines".to_string())
        .await
        .unwrap();

    framed.send(String::new()).await.unwrap();

    drop(framed);
    tokio::time::sleep(Duration::from_millis(150)).await;

    let received = server.await.unwrap();
    println!("\n  {} frames received intact, newlines and all.", received.len());
    println!("  No escaping needed — the length says where each frame ends.\n");
}

// ---------------------------------------------------------------------------
// 4. The command protocol.
// ---------------------------------------------------------------------------

/// Parses a representative set of inputs, including the ones that should fail.
/// The error cases matter more than the happy path — that is where student
/// implementations usually panic.
fn demo_protocol() {
    println!("=== 4. The chat command protocol ===\n");

    let inputs = [
        "hello everyone",
        "/nick alice",
        "/join rust",
        "/msg bob hello there    friend",
        "/who",
        "/rooms",
        "/quit",
        "",
        "/nick",
        "/nick alice smith",
        "/nick *admin",
        "/msg bob",
        "/dance",
    ];

    for input in inputs {
        match Command::parse(input) {
            Ok(cmd) => println!("  {input:<32} -> {cmd:?}"),
            Err(e) => println!("  {input:<32} -> error: {e}"),
        }
    }

    println!("\n  Server -> client wire format:");
    for event in [
        Event::notice("alice joined #rust"),
        Event::error("unknown command"),
        Event::Message { from: "alice".into(), text: "hi all".into() },
        Event::Private { from: "bob".into(), text: "psst".into() },
    ] {
        println!("    {event}");
    }

    println!("\n  Each prefix is unambiguous because nicknames may not start with");
    println!("  * ! [ or / — which is why `validate_nick` rejects them.\n");
}
