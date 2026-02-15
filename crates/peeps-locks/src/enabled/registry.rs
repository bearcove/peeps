// ── Diagnostics-enabled registry ─────────────────────────

use std::backtrace::Backtrace;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex as StdMutex, Weak};
use std::time::Instant;

pub(crate) static LOCK_REGISTRY: LazyLock<StdMutex<Vec<Weak<LockInfo>>>> =
    LazyLock::new(|| StdMutex::new(Vec::new()));

#[derive(Debug, Clone, Copy)]
pub(crate) enum AcquireKind {
    Read,
    Write,
    Mutex,
}

impl std::fmt::Display for AcquireKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcquireKind::Read => write!(f, "read"),
            AcquireKind::Write => write!(f, "write"),
            AcquireKind::Mutex => write!(f, "mutex"),
        }
    }
}

pub(crate) struct WaiterOrHolder {
    pub(crate) id: u64,
    pub(crate) kind: AcquireKind,
    pub(crate) since: Instant,
    pub(crate) backtrace: Backtrace,
    pub(crate) peeps_task_id: Option<u64>,
}

pub(crate) struct LockInfo {
    pub(crate) name: &'static str,
    pub(crate) next_id: AtomicU64,
    pub(crate) waiters: StdMutex<Vec<WaiterOrHolder>>,
    pub(crate) holders: StdMutex<Vec<WaiterOrHolder>>,
    pub(crate) total_acquires: AtomicU64,
    pub(crate) total_releases: AtomicU64,
}

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
            peeps_task_id: peeps_futures::current_task_id(),
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
            peeps_task_id: peeps_futures::current_task_id(),
        });

        self.total_acquires.fetch_add(1, Ordering::Relaxed);
        holder_id
    }
}

// ── WaiterToken (cancellation-safe waiter cleanup) ──────

pub(crate) struct WaiterToken {
    info: Arc<LockInfo>,
    pub(crate) id: u64,
    armed: bool,
}

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

impl Drop for WaiterToken {
    fn drop(&mut self) {
        if self.armed {
            self.info.remove_waiter(self.id);
        }
    }
}
