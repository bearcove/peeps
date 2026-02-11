//! peeps - Easy instrumentation and diagnostics
//!
//! This crate provides the main API for instrumenting your application:
//! - `peeps::init()` - Initialize all instrumentation
//! - `peeps::install_sigusr1()` - Set up SIGUSR1 dump on signal
//! - `peeps::collect_dump()` - Manually collect a diagnostic dump
//! - `peeps::write_dump()` - Write a dump to disk

use std::collections::HashMap;
use std::path::Path;

pub use peeps_tasks as tasks;
pub use peeps_threads as threads;
pub use peeps_types::{self as types, Diagnostics, ProcessDump};

#[cfg(feature = "locks")]
pub use peeps_locks as locks;

/// The dump directory where processes write their JSON dumps.
pub const DUMP_DIR: &str = "/tmp/peeps-dumps";

/// Initialize peeps instrumentation.
///
/// Call this once at the start of your program, before spawning any tasks or threads.
/// This sets up task tracking and thread sampling.
pub fn init() {
    peeps_tasks::init_task_tracking();
    peeps_threads::install_sigprof_handler();
    peeps_threads::register_thread("main");
}

/// Install a SIGUSR1 handler that dumps diagnostics on signal.
///
/// On Unix systems, sending SIGUSR1 to the process will trigger a dump to `/tmp/peeps-dumps/{pid}.json`.
#[cfg(unix)]
pub fn install_sigusr1(process_name: impl Into<String>) {
    let name = process_name.into();
    peeps_tasks::spawn_tracked("peeps_sigusr1_handler", async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigusr1 =
            signal(SignalKind::user_defined1()).expect("failed to register SIGUSR1 handler");
        loop {
            sigusr1.recv().await;
            eprintln!("[peeps] SIGUSR1 received, dumping diagnostics");
            let dump = collect_dump(&name, HashMap::new());
            write_dump(&dump);
        }
    });
}

/// Manually collect a diagnostic dump.
pub fn collect_dump(process_name: &str, custom: HashMap<String, String>) -> ProcessDump {
    let timestamp = format_timestamp();

    let tasks = peeps_tasks::snapshot_all_tasks();
    let threads = peeps_threads::collect_all_thread_stacks();

    #[cfg(feature = "locks")]
    let locks = Some(peeps_locks::snapshot_lock_diagnostics());
    #[cfg(not(feature = "locks"))]
    let locks = None;

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
        threads,
        locks,
        roam,
        shm,
        custom,
    }
}

/// Write a process dump as JSON to `/tmp/peeps-dumps/{pid}.json`.
pub fn write_dump(dump: &ProcessDump) {
    let dir = Path::new(DUMP_DIR);
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("[peeps] failed to create {DUMP_DIR}: {e}");
        return;
    }

    let path = dir.join(format!("{}.json", dump.pid));

    let json = match facet_json::to_string(dump) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[peeps] failed to serialize dump: {e}");
            return;
        }
    };

    let tmp_path = dir.join(format!("{}.json.tmp", dump.pid));
    match std::fs::File::create(&tmp_path) {
        Ok(mut f) => {
            use std::io::Write;
            if let Err(e) = f.write_all(json.as_bytes()) {
                eprintln!("[peeps] failed to write {}: {e}", tmp_path.display());
                let _ = std::fs::remove_file(&tmp_path);
                return;
            }
        }
        Err(e) => {
            eprintln!("[peeps] failed to create {}: {e}", tmp_path.display());
            return;
        }
    }

    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        eprintln!("[peeps] failed to rename to {}: {e}", path.display());
        let _ = std::fs::remove_file(&tmp_path);
    } else {
        eprintln!("[peeps] dump written to {}", path.display());
    }
}

/// Read all process dumps from the dump directory.
pub fn read_all_dumps() -> Vec<ProcessDump> {
    let dir = Path::new(DUMP_DIR);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut dumps = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            match std::fs::read_to_string(&path) {
                Ok(json) => match facet_json::from_str::<ProcessDump>(&json) {
                    Ok(dump) => dumps.push(dump),
                    Err(e) => {
                        eprintln!("[peeps] failed to parse {}: {e}", path.display());
                    }
                },
                Err(e) => {
                    eprintln!("[peeps] failed to read {}: {e}", path.display());
                }
            }
        }
    }

    dumps.sort_by(|a, b| a.process_name.cmp(&b.process_name));
    dumps
}

/// Clean stale dump files from the dump directory.
pub fn clean_dumps() {
    let dir = Path::new(DUMP_DIR);
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json" || e == "tmp") {
                let _ = std::fs::remove_file(&path);
            }
        }
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
