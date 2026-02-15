// ── Diagnostics-enabled registry ─────────────────────────

#[cfg(feature = "diagnostics")]
use std::backtrace::Backtrace;
#[cfg(feature = "diagnostics")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "diagnostics")]
use std::sync::{Arc, LazyLock, Mutex as StdMutex, Weak};
#[cfg(feature = "diagnostics")]
use std::time::Instant;

#[cfg(feature = "diagnostics")]
pub(crate) static LOCK_REGISTRY: LazyLock<StdMutex<Vec<Weak<LockInfo>>>> =
    LazyLock::new(|| StdMutex::new(Vec::new()));

#[cfg(feature = "diagnostics")]
#[derive(Debug, Clone, Copy)]
pub(crate) enum AcquireKind {
    Read,
    Write,
    Mutex,
}

#[cfg(feature = "diagnostics")]
impl std::fmt::Display for AcquireKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcquireKind::Read => write!(f, "read"),
            AcquireKind::Write => write!(f, "write"),
            AcquireKind::Mutex => write!(f, "mutex"),
        }
    }
}

#[cfg(feature = "diagnostics")]
pub(crate) struct WaiterOrHolder {
    pub(crate) id: u64,
    pub(crate) kind: AcquireKind,
    pub(crate) since: Instant,
    pub(crate) backtrace: Backtrace,
    pub(crate) peeps_task_id: Option<u64>,
}

#[cfg(feature = "diagnostics")]
pub(crate) struct LockInfo {
    pub(crate) name: &'static str,
    pub(crate) next_id: AtomicU64,
    pub(crate) waiters: StdMutex<Vec<WaiterOrHolder>>,
    pub(crate) holders: StdMutex<Vec<WaiterOrHolder>>,
    pub(crate) total_acquires: AtomicU64,
    pub(crate) total_releases: AtomicU64,
}

#[cfg(feature = "diagnostics")]
impl LockInfo {
    pub(crate) fn new(name: &'static str) -> Arc<Self> {
        let info = Arc::new(Self {
            name,
            next_id: AtomicU64::new(0),
            waiters: StdMutex::new(Vec::new()),
            holders: StdMutex::new(Vec::new()),
            total_acquires: AtomicU64::new(0),
            total_releases: AtomicU64::new(0),
        });
        LOCK_REGISTRY.lock().unwrap().push(Arc::downgrade(&info));
        info
    }

    pub(crate) fn add_waiter(&self, kind: AcquireKind) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.waiters.lock().unwrap().push(WaiterOrHolder {
            id,
            kind,
            since: Instant::now(),
            backtrace: Backtrace::force_capture(),
            peeps_task_id: peeps_tasks::current_task_id(),
        });
        id
    }

    pub(crate) fn remove_waiter(&self, id: u64) {
        self.waiters.lock().unwrap().retain(|w| w.id != id);
    }

    pub(crate) fn remove_holder(&self, id: u64) {
        self.holders.lock().unwrap().retain(|h| h.id != id);
        self.total_releases.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn promote_waiter_to_holder(&self, waiter_id: u64, kind: AcquireKind) -> u64 {
        let holder_id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let mut waiters = self.waiters.lock().unwrap();
        let mut holders = self.holders.lock().unwrap();

        waiters.retain(|w| w.id != waiter_id);
        holders.push(WaiterOrHolder {
            id: holder_id,
            kind,
            since: Instant::now(),
            backtrace: Backtrace::force_capture(),
            peeps_task_id: peeps_tasks::current_task_id(),
        });

        self.total_acquires.fetch_add(1, Ordering::Relaxed);
        holder_id
    }
}

// ── WaiterToken (cancellation-safe waiter cleanup) ──────

#[cfg(feature = "diagnostics")]
pub(crate) struct WaiterToken {
    info: Arc<LockInfo>,
    pub(crate) id: u64,
    armed: bool,
}

#[cfg(feature = "diagnostics")]
impl WaiterToken {
    pub(crate) fn new(info: &Arc<LockInfo>, kind: AcquireKind) -> Self {
        let id = info.add_waiter(kind);
        Self {
            info: Arc::clone(info),
            id,
            armed: true,
        }
    }

    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

#[cfg(feature = "diagnostics")]
impl Drop for WaiterToken {
    fn drop(&mut self) {
        if self.armed {
            self.info.remove_waiter(self.id);
        }
    }
}

// ── Format helpers (diagnostics only) ───────────────────

#[cfg(feature = "diagnostics")]
pub(crate) fn format_peeps_task_info(peeps_task_id: Option<u64>) -> String {
    match peeps_task_id {
        Some(id) => {
            let name = peeps_tasks::task_name(id);
            match name {
                Some(name) => format!(" task={id}({name})"),
                None => format!(" task={id}"),
            }
        }
        None => String::new(),
    }
}

#[cfg(feature = "diagnostics")]
pub(crate) fn format_backtrace(bt: &Backtrace, out: &mut String) {
    let text = format!("{bt}");
    let mut shown = 0;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("vx_runner")
            || trimmed.contains("vx_vfs")
            || trimmed.contains("fuse3")
            || trimmed.contains("cas_vfs")
            || trimmed.contains("diagnostic_lock")
        {
            out.push_str(&format!("      {trimmed}\n"));
            shown += 1;
            if shown >= 8 {
                break;
            }
        }
    }
    if shown == 0 {
        for line in text.lines().take(6) {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                out.push_str(&format!("      {trimmed}\n"));
            }
        }
    }
}
