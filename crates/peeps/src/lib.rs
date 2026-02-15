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

use std::future::IntoFuture;

mod collect;
pub(crate) mod futures;
pub(crate) mod locks;
pub mod registry;
pub mod stack;
pub(crate) mod sync;

#[cfg(feature = "dashboard")]
mod dashboard_client;

// ── peeps_types re-exports ──────────────────────────────

pub use peeps_types::{self as types, GraphSnapshot, IntoMetaValue, MetaBuilder, MetaValue};
pub use peeps_types::{canonical_id, meta_key};

// ── collect ─────────────────────────────────────────────

pub use collect::collect_graph;

// ── futures ─────────────────────────────────────────────

pub use futures::{peepable, peepable_with_meta, spawn_tracked, PeepableFuture};

// ── locks ───────────────────────────────────────────────

pub type Mutex<T> = locks::DiagnosticMutex<T>;
pub type RwLock<T> = locks::DiagnosticRwLock<T>;
pub type AsyncMutex<T> = locks::DiagnosticAsyncMutex<T>;
pub type AsyncRwLock<T> = locks::DiagnosticAsyncRwLock<T>;

#[cfg(feature = "diagnostics")]
pub use locks::{
    DiagnosticAsyncMutexGuard as AsyncMutexGuard,
    DiagnosticAsyncRwLockReadGuard as AsyncRwLockReadGuard,
    DiagnosticAsyncRwLockWriteGuard as AsyncRwLockWriteGuard,
    DiagnosticMutexGuard as MutexGuard,
    DiagnosticRwLockReadGuard as RwLockReadGuard,
    DiagnosticRwLockWriteGuard as RwLockWriteGuard,
};

// ── channels ────────────────────────────────────────────

pub use sync::{channel, oneshot_channel, unbounded_channel, watch_channel};
pub use sync::{OneshotReceiver, OneshotSender, Receiver, Sender};
pub use sync::{UnboundedReceiver, UnboundedSender, WatchReceiver, WatchSender};

// ── sync primitives ─────────────────────────────────────

pub type Semaphore = sync::DiagnosticSemaphore;
pub use sync::OnceCell;

// ── PeepableFutureExt ───────────────────────────────────

pub trait PeepableFutureExt: IntoFuture + Sized {
    fn peepable(self, resource: impl Into<String>) -> PeepableFuture<Self::IntoFuture> {
        crate::futures::peepable(self, resource)
    }
    fn peepable_with_meta<const N: usize>(
        self,
        resource: impl Into<String>,
        meta: MetaBuilder<'_, N>,
    ) -> PeepableFuture<Self::IntoFuture> {
        crate::futures::peepable_with_meta(self, resource, meta)
    }
}

impl<F: IntoFuture> PeepableFutureExt for F {}

// ── Macros ──────────────────────────────────────────────

/// Build a `MetaBuilder` on the stack from key-value pairs.
///
/// ```ignore
/// peep_meta!("request.id" => MetaValue::U64(42), "request.method" => MetaValue::Static("get"))
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
        $crate::PeepableFutureExt::peepable_with_meta(
            $future,
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
    ($future:expr, $label:literal, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::<{
            6 $(+ $crate::peep!(@count $k))*
        }>::new();
        mb.push($crate::meta_key::CTX_MODULE_PATH, $crate::MetaValue::Static(module_path!()));
        mb.push($crate::meta_key::CTX_FILE, $crate::MetaValue::Static(file!()));
        mb.push($crate::meta_key::CTX_LINE, $crate::MetaValue::U64(line!() as u64));
        mb.push($crate::meta_key::CTX_CRATE_NAME, $crate::MetaValue::Static(env!("CARGO_PKG_NAME")));
        mb.push($crate::meta_key::CTX_CRATE_VERSION, $crate::MetaValue::Static(env!("CARGO_PKG_VERSION")));
        mb.push($crate::meta_key::CTX_CALLSITE, $crate::MetaValue::Static(concat!($label, "@", file!(), ":", line!(), "::", module_path!())));
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::peepable_with_meta($future, $label, mb)
    }};
    ($future:expr, $label:literal) => {{
        let mut mb = $crate::MetaBuilder::<6>::new();
        mb.push($crate::meta_key::CTX_MODULE_PATH, $crate::MetaValue::Static(module_path!()));
        mb.push($crate::meta_key::CTX_FILE, $crate::MetaValue::Static(file!()));
        mb.push($crate::meta_key::CTX_LINE, $crate::MetaValue::U64(line!() as u64));
        mb.push($crate::meta_key::CTX_CRATE_NAME, $crate::MetaValue::Static(env!("CARGO_PKG_NAME")));
        mb.push($crate::meta_key::CTX_CRATE_VERSION, $crate::MetaValue::Static(env!("CARGO_PKG_VERSION")));
        mb.push($crate::meta_key::CTX_CALLSITE, $crate::MetaValue::Static(concat!($label, "@", file!(), ":", line!(), "::", module_path!())));
        $crate::peepable_with_meta($future, $label, mb)
    }};
    (@count $x:literal) => { 1usize };
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! peep {
    ($future:expr, $label:literal, {$($k:literal => $v:expr),* $(,)?}) => {{ $future }};
    ($future:expr, $label:literal) => {{ $future }};
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
