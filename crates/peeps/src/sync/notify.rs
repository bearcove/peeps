use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Instant;

use facet::Facet;
use peeps_types::NodeKind;

// ── Attrs structs ─────────────────────────────────────

#[derive(Facet)]
struct NotifyAttrs<'a> {
    name: &'a str,
    source: &'a str,
    #[facet(rename = "wait.kind")]
    wait_kind: &'a str,
    waiters: u64,
    notify_count: u64,
    wakeup_count: u64,
    oldest_wait_ns: u64,
    elapsed_ns: u64,
    high_waiters_watermark: u64,
}

// ── Info type ───────────────────────────────────────────

pub(super) struct NotifyInfo {
    pub(super) name: String,
    pub(super) node_id: String,
    pub(super) created_at_ns: i64,
    pub(super) location: String,
    pub(super) waiters: AtomicU64,
    pub(super) notify_count: AtomicU64,
    pub(super) wakeup_count: AtomicU64,
    pub(super) total_wait_nanos: AtomicU64,
    pub(super) max_wait_nanos: AtomicU64,
    pub(super) high_waiters_watermark: AtomicU64,
    pub(super) created_at: Instant,
    pub(super) active_waiter_starts: Mutex<Vec<Instant>>,
}

// ── Storage ─────────────────────────────────────────────

static NOTIFY_REGISTRY: LazyLock<Mutex<Vec<Weak<NotifyInfo>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

fn prune_and_register_notify(info: &Arc<NotifyInfo>) {
    let mut reg = NOTIFY_REGISTRY.lock().unwrap();
    reg.retain(|w| w.strong_count() > 0);
    reg.push(Arc::downgrade(info));
}

// ── Helpers ─────────────────────────────────────────────

fn update_max(target: &AtomicU64, observed: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while observed > current {
        match target.compare_exchange_weak(current, observed, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

// ── DiagnosticNotify ────────────────────────────────────

pub struct DiagnosticNotify {
    inner: Arc<tokio::sync::Notify>,
    info: Arc<NotifyInfo>,
}

impl Clone for DiagnosticNotify {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            info: Arc::clone(&self.info),
        }
    }
}

impl DiagnosticNotify {
    #[track_caller]
    pub fn new(name: impl Into<String>) -> Self {
        let caller = std::panic::Location::caller();
        let location = crate::caller_location(caller);
        let info = Arc::new(NotifyInfo {
            name: name.into(),
            node_id: peeps_types::new_node_id("notify"),
            created_at_ns: crate::registry::created_at_now_ns(),
            location,
            waiters: AtomicU64::new(0),
            notify_count: AtomicU64::new(0),
            wakeup_count: AtomicU64::new(0),
            total_wait_nanos: AtomicU64::new(0),
            max_wait_nanos: AtomicU64::new(0),
            high_waiters_watermark: AtomicU64::new(0),
            created_at: Instant::now(),
            active_waiter_starts: Mutex::new(Vec::new()),
        });
        prune_and_register_notify(&info);
        Self {
            inner: Arc::new(tokio::sync::Notify::new()),
            info,
        }
    }

    /// Wait for a notification.
    pub async fn notified(&self) {
        let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
        update_max(&self.info.high_waiters_watermark, new_waiters);
        let start = Instant::now();
        self.info.active_waiter_starts.lock().unwrap().push(start);
        let mut edge_src: Option<String> = None;
        crate::stack::with_top(|src| {
            edge_src = Some(src.to_string());
            crate::registry::edge(src, &self.info.node_id);
        });

        self.inner.notified().await;

        if let Some(ref src) = edge_src {
            crate::registry::remove_edge(src, &self.info.node_id);
        }
        self.info.waiters.fetch_sub(1, Ordering::Relaxed);
        {
            let mut starts = self.info.active_waiter_starts.lock().unwrap();
            if let Some(pos) = starts.iter().position(|&s| s == start) {
                starts.swap_remove(pos);
            }
        }
        self.info.wakeup_count.fetch_add(1, Ordering::Relaxed);
        let waited_nanos = start.elapsed().as_nanos() as u64;
        self.info
            .total_wait_nanos
            .fetch_add(waited_nanos, Ordering::Relaxed);
        update_max(&self.info.max_wait_nanos, waited_nanos);
    }

    /// Notify a single waiting task.
    pub fn notify_one(&self) {
        self.info.notify_count.fetch_add(1, Ordering::Relaxed);
        self.inner.notify_one();
    }

    /// Notify all waiting tasks.
    pub fn notify_waiters(&self) {
        self.info.notify_count.fetch_add(1, Ordering::Relaxed);
        self.inner.notify_waiters();
    }
}

// ── Graph emission ──────────────────────────────────────

pub(super) fn emit_notify_nodes(graph: &mut peeps_types::GraphSnapshot) {
    let now = Instant::now();
    let reg = NOTIFY_REGISTRY.lock().unwrap();

    for info in reg.iter().filter_map(|w| w.upgrade()) {
        let name = &info.name;
        let waiters = info.waiters.load(Ordering::Relaxed);
        let notify_count = info.notify_count.load(Ordering::Relaxed);
        let wakeup_count = info.wakeup_count.load(Ordering::Relaxed);
        let high_waiters_watermark = info.high_waiters_watermark.load(Ordering::Relaxed);

        let oldest_wait_ns = {
            let starts = info.active_waiter_starts.lock().unwrap();
            starts
                .iter()
                .map(|&s| now.duration_since(s).as_nanos() as u64)
                .max()
                .unwrap_or(0)
        };

        let elapsed_ns = (now
            .duration_since(info.created_at)
            .as_nanos()
            .min(u64::MAX as u128)) as u64;

        let attrs = NotifyAttrs {
            name,
            wait_kind: "notify",
            waiters,
            notify_count,
            wakeup_count,
            oldest_wait_ns,
            elapsed_ns,
            high_waiters_watermark,
            source: &info.location,
        };

        graph.nodes.push(crate::registry::make_node(
            info.node_id.clone(),
            NodeKind::Notify,
            Some(name.clone()),
            facet_json::to_string(&attrs).unwrap(),
            info.created_at_ns,
        ));
    }
}
