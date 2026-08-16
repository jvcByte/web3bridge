//! Week 4 · Day 5 — Blog API.
//!
//! The Week 4 deliverable, in the module layout the curriculum specifies:
//!
//! ```text
//! src/
//! ├── errors/       one ApiError enum + its IntoResponse impl
//! ├── models/       Post, CreatePost, UpdatePost + validation
//! ├── state/        AppState, Store, the single mutex
//! ├── middleware/   request logging, X-API-KEY auth
//! ├── handlers/     the actual request handlers
//! ├── routes/       the routing table, and nothing else
//! ├── lib.rs        this file — wires the modules together
//! └── main.rs       binds a port and serves
//! ```
//!
//! **Why both `lib.rs` and `main.rs`?** A binary crate cannot be imported by
//! anything, including its own `tests/` directory. Putting the app in a library
//! and leaving `main.rs` as a thin shell over it means the integration tests can
//! `use day5_blog_api::...` and build the real router. This is the standard Rust
//! layout for a testable binary, and worth calling out — students frequently
//! write everything in `main.rs` and then discover they cannot test it.

pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod state;

pub use routes::app;
pub use state::AppState;
