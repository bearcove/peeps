//! peeps-core - Easy instrumentation and diagnostics
//!
//! This crate provides the main API for instrumenting your application:
//! - `peeps::init()` - Initialize all instrumentation
//! - `peeps::install_sigusr1()` - Set up SIGUSR1 dump on signal
//! - `peeps::dump()` - Manually trigger a diagnostic dump
//! - `peeps::serve_dashboard()` - Serve the interactive web dashboard

use facet::Facet;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub use peeps_tasks as tasks;
pub use peeps_threads as threads;

#[cfg(feature = "locks")]
pub use peeps_locks as locks;

#[cfg(feature = "roam-session")]
pub use peeps_roam as roam;

/// The dump directory where processes write their JSON dumps.
pub const DUMP_DIR: &str = "/tmp/peeps-dumps";

/// Per-process diagnostic dump.
#[derive(Debug, Clone, Facet)]
pub struct ProcessDump {
    /// Process name (e.g., "my-server").
    pub process_name: String,
    /// Process ID.
    pub pid: u32,
    /// ISO 8601 timestamp of when the dump was taken.
    pub timestamp: String,
    /// Tracked Tokio task snapshots.
    pub tasks: Vec<peeps_tasks::TaskSnapshot>,
    /// Thread stack traces (captured via SIGPROF).
    pub threads: Vec<peeps_threads::ThreadStackSnapshot>,
    /// Lock contention diagnostics (if locks feature enabled).
    #[cfg(feature = "locks")]
    pub locks: Option<peeps_locks::LockSnapshot>,
    /// Roam session diagnostics (if roam-session feature enabled).
    #[cfg(feature = "roam-session")]
    pub roam: Option<peeps_roam::DiagnosticSnapshot>,
    /// Roam SHM diagnostics (if roam-shm feature enabled).
    #[cfg(feature = "roam-shm")]
    pub shm: Option<peeps_roam::ShmSnapshot>,
    /// Process-specific key-value extras.
    pub custom: HashMap<String, String>,
}

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
///
/// # Example
/// ```no_run
/// peeps::init();
/// peeps::install_sigusr1("my-service");
///
/// // Now run: kill -SIGUSR1 <pid>
/// // Dump will be written to /tmp/peeps-dumps/<pid>.json
/// ```
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
///
/// This is useful for triggered dumps or periodic snapshots.
pub fn collect_dump(process_name: &str, custom: HashMap<String, String>) -> ProcessDump {
    let timestamp = {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let total_secs = d.as_secs();
        let millis = d.subsec_millis();

        // Compute UTC date/time from epoch seconds (civil_from_days algorithm)
        let days = (total_secs / 86400) as i64;
        let day_secs = (total_secs % 86400) as u32;
        let hours = day_secs / 3600;
        let minutes = (day_secs % 3600) / 60;
        let seconds = day_secs % 60;

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
    };

    let tasks = peeps_tasks::snapshot_all_tasks();
    let threads = peeps_threads::collect_all_thread_stacks();

    ProcessDump {
        process_name: process_name.to_string(),
        pid: std::process::id(),
        timestamp,
        tasks,
        threads,
        #[cfg(feature = "locks")]
        locks: Some(peeps_locks::snapshot_lock_diagnostics()),
        #[cfg(feature = "roam-session")]
        roam: Some(peeps_roam::snapshot_session()),
        #[cfg(feature = "roam-shm")]
        shm: Some(peeps_roam::snapshot_shm()),
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

/// Serve the interactive dashboard on a local port.
///
/// This loads all dumps from `/tmp/peeps-dumps/` and serves a web UI.
/// Opens the browser automatically if possible.
///
/// # Example
/// ```no_run
/// # async fn example() {
/// peeps::serve_dashboard().await.unwrap();
/// # }
/// ```
#[cfg(feature = "dashboard")]
pub async fn serve_dashboard() -> std::io::Result<()> {
    let dumps = read_all_dumps();
    if dumps.is_empty() {
        eprintln!("[peeps] No dumps found in {}", DUMP_DIR);
        eprintln!("[peeps] Trigger a dump with: kill -SIGUSR1 <pid>");
        return Ok(());
    }

    // TODO: Implement dashboard server (similar to vx debug)
    // For now, just print the dumps
    eprintln!("[peeps] Found {} dumps", dumps.len());
    for dump in &dumps {
        eprintln!(
            "  {} (pid {}): {} tasks, {} threads",
            dump.process_name,
            dump.pid,
            dump.tasks.len(),
            dump.threads.len()
        );
    }

    Ok(())
}
