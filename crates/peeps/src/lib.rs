//! peeps - Easy instrumentation and diagnostics
//!
//! This crate provides the main API for instrumenting your application:
//! - `peeps::init()` - Initialize all instrumentation
//! - `peeps::collect_dump()` - Manually collect a diagnostic dump

use std::collections::HashMap;

#[cfg(feature = "dashboard")]
mod dashboard_client;

pub use peeps_sync as sync;
pub use peeps_tasks as tasks;
pub use peeps_threads as threads;
pub use peeps_types::{self as types, Diagnostics, ProcessDump};

#[cfg(feature = "locks")]
pub use peeps_locks as locks;

/// Initialize peeps instrumentation.
///
/// Call this once at the start of your program, before spawning any tasks or threads.
/// This sets up task tracking and thread sampling.
pub fn init() {
    peeps_tasks::init_task_tracking();
    peeps_threads::install_sigprof_handler();
    peeps_threads::register_thread("main");
}

/// Initialize peeps and start pushing snapshots to a dashboard server.
///
/// If `PEEPS_DASHBOARD` is set (e.g. `127.0.0.1:9119`), a background task
/// connects to that address and pushes periodic JSON snapshots.
pub fn init_named(process_name: impl Into<String>) {
    init();
    #[cfg(feature = "dashboard")]
    {
        if let Ok(addr) = std::env::var("PEEPS_DASHBOARD") {
            dashboard_client::start_push_loop(
                process_name.into(),
                addr,
                std::time::Duration::from_secs(2),
            );
            return;
        }
    }
    let _ = process_name;
}

/// Manually collect a diagnostic dump.
pub fn collect_dump(process_name: &str, custom: HashMap<String, String>) -> ProcessDump {
    let timestamp = format_timestamp();

    let tasks = peeps_tasks::snapshot_all_tasks();
    let wake_edges = peeps_tasks::snapshot_wake_edges();
    let future_wake_edges = peeps_tasks::snapshot_future_wake_edges();
    let future_waits = peeps_tasks::snapshot_future_waits();
    let threads = peeps_threads::collect_all_thread_stacks();

    #[cfg(feature = "locks")]
    let locks = Some(peeps_locks::snapshot_lock_diagnostics());
    #[cfg(not(feature = "locks"))]
    let locks = None;

    let sync = {
        let snap = peeps_sync::snapshot_all();
        if snap.mpsc_channels.is_empty()
            && snap.oneshot_channels.is_empty()
            && snap.watch_channels.is_empty()
            && snap.once_cells.is_empty()
        {
            None
        } else {
            Some(snap)
        }
    };

    // Collect roam diagnostics from inventory-registered sources
    let all_diags = peeps_types::collect_all_diagnostics();
    let mut roam = None;
    let mut shm = None;
    for diag in all_diags {
        match diag {
            Diagnostics::RoamSession(s) => roam = Some(s),
            Diagnostics::RoamShm(s) => shm = Some(s),
        }
    }

    ProcessDump {
        process_name: process_name.to_string(),
        pid: std::process::id(),
        timestamp,
        tasks,
        wake_edges,
        future_wake_edges,
        future_waits,
        threads,
        locks,
        sync,
        roam,
        shm,
        custom,
    }
}

fn format_timestamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = d.as_secs();
    let millis = d.subsec_millis();

    let day_secs = (total_secs % 86400) as u32;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let seconds = day_secs % 60;

    let days = (total_secs / 86400) as i64;
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}
