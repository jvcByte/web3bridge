//! Week 5 · Day 2 — Async TCP & Buffers
//!
//! Day 1's echo server, converted to tokio. The structure is deliberately kept
//! identical so the diff is the lesson: add `.await`, swap `thread::spawn` for
//! `tokio::spawn`, import the `Async*Ext` traits.
//!
//!   cargo run                 # the async echo server
//!   cargo run -- flood 1000   # 1000 concurrent connections, to prove the point
//!   cargo run -- raw          # read() into a raw buffer, no line abstraction
//!
//! While a flood is running, count the server's OS threads:
//!   ps -o nlwp= -p $(pgrep -f 'day2-async-echo$')

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// The extension traits are what provide `read`, `write_all`, `next_line`, etc.
// Forgetting these imports produces "no method named `write_all`" on a type that
// plainly has it — the most common day-two compile error.
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::{TcpListener, TcpStream};

const ADDR: &str = "127.0.0.1:7878";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("serve");

    let result = match mode {
        "serve" => serve().await,
        "raw" => serve_raw().await,
        "flood" => {
            let count: usize = args
                .get(2)
                .and_then(|n| n.parse().ok())
                .unwrap_or(1000);
            flood(count).await
        }
        other => {
            eprintln!("unknown mode {other:?}; try: serve | raw | flood [n]");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// The async echo server.
// ---------------------------------------------------------------------------

async fn serve() -> std::io::Result<()> {
    let listener = TcpListener::bind(ADDR).await?;

    // Shared across every connection task. `AtomicU64` rather than `Mutex<u64>`
    // because increment-and-read is exactly what atomics are for; no lock needed,
    // and no chance of holding a guard across an `.await`.
    let live = Arc::new(AtomicU64::new(0));
    let total = Arc::new(AtomicU64::new(0));

    println!("async echo server on {ADDR}");
    println!("  nc localhost 7878");
    println!("  cargo run -- flood 1000     (from another terminal)");
    println!();
    println!("Note what does NOT happen: a second client no longer waits.");
    println!();

    loop {
        // `accept()` yields to the runtime while waiting, instead of parking an OS
        // thread. That single difference is the whole reason this scales.
        let (stream, peer) = listener.accept().await?;

        let live = Arc::clone(&live);
        let total = Arc::clone(&total);

        // The Day 1 → Day 2 swap: `thread::spawn` becomes `tokio::spawn`. Same
        // Send + 'static bound, for the same reason — the work may move between
        // worker threads and may outlive this loop iteration.
        //
        // A task costs a few hundred bytes. An OS thread reserves 8 MiB of stack.
        tokio::spawn(async move {
            let n = live.fetch_add(1, Ordering::Relaxed) + 1;
            let seq = total.fetch_add(1, Ordering::Relaxed) + 1;

            // Only log occasionally during a flood, or the logging becomes the
            // bottleneck and you end up benchmarking println!.
            if n <= 5 || n % 250 == 0 {
                println!("[+] {peer} connected   (live: {n}, total: {seq})");
            }

            if let Err(e) = handle_client(stream).await {
                // A client vanishing mid-write is normal, not exceptional. Log at
                // low volume and carry on — one bad connection must never take the
                // server down. Day 4 goes further on this.
                if n <= 5 {
                    eprintln!("[!] {peer}: {e}");
                }
            }

            let n = live.fetch_sub(1, Ordering::Relaxed) - 1;
            if n < 5 || n % 250 == 0 {
                println!("[-] {peer} disconnected (live: {n})");
            }
        });
    }
}

/// Identical in shape to Day 1's `handle_client`, with `.await` added.
async fn handle_client(stream: TcpStream) -> std::io::Result<()> {
    // `into_split` gives an owned read half and write half, so each could move to
    // a different task if we needed that. Day 4 needs exactly this: one task
    // reading from the socket, another writing broadcasts to it.
    //
    // Day 1 used `try_clone()` to duplicate the file descriptor. Same idea, but
    // this version is checked by the type system — you cannot accidentally read
    // from the write half.
    let (read_half, write_half) = stream.into_split();

    let mut lines = BufReader::new(read_half).lines();
    let mut writer = BufWriter::new(write_half);

    writer
        .write_all(b"welcome. type something; /quit to leave.\n")
        .await?;
    writer.flush().await?;

    // Uncomment to see what blocking inside async does to the whole runtime.
    // Run a flood afterwards and watch throughput collapse: this stalls an entire
    // worker thread, freezing every other task scheduled on it.
    //
    // std::thread::sleep(Duration::from_millis(500));
    //
    // The async equivalent, which yields instead of blocking:
    // tokio::time::sleep(Duration::from_millis(500)).await;

    // `next_line()` returns Ok(None) at EOF — the async spelling of Day 1's
    // `Ok(0)`. Async changed how you wait; it did not change TCP.
    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();

        if trimmed == "/quit" {
            writer.write_all(b"bye\n").await?;
            writer.flush().await?;
            break;
        }

        writer.write_all(trimmed.to_uppercase().as_bytes()).await?;
        writer.write_all(b"\n").await?;

        // Still required. BufWriter buffers exactly as it did yesterday; async
        // does not flush for you.
        writer.flush().await?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Raw buffer mode — what `lines()` is hiding.
// ---------------------------------------------------------------------------

/// Echoes bytes with no line abstraction at all: `read` into a fixed buffer,
/// write back whatever arrived.
///
/// Connect with `nc localhost 7878` and paste a long paragraph. Watch the server
/// log show it arriving in several chunks of arbitrary size. Then send two short
/// lines quickly and watch them arrive together.
///
/// This is the interface TCP actually offers. `BufReader::lines()` is a
/// convenience built on top of it, and tomorrow you build that convenience
/// yourself with a real codec.
async fn serve_raw() -> std::io::Result<()> {
    let listener = TcpListener::bind(ADDR).await?;

    println!("RAW mode on {ADDR} — no line framing, just read() and write()");
    println!("  nc localhost 7878, then paste a long paragraph");
    println!("  watch it arrive in arbitrarily-sized chunks");
    println!();

    loop {
        let (mut stream, peer) = listener.accept().await?;

        tokio::spawn(async move {
            // 64 bytes, deliberately small, so chunking is obvious on a local
            // connection. A real server would use 4-8 KiB.
            let mut buf = [0u8; 64];
            let mut reads = 0;

            loop {
                match stream.read(&mut buf).await {
                    // Ok(0) is EOF: the peer closed. Not "nothing available".
                    // Treating it as the latter is the classic 100%-CPU spin.
                    Ok(0) => {
                        println!("[-] {peer} EOF after {reads} reads");
                        return;
                    }
                    Ok(n) => {
                        reads += 1;
                        println!(
                            "    read #{reads:<3} {n:>3} bytes: {:?}",
                            String::from_utf8_lossy(&buf[..n])
                        );
                        if stream.write_all(&buf[..n]).await.is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        eprintln!("[!] {peer}: {e}");
                        return;
                    }
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Flood — the load generator that makes the scaling argument concrete.
// ---------------------------------------------------------------------------

/// Opens `count` connections concurrently, sends one line on each, and verifies
/// the echo. Reports how long the whole thing took.
///
/// Run this against the server and check its thread count while it runs. A
/// thousand connections held by a handful of threads is the entire case for async.
async fn flood(count: usize) -> std::io::Result<()> {
    println!("opening {count} concurrent connections to {ADDR}...");
    let started = Instant::now();

    let mut handles = Vec::with_capacity(count);

    for i in 0..count {
        handles.push(tokio::spawn(async move {
            let stream = TcpStream::connect(ADDR).await?;
            let (read_half, mut write_half) = stream.into_split();
            let mut lines = BufReader::new(read_half).lines();

            // Greeting.
            lines.next_line().await?;

            let msg = format!("client {i}\n");
            write_half.write_all(msg.as_bytes()).await?;
            write_half.flush().await?;

            let echoed = lines.next_line().await?.unwrap_or_default();

            // Hold the connection open briefly so they overlap and the server
            // genuinely has `count` live sockets at once.
            tokio::time::sleep(Duration::from_millis(500)).await;

            write_half.write_all(b"/quit\n").await?;
            write_half.flush().await?;

            Ok::<String, std::io::Error>(echoed)
        }));
    }

    let mut ok = 0;
    let mut failed = 0;

    for handle in handles {
        match handle.await {
            Ok(Ok(echoed)) if echoed.starts_with("CLIENT ") => ok += 1,
            Ok(Ok(other)) => {
                failed += 1;
                if failed <= 3 {
                    eprintln!("  unexpected echo: {other:?}");
                }
            }
            Ok(Err(e)) => {
                failed += 1;
                if failed <= 3 {
                    eprintln!("  io error: {e}");
                }
            }
            Err(e) => {
                failed += 1;
                if failed <= 3 {
                    eprintln!("  task panicked: {e}");
                }
            }
        }
    }

    println!();
    println!("{ok} succeeded, {failed} failed, in {:.2?}", started.elapsed());
    println!();
    println!("now count the server's OS threads:");
    println!("  ps -o nlwp= -p $(pgrep -f 'day2-async-echo$')");
    println!();
    println!("Day 1's threaded server would have needed {count} threads for this.");

    if failed > 0 {
        // A flood that hits the open-file-descriptor limit is a useful lesson in
        // itself, so point at it rather than just reporting a number.
        println!();
        println!("some failed — check `ulimit -n` (open fd limit). Each connection");
        println!("costs one fd on each side, and the default is often 1024.");
    }

    Ok(())
}
