// r[impl process.dependency]
//! Runtime graph instrumentation for Tokio-based Rust systems.
//!
//! Moiré replaces Tokio's primitives with named, instrumented wrappers. At every
//! API boundary — every lock acquisition, channel send/receive, spawn, and RPC
//! call — it captures the current call stack via frame-pointer walking and interns
//! it as a [`BacktraceId`]. The resulting graph of entities (tasks, locks,
//! channels, RPC calls) connected by typed edges (polls, waiting\_on, paired\_with,
//! holds) is pushed as a live stream to a `moire-web` dashboard for investigation.
//!
//! The dashboard shows you which tasks are stuck, what they are waiting on, and
//! exactly where in your code they got there.
//!
//! # Using this crate
//!
//! Add `moire` as a dependency and replace Tokio/parking\_lot primitives with
//! their `moire::` equivalents:
//!
//! ```toml
//! # Cargo.toml
//! moire = { ..., features = ["diagnostics"] }
//! ```
//!
//! ```rust,no_run
//! #[tokio::main]
//! async fn main() {
//!     // No init call needed — moire initializes itself via `ctor`.
//!
//!     moire::spawn("connection_handler", async {
//!         let mu = moire::Mutex::new("state", MyState::default());
//!         let (tx, rx) = moire::channel("work_queue", 64);
//!         // ...
//!     });
//! }
//! ```
//!
//! Run `moire-web` and point your process at it:
//!
//! ```text
//! MOIRE_DASHBOARD=127.0.0.1:9119 ./your-binary
//! ```
//!
//! # Cargo features
//!
//! | Feature | Effect |
//! |---------|--------|
//! | *(default, none)* | All wrappers compile to pass-throughs; no instrumentation overhead. |
//! | `diagnostics` | Enables backtrace capture, entity tracking, and live dashboard push. |
//!
//! Without `diagnostics`, setting `MOIRE_DASHBOARD` emits a warning and does not connect.
//!
//! # What is instrumented
//!
//! - **Tasks**: [`spawn`], [`spawn_blocking`], [`JoinSet`]
//! - **Channels**: [`channel`] (mpsc), [`unbounded_channel`], [`broadcast`], [`oneshot`], [`watch`]
//! - **Synchronization**: [`Mutex`], [`RwLock`], [`Semaphore`], [`Notify`], [`OnceCell`]
//! - **Processes**: [`Command`]
//! - **RPC**: [`rpc_request`], [`rpc_response_for`] (used by Roam)
//!
//! # Platform backends
//!
//! This crate re-exports the right backend for the current target:
//!
//! - **native** (`not(target_arch = "wasm32")`) → `moire-tokio`
//! - **wasm32** → `moire-wasm` (all instrumentation is a no-op; API surface is identical)

// r[impl api.backend.native]
#[cfg(not(target_arch = "wasm32"))]
pub use moire_tokio::*;
// r[impl api.backend.wasm]
#[cfg(target_arch = "wasm32")]
pub use moire_wasm::*;
