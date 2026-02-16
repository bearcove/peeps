//! peeps - Easy instrumentation and diagnostics
//!
//! This crate provides the main API for instrumenting your application:
//! - `peeps::init()` - Initialize instrumentation with a process name
//! - `peeps::collect_graph()` - Collect canonical graph snapshot data
//!
//! ## Sync primitives
//!
//! All sync primitives are diagnostic wrappers around standard libraries.
//! When the `diagnostics` feature is enabled, they track waiters, holders,
//! contention, and emit nodes into the canonical graph. When disabled,
//! they compile to zero-cost pass-throughs.
//!
//! - [`Mutex`], [`RwLock`] - Sync locks (wraps `parking_lot`)
//! - [`AsyncMutex`], [`AsyncRwLock`] - Async locks (wraps `tokio::sync`)
//! - [`Semaphore`] - Async semaphore
//! - [`OnceCell`] - Async once-cell
//! - [`channel`], [`unbounded_channel`], [`oneshot_channel`], [`watch_channel`] - Channels

#[cfg(feature = "diagnostics")]
use std::path::{Path, PathBuf};
#[cfg(feature = "diagnostics")]
use std::sync::LazyLock;

mod collect;
pub mod command;
pub mod fs;
pub(crate) mod futures;
mod joinset;
pub(crate) mod locks;
pub mod net;
pub mod registry;
pub mod rpc;
pub mod stack;
pub(crate) mod sync;

#[cfg(feature = "dashboard")]
mod dashboard_client;

// ── peeps_types re-exports ──────────────────────────────

pub use peeps_types::{self as types, GraphSnapshot, IntoMetaValue, MetaBuilder, MetaValue};
pub use peeps_types::{canonical_id, meta_key};

// ── collect ─────────────────────────────────────────────

pub use collect::collect_graph;

// ── stack ───────────────────────────────────────────────

pub use stack::ensure as ensure_stack;

// ── futures ─────────────────────────────────────────────

pub use futures::{
    peepable, peepable_with_meta, peepable_with_meta_kind, peepable_with_meta_kind_level, sleep,
    spawn_blocking_tracked, spawn_tracked, timeout, PeepableFuture,
};
pub use joinset::JoinSet;
pub use peeps_types::InstrumentationLevel;

// ── locks ───────────────────────────────────────────────

pub type Mutex<T> = locks::DiagnosticMutex<T>;
pub type RwLock<T> = locks::DiagnosticRwLock<T>;
pub type AsyncMutex<T> = locks::DiagnosticAsyncMutex<T>;
pub type AsyncRwLock<T> = locks::DiagnosticAsyncRwLock<T>;

#[cfg(feature = "diagnostics")]
pub use locks::{
    DiagnosticAsyncMutexGuard as AsyncMutexGuard,
    DiagnosticAsyncRwLockReadGuard as AsyncRwLockReadGuard,
    DiagnosticAsyncRwLockWriteGuard as AsyncRwLockWriteGuard, DiagnosticMutexGuard as MutexGuard,
    DiagnosticRwLockReadGuard as RwLockReadGuard, DiagnosticRwLockWriteGuard as RwLockWriteGuard,
};

// ── channels ────────────────────────────────────────────

pub use sync::{channel, oneshot_channel, unbounded_channel, watch_channel};
pub use sync::{OneshotReceiver, OneshotSender, Receiver, Sender};
pub use sync::{UnboundedReceiver, UnboundedSender, WatchReceiver, WatchSender};

// ── sync primitives ─────────────────────────────────────

pub type Semaphore = sync::DiagnosticSemaphore;
pub type Notify = sync::DiagnosticNotify;
pub use sync::OnceCell;

// ── timers ─────────────────────────────────────────────

pub use sync::DiagnosticInterval as Interval;
pub use sync::{interval, interval_at};

// ── command ────────────────────────────────────────────

pub use command::{Child, Command};

#[cfg(feature = "diagnostics")]
static START_CWD: LazyLock<PathBuf> =
    LazyLock::new(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

#[cfg(feature = "diagnostics")]
pub(crate) fn caller_location(caller: &std::panic::Location<'_>) -> String {
    let file = Path::new(caller.file());
    let path = if file.is_absolute() {
        file.to_path_buf()
    } else {
        START_CWD.join(file)
    };
    format!("{}:{}", path.display(), caller.line())
}

// ── Macros ──────────────────────────────────────────────

/// Build a `MetaBuilder` on the stack from key-value pairs.
///
/// ```ignore
/// peep_meta!("correlation" => MetaValue::U64(42), "method" => MetaValue::Static("get"))
/// ```
#[macro_export]
macro_rules! peep_meta {
    ($($k:literal => $v:expr),* $(,)?) => {{
        let mut mb = $crate::MetaBuilder::<16>::new();
        $(mb.push($k, $v);)*
        mb
    }};
}

/// Wrap a future with metadata, compiling away to bare future when diagnostics are disabled.
#[cfg(feature = "diagnostics")]
#[macro_export]
macro_rules! peepable_with_meta {
    ($future:expr, $label:literal, {$($k:literal => $v:expr),* $(,)?}) => {{
        $crate::peepable_with_meta(
            $future,
            $label,
            $crate::peep_meta!($($k => $v),*),
        )
    }};
    ($future:expr, $label:literal, kind = $kind:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        $crate::peepable_with_meta_kind(
            $future,
            $kind,
            $label,
            $crate::peep_meta!($($k => $v),*),
        )
    }};
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! peepable_with_meta {
    ($future:expr, $label:literal, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
    ($future:expr, $label:literal, kind = $kind:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
}

/// Wrap a future with auto-injected callsite context and optional custom metadata.
///
/// When `diagnostics` is disabled, expands to the bare future (zero cost).
///
/// ```ignore
/// // Label only (auto context injected):
/// peep!(stream.flush(), "socket.flush").await?;
///
/// // Label + custom keys:
/// peep!(stream.read(&mut buf), "socket.read", {
///     "resource.path" => path.as_str(),
///     "bytes" => buf.len(),
/// }).await?;
/// ```
#[cfg(feature = "diagnostics")]
#[macro_export]
macro_rules! peep {
    ($future:expr, $label:expr, level = $level:expr, kind = $kind:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::peepable_with_meta_kind_level($future, $kind, $label, $level, mb)
    }};
    ($future:expr, $label:expr, level = $level:expr, kind = $kind:expr) => {{
        let mb = $crate::MetaBuilder::new();
        $crate::peepable_with_meta_kind_level($future, $kind, $label, $level, mb)
    }};
    ($future:expr, $label:expr, kind = $kind:expr, level = $level:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::peepable_with_meta_kind_level($future, $kind, $label, $level, mb)
    }};
    ($future:expr, $label:expr, kind = $kind:expr, level = $level:expr) => {{
        let mb = $crate::MetaBuilder::new();
        $crate::peepable_with_meta_kind_level($future, $kind, $label, $level, mb)
    }};
    ($future:expr, $label:expr, kind = $kind:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::peepable_with_meta_kind($future, $kind, $label, mb)
    }};
    ($future:expr, $label:expr, level = $level:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::peepable_with_meta_kind_level($future, $crate::types::NodeKind::Future, $label, $level, mb)
    }};
    ($future:expr, $label:expr, level = $level:expr) => {{
        let mb = $crate::MetaBuilder::new();
        $crate::peepable_with_meta_kind_level($future, $crate::types::NodeKind::Future, $label, $level, mb)
    }};
    ($future:expr, $label:expr, kind = $kind:expr) => {{
        let mb = $crate::MetaBuilder::new();
        $crate::peepable_with_meta_kind($future, $kind, $label, mb)
    }};
    ($future:expr, $label:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::peepable_with_meta($future, $label, mb)
    }};
    ($future:expr, $label:expr) => {{
        let mb = $crate::MetaBuilder::new();
        $crate::peepable_with_meta($future, $label, mb)
    }};
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! peep {
    ($future:expr, $label:expr, level = $level:expr, kind = $kind:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
    ($future:expr, $label:expr, level = $level:expr, kind = $kind:expr) => {{
        $future
    }};
    ($future:expr, $label:expr, kind = $kind:expr, level = $level:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
    ($future:expr, $label:expr, kind = $kind:expr, level = $level:expr) => {{
        $future
    }};
    ($future:expr, $label:expr, kind = $kind:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
    ($future:expr, $label:expr, level = $level:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
    ($future:expr, $label:expr, level = $level:expr) => {{
        $future
    }};
    ($future:expr, $label:expr, kind = $kind:expr) => {{
        $future
    }};
    ($future:expr, $label:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
    ($future:expr, $label:expr) => {{
        $future
    }};
}

/// Record an RPC request entity with metadata key/value pairs.
///
/// Prefer this macro from wrapper crates when you want diagnostics-off builds
/// to compile to a true no-op (including metadata construction).
#[cfg(feature = "diagnostics")]
#[macro_export]
macro_rules! rpc_request_event {
    ($entity_id:expr, $name:expr, parent = $parent:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::<16>::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::rpc::record_request_with_meta($entity_id, $name, mb, Some($parent));
    }};
    ($entity_id:expr, $name:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::<16>::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::rpc::record_request_with_meta($entity_id, $name, mb, None);
    }};
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! rpc_request_event {
    ($entity_id:expr, $name:expr, parent = $parent:expr, {$($k:literal => $v:expr),* $(,)?}) => {{}};
    ($entity_id:expr, $name:expr, {$($k:literal => $v:expr),* $(,)?}) => {{}};
}

/// Record an RPC response entity with metadata key/value pairs.
///
/// Prefer this macro from wrapper crates when you want diagnostics-off builds
/// to compile to a true no-op (including metadata construction).
#[cfg(feature = "diagnostics")]
#[macro_export]
macro_rules! rpc_response_event {
    ($entity_id:expr, $name:expr, parent = $parent:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::<16>::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::rpc::record_response_with_meta($entity_id, $name, mb, Some($parent));
    }};
    ($entity_id:expr, $name:expr, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::<16>::new();
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::rpc::record_response_with_meta($entity_id, $name, mb, None);
    }};
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! rpc_response_event {
    ($entity_id:expr, $name:expr, parent = $parent:expr, {$($k:literal => $v:expr),* $(,)?}) => {{}};
    ($entity_id:expr, $name:expr, {$($k:literal => $v:expr),* $(,)?}) => {{}};
}

// ── Initialization ──────────────────────────────────────

/// Initialize peeps instrumentation with a process name.
///
/// Sets up the central registry with process metadata. If `PEEPS_DASHBOARD`
/// is set (e.g. `127.0.0.1:9119`), starts a background pull loop.
pub fn init(process_name: impl Into<String>) {
    let name = process_name.into();
    let pid = std::process::id();
    let proc_key = peeps_types::make_proc_key(&name, pid);
    registry::init(&name, &proc_key);
    peeps_types::set_process_name(&name);
    #[cfg(feature = "dashboard")]
    {
        if let Ok(addr) = std::env::var("PEEPS_DASHBOARD") {
            dashboard_client::start_pull_loop(name, addr);
            return;
        }
    }
    let _ = name;
}
