//! Week 5 · Day 1 — TCP Fundamentals
//!
//! A blocking echo server built on `std::net` alone. No tokio, no dependencies.
//!
//! The default mode serves exactly one client at a time. That is the lesson, not
//! a bug: connect a second client while the first is open and watch it hang in
//! the kernel's listen backlog.
//!
//!   cargo run              # blocking, single client
//!   cargo run -- threaded  # one OS thread per client
//!   cargo run -- client    # minimal test client
//!
//! Or just use netcat: `nc localhost 7878`

use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

const ADDR: &str = "127.0.0.1:7878";

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "serve".to_string());

    let result = match mode.as_str() {
        "serve" => serve_blocking(),
        "threaded" => serve_threaded(),
        "client" => run_client(),
        other => {
            eprintln!("unknown mode {other:?}; try: serve | threaded | client");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("fatal: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// 1. The blocking server — one client at a time.
// ---------------------------------------------------------------------------

/// Accept one connection, serve it to completion, then accept the next.
///
/// `listener.incoming()` yields connections one at a time, and because
/// `handle_client` runs to completion before the loop comes back around, a second
/// client cannot be served until the first disconnects.
///
/// The second client is not *rejected*. The kernel completes its TCP handshake and
/// parks it in the listen backlog (default 128 on Linux). From the client's side
/// the connection looks established — it just never gets a reply. Silent hangs
/// like this are far more confusing than an outright connection refused, which is
/// worth naming in class.
fn serve_blocking() -> std::io::Result<()> {
    let listener = TcpListener::bind(ADDR)?;

    println!("blocking echo server on {ADDR}");
    println!("  nc localhost 7878");
    println!();
    println!("EXPERIMENT: connect a SECOND client while the first is open.");
    println!("It will hang — accepted by the kernel, never accept()ed by us.");
    println!("Close the first and watch the second come alive instantly.");
    println!();

    for stream in listener.incoming() {
        let stream = stream?;
        let peer = stream.peer_addr()?;

        println!("[+] {peer} connected  (the server is now deaf to everyone else)");

        // Blocks here until this client disconnects.
        if let Err(e) = handle_client(stream) {
            eprintln!("[!] {peer} error: {e}");
        }

        println!("[-] {peer} disconnected (now accepting again)");
    }

    Ok(())
}

/// Read lines, echo them back uppercased.
///
/// `BufReader::read_line` is doing real work here: it buffers whatever the socket
/// gives it and hands back exactly one line, hiding the fact that TCP has no
/// message boundaries. That convenience is why the framing problem stays invisible
/// until Day 3 — point at this function when it surfaces.
fn handle_client(stream: TcpStream) -> std::io::Result<()> {
    let peer = stream.peer_addr()?;

    // Two independent handles to the same socket: one for reading, one for
    // writing. `try_clone` duplicates the file descriptor — both refer to the same
    // underlying connection.
    let reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);

    writeln!(writer, "welcome. type something; /quit to leave.")?;
    writer.flush()?;

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        println!("    {peer} says: {trimmed:?}");

        if trimmed == "/quit" {
            writeln!(writer, "bye")?;
            writer.flush()?;
            break;
        }

        writeln!(writer, "{}", trimmed.to_uppercase())?;

        // Without this, the reply sits in the BufWriter's 8 KiB buffer until it
        // fills or the writer drops — so an interactive client sees nothing and
        // concludes the server is broken. Buffering is a throughput optimisation
        // that trades away latency, and interactive protocols have to flush.
        writer.flush()?;
    }

    // Falling out of `lines()` means read returned Ok(0): EOF, the peer closed
    // its end. A zero-length read is *the* disconnect signal in TCP. Treating it
    // as "nothing available right now" and looping is the classic bug that pins a
    // core at 100%.
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. One thread per client — the traditional fix.
// ---------------------------------------------------------------------------

/// Same server, but each connection gets its own OS thread, so the accept loop
/// comes back around immediately.
///
/// This genuinely works, and for a few hundred mostly-active connections it is a
/// perfectly good design: simple, debuggable, and every stack trace is a real
/// stack trace.
///
/// The cost is per-connection. Each thread reserves 8 MiB of virtual address
/// space for its stack by default on Linux (committed lazily, but the scheduling
/// entity is real), and every context switch is a kernel transition. A chat server
/// is the worst case for this model: thousands of connections that are idle almost
/// all of the time, each holding a whole thread to wait on a socket.
///
/// Tomorrow, `tokio::spawn` replaces `thread::spawn` and each connection costs a
/// few hundred bytes instead.
fn serve_threaded() -> std::io::Result<()> {
    let listener = TcpListener::bind(ADDR)?;

    println!("threaded echo server on {ADDR}");
    println!("connect as many clients as you like — they all work now.");
    println!();
    println!("Ask yourself: what does the 10,000th client cost?");
    println!();

    for stream in listener.incoming() {
        let stream = stream?;
        let peer = stream.peer_addr()?;

        println!("[+] {peer} connected");

        // The handle is dropped, detaching the thread. For a demo that is fine;
        // it does mean shutdown cannot wait for clients to finish. Making this
        // clean is today's homework.
        thread::spawn(move || {
            if let Err(e) = handle_client(stream) {
                eprintln!("[!] {peer} error: {e}");
            }
            println!("[-] {peer} disconnected");
        });
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// 3. A minimal client — mostly so the framing demo below has something to drive.
// ---------------------------------------------------------------------------

fn run_client() -> std::io::Result<()> {
    let mut stream = TcpStream::connect(ADDR)?;
    println!("connected to {ADDR}");

    // Read the greeting.
    let mut buf = [0u8; 512];
    let n = stream.read(&mut buf)?;
    print!("server: {}", String::from_utf8_lossy(&buf[..n]));

    // The framing demonstration, worth running live.
    //
    // These two writes send "hel" and then "lo\n" as separate syscalls with a
    // pause between them. The server's `read_line` blocks until it sees the
    // newline and then reports one clean line — it had to reassemble it from two
    // reads. Now imagine that reassembly is your job, because on Day 3 it will be.
    println!("\nsending 'hel' ... then 'lo\\n' 200ms later");
    stream.write_all(b"hel")?;
    stream.flush()?;
    thread::sleep(Duration::from_millis(200));
    stream.write_all(b"lo\n")?;
    stream.flush()?;

    let n = stream.read(&mut buf)?;
    print!("server: {}", String::from_utf8_lossy(&buf[..n]));
    println!("^ one line, reassembled from two writes. TCP has no message boundaries.");

    // And the reverse: two messages that may arrive in a single read.
    println!("\nsending 'one\\ntwo\\n' in a single write");
    stream.write_all(b"one\ntwo\n")?;
    stream.flush()?;

    thread::sleep(Duration::from_millis(200));
    let n = stream.read(&mut buf)?;
    print!("server: {}", String::from_utf8_lossy(&buf[..n]));
    println!("^ likely both replies in one read. Same stream, no boundaries either way.");

    stream.write_all(b"/quit\n")?;
    stream.flush()?;

    Ok(())
}
