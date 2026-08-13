//! Week 4 · Day 1 — Async Rust Foundations
//!
//! Five demos, run in order. Each one prints its own elapsed time, because the
//! entire lesson of today is visible in the difference between ~6s and ~3s.
//!
//! Run with: `cargo run`

use std::time::{Duration, Instant};

use tokio::time::sleep;

// ---------------------------------------------------------------------------
// The three "downloads". Real network I/O is just waiting, so `sleep` is an
// honest simulation of it — the CPU is idle either way.
// ---------------------------------------------------------------------------

async fn download_file(name: &str) -> String {
    println!("  -> start  file   {name}");
    sleep(Duration::from_secs(3)).await;
    println!("  <- done   file   {name}");
    format!("{name} (2.4 MB)")
}

async fn download_image(name: &str) -> String {
    println!("  -> start  image  {name}");
    sleep(Duration::from_secs(2)).await;
    println!("  <- done   image  {name}");
    format!("{name} (860 KB)")
}

async fn download_json(endpoint: &str) -> String {
    println!("  -> start  json   {endpoint}");
    sleep(Duration::from_secs(1)).await;
    println!("  <- done   json   {endpoint}");
    format!("{endpoint} (12 KB)")
}

// ---------------------------------------------------------------------------
// 1. Futures are lazy.
// ---------------------------------------------------------------------------

/// Calling an `async fn` runs *none* of its body. It builds a value implementing
/// `Future` and hands it to you, inert. Nothing is scheduled until something
/// polls it — which is what `.await` and `spawn` do.
///
/// This is the #1 day-one bug for anyone arriving from JavaScript, where calling
/// an `async` function starts the work immediately.runtime
async fn demo_lazy_futures() {
    println!("\n=== 1. Futures are lazy ===");

    let future = download_json("/api/never-runs");
    println!("  built the future — notice nothing printed above this line");

    sleep(Duration::from_millis(500)).await;
    println!("  half a second later, still nothing has run");

    // Only now does the body actually execute.
    let result = future.await;
    println!("  after .await: {result}");
}

// ---------------------------------------------------------------------------
// 2. Sequential — the naive version.
// ---------------------------------------------------------------------------

/// Each `.await` fully completes before the next line starts. Total time is the
/// *sum*: 3 + 2 + 1 = 6 seconds. Note that the CPU is idle for essentially all
/// of it. That idleness is the waste async exists to reclaim.
async fn demo_sequential() {
    println!("\n=== 2. Sequential (.await one at a time) ===");
    let started = Instant::now();

    let file = download_file("report.pdf").await;
    let image = download_image("hero.png").await;
    let json = download_json("/api/posts").await;

    println!("  got: {file}, {image}, {json}");
    println!(
        "  elapsed: {:.2?}  <- the SUM of all three",
        started.elapsed()
    );
}

// ---------------------------------------------------------------------------
// 3. Concurrent with join! — one task, interleaved.
// ---------------------------------------------------------------------------

/// `tokio::join!` polls all three futures on the *current* task, switching
/// between them at every `.await` point. Total time is the *max*: 3 seconds.
///
/// Because everything stays on one task, the futures may borrow from the
/// surrounding scope — no `Send + 'static` bound is required. That is `join!`'s
/// real advantage over `spawn`, and it is not about speed.
async fn demo_join() {
    println!("\n=== 3. Concurrent with join! ===");
    let started = Instant::now();

    // A local String, borrowed by one of the futures below. This compiles under
    // join! and would NOT compile under spawn — see demo 4.
    let endpoint = String::from("/api/posts");

    let (file, image, json) = tokio::join!(
        download_file("report.pdf"),
        download_image("hero.png"),
        download_json(&endpoint),
    );

    println!("  got: {file}, {image}, {json}");
    println!(
        "  elapsed: {:.2?}  <- the MAX of all three",
        started.elapsed()
    );
}

// ---------------------------------------------------------------------------
// 4. Concurrent with spawn — three independent tasks.
// ---------------------------------------------------------------------------

/// `tokio::spawn` hands each future to the runtime as its own task. Under the
/// multi-threaded scheduler those tasks can run on different worker threads, so
/// this is genuine parallelism, not just concurrency.
///
/// The price: the runtime may move a task to another thread at any `.await`, and
/// it may outlive this function, so every spawned future must be `Send + 'static`.
/// That is why the arguments below are string literals (`&'static str`) rather
/// than references to a local `String`.
async fn demo_spawn() {
    println!("\n=== 4. Concurrent with spawn ===");
    let started = Instant::now();

    // Try changing this to `let endpoint = String::from("/api/posts");` and
    // passing `&endpoint` — the compiler will reject it. Read that error in class.
    let file_task = tokio::spawn(download_file("report.pdf"));
    let image_task = tokio::spawn(download_image("hero.png"));
    let json_task = tokio::spawn(download_json("/api/posts"));

    // spawn returns a JoinHandle, which is itself a future. Awaiting it yields
    // Result<T, JoinError> — Err if that task panicked or was cancelled.
    let file = file_task.await.expect("file task panicked");
    let image = image_task.await.expect("image task panicked");
    let json = json_task.await.expect("json task panicked");

    println!("  got: {file}, {image}, {json}");
    println!(
        "  elapsed: {:.2?}  <- same as join!, different mechanism",
        started.elapsed()
    );
}

// ---------------------------------------------------------------------------
// 5. Detached tasks die with the runtime.
// ---------------------------------------------------------------------------

/// A spawned task starts running immediately, but nobody is waiting for it. When
/// `main` returns, the runtime shuts down and drops every task still in flight —
/// mid-`.await`, without unwinding.
///
/// Here the 1s download finishes and the 3s one never does. This bites students
/// constantly in Week 5 when a connection handler silently vanishes.
async fn demo_spawn_detached() {
    println!("\n=== 5. Detached tasks are cancelled at shutdown ===");

    tokio::spawn(download_json("/api/fast"));
    tokio::spawn(download_file("slow-and-doomed.pdf"));

    println!("  both spawned; sleeping only 1.5s before moving on");
    sleep(Duration::from_millis(1500)).await;
    println!("  the 1s json finished. the 3s file never will — no 'done' line for it.");
}

// ---------------------------------------------------------------------------

/// `#[tokio::main]` rewrites this into a synchronous `fn main` that builds a
/// multi-threaded runtime and calls `block_on` with the body. Run
/// `cargo expand` to see it — worth doing live.
#[tokio::main]
async fn main() {
    println!("Week 4 · Day 1 — Async Rust Foundations");
    println!(
        "Worker threads available: {}",
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    );

    demo_lazy_futures().await;
    demo_sequential().await;
    demo_join().await;
    demo_spawn().await;
    demo_spawn_detached().await;

    println!("\nTakeaway: sequential = sum of waits. join!/spawn = max of waits.");
    println!("join! is one task borrowing freely; spawn is N tasks needing Send + 'static.");
}
