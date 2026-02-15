//! Diagnostic wrappers for tokio channels, semaphores, and `OnceCell`.
//!
//! When the `diagnostics` feature is enabled, wraps tokio sync primitives
//! to track message counts, channel state, semaphore contention, and OnceCell
//! initialization timing. When disabled, all wrappers are zero-cost.

pub mod channels;
pub mod oncecell;
#[cfg(feature = "diagnostics")]
pub(crate) mod registry;
pub mod semaphore;
mod snapshot;

pub use peeps_types::{
    MpscChannelSnapshot, OnceCellSnapshot, OnceCellState, OneshotChannelSnapshot, OneshotState,
    SemaphoreSnapshot, SyncSnapshot, WatchChannelSnapshot,
};

// ── Public API ──────────────────────────────────────────

pub use channels::{
    channel, oneshot_channel, unbounded_channel, watch_channel, OneshotReceiver, OneshotSender,
    Receiver, Sender, UnboundedReceiver, UnboundedSender, WatchReceiver, WatchSender,
};
pub use oncecell::OnceCell;
pub use semaphore::DiagnosticSemaphore;
pub use snapshot::snapshot_all;
