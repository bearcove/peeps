//! Diagnostic wrappers for tokio channels, semaphores, and `OnceCell`.
//!
//! When the `diagnostics` feature is enabled, wraps tokio sync primitives
//! to track message counts, channel state, semaphore contention, and OnceCell
//! initialization timing. When disabled, all wrappers are zero-cost.

#[cfg(feature = "diagnostics")]
pub(crate) mod channels;
#[cfg(feature = "diagnostics")]
pub(crate) mod oncecell;
#[cfg(feature = "diagnostics")]
pub(crate) mod semaphore;

#[cfg(feature = "diagnostics")]
mod enabled;
#[cfg(not(feature = "diagnostics"))]
mod disabled;

// Public types are re-exported so the crate root can `pub use` them.
// The module itself is `pub(crate)`, so `peeps::sync::Sender` is not
// accessible externally — only `peeps::Sender` (via lib.rs re-export).
#[cfg(feature = "diagnostics")]
pub use enabled::{
    channel, oneshot_channel, unbounded_channel, watch_channel, DiagnosticSemaphore, OnceCell,
    OneshotReceiver, OneshotSender, Receiver, Sender, UnboundedReceiver, UnboundedSender,
    WatchReceiver, WatchSender,
};
#[cfg(not(feature = "diagnostics"))]
pub use disabled::{
    channel, oneshot_channel, unbounded_channel, watch_channel, DiagnosticSemaphore, OnceCell,
    OneshotReceiver, OneshotSender, Receiver, Sender, UnboundedReceiver, UnboundedSender,
    WatchReceiver, WatchSender,
};

// emit_into_graph is crate-internal only
#[cfg(feature = "diagnostics")]
pub(crate) use enabled::emit_into_graph;
#[cfg(not(feature = "diagnostics"))]
pub(crate) use disabled::emit_into_graph;
