//! peeps - Easy instrumentation and diagnostics
//!
//! This crate provides the main API for instrumenting your application:
//! - `peeps::init()` - Initialize all instrumentation
//! - `peeps::collect_graph()` - Collect canonical graph snapshot data

mod collect;

#[cfg(feature = "dashboard")]
mod dashboard_client;

pub use peeps_sync as sync;
pub use peeps_futures as futures;
pub use peeps_types::{self as types, Diagnostics};

#[cfg(feature = "locks")]
pub use peeps_locks as locks;

pub use collect::collect_graph;

/// Initialize peeps instrumentation.
///
/// Call this once at the start of your program, before spawning any tasks.
/// This sets up task tracking.
pub fn init() {
    peeps_futures::init_task_tracking();
}

/// Initialize peeps and start pushing snapshots to a dashboard server.
///
/// If `PEEPS_DASHBOARD` is set (e.g. `127.0.0.1:9119`), a background task
/// connects to that address and pushes periodic JSON snapshots.
pub fn init_named(process_name: impl Into<String>) {
    init();
    let name = process_name.into();
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
