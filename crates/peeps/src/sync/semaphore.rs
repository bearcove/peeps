use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Instant;

use facet::Facet;
use peeps_types::NodeKind;

// ── Attrs structs ─────────────────────────────────────

#[derive(Facet)]
struct SemaphoreAttrs<'a> {
    name: &'a str,
    source: &'a str,
    permits_total: u64,
    permits_available: u64,
    waiters: u64,
    acquires: u64,
    oldest_wait_ns: u64,
    high_waiters_watermark: u64,
}

// ── Info type ───────────────────────────────────────────

pub(super) struct SemaphoreInfo {
    pub(super) name: String,
    pub(super) node_id: String,
    pub(super) created_at_ns: i64,
    pub(super) location: String,
    pub(super) permits_total: u64,
    pub(super) waiters: AtomicU64,
    pub(super) acquires: AtomicU64,
    pub(super) total_wait_nanos: AtomicU64,
    pub(super) max_wait_nanos: AtomicU64,
    pub(super) high_waiters_watermark: AtomicU64,
    pub(super) available_permits: Box<dyn Fn() -> usize + Send + Sync>,
    pub(super) active_waiter_starts: Mutex<Vec<Instant>>,
}

// ── Storage ─────────────────────────────────────────────

static SEMAPHORE_REGISTRY: LazyLock<Mutex<Vec<Weak<SemaphoreInfo>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

fn prune_and_register_semaphore(info: &Arc<SemaphoreInfo>) {
    let mut reg = SEMAPHORE_REGISTRY.lock().unwrap();
    reg.retain(|w| w.strong_count() > 0);
    reg.push(Arc::downgrade(info));
}

// ── Helpers ─────────────────────────────────────────────

fn update_max_wait(max_wait_nanos: &AtomicU64, observed: u64) {
    let mut current = max_wait_nanos.load(Ordering::Relaxed);
    while observed > current {
        match max_wait_nanos.compare_exchange_weak(
            current,
            observed,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

// ── DiagnosticSemaphore ─────────────────────────────────

pub struct DiagnosticSemaphore {
    inner: Arc<tokio::sync::Semaphore>,
    info: Arc<SemaphoreInfo>,
}

impl Clone for DiagnosticSemaphore {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            info: Arc::clone(&self.info),
        }
    }
}

impl DiagnosticSemaphore {
    #[track_caller]
    pub fn new(name: impl Into<String>, permits: usize) -> Self {
        let inner = Arc::new(tokio::sync::Semaphore::new(permits));
        let inner_for_snapshot = Arc::clone(&inner);
        let caller = std::panic::Location::caller();
        let location = crate::caller_location(caller);
        let info = Arc::new(SemaphoreInfo {
            name: name.into(),
            node_id: peeps_types::new_node_id("semaphore"),
            created_at_ns: crate::registry::created_at_now_ns(),
            location,
            permits_total: permits as u64,
            waiters: AtomicU64::new(0),
            acquires: AtomicU64::new(0),
            total_wait_nanos: AtomicU64::new(0),
            max_wait_nanos: AtomicU64::new(0),
            high_waiters_watermark: AtomicU64::new(0),
            available_permits: Box::new(move || inner_for_snapshot.available_permits()),
            active_waiter_starts: Mutex::new(Vec::new()),
        });
        prune_and_register_semaphore(&info);
        Self { inner, info }
    }

    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    pub fn close(&self) {
        self.inner.close();
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    pub fn add_permits(&self, n: usize) {
        self.inner.add_permits(n);
    }

    pub async fn acquire(
        &self,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
        let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
        update_max_wait(&self.info.high_waiters_watermark, new_waiters);
        let start = Instant::now();
        self.info.active_waiter_starts.lock().unwrap().push(start);
        let mut edge_src: Option<String> = None;
        crate::stack::with_top(|src| {
            edge_src = Some(src.to_string());
            crate::registry::edge(src, &self.info.node_id);
        });
        let result = self.inner.acquire().await;
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
        if result.is_ok() {
            self.info.acquires.fetch_add(1, Ordering::Relaxed);
            let waited_nanos = start.elapsed().as_nanos() as u64;
            self.info
                .total_wait_nanos
                .fetch_add(waited_nanos, Ordering::Relaxed);
            update_max_wait(&self.info.max_wait_nanos, waited_nanos);
        }
        result
    }

    pub async fn acquire_many(
        &self,
        n: u32,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
        let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
        update_max_wait(&self.info.high_waiters_watermark, new_waiters);
        let start = Instant::now();
        self.info.active_waiter_starts.lock().unwrap().push(start);
        let mut edge_src: Option<String> = None;
        crate::stack::with_top(|src| {
            edge_src = Some(src.to_string());
            crate::registry::edge(src, &self.info.node_id);
        });
        let result = self.inner.acquire_many(n).await;
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
        if result.is_ok() {
            self.info.acquires.fetch_add(1, Ordering::Relaxed);
            let waited_nanos = start.elapsed().as_nanos() as u64;
            self.info
                .total_wait_nanos
                .fetch_add(waited_nanos, Ordering::Relaxed);
            update_max_wait(&self.info.max_wait_nanos, waited_nanos);
        }
        result
    }

    pub async fn acquire_owned(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
        let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
        update_max_wait(&self.info.high_waiters_watermark, new_waiters);
        let start = Instant::now();
        self.info.active_waiter_starts.lock().unwrap().push(start);
        let mut edge_src: Option<String> = None;
        crate::stack::with_top(|src| {
            edge_src = Some(src.to_string());
            crate::registry::edge(src, &self.info.node_id);
        });
        let result = Arc::clone(&self.inner).acquire_owned().await;
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
        if result.is_ok() {
            self.info.acquires.fetch_add(1, Ordering::Relaxed);
            let waited_nanos = start.elapsed().as_nanos() as u64;
            self.info
                .total_wait_nanos
                .fetch_add(waited_nanos, Ordering::Relaxed);
            update_max_wait(&self.info.max_wait_nanos, waited_nanos);
        }
        result
    }

    pub async fn acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
        let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
        update_max_wait(&self.info.high_waiters_watermark, new_waiters);
        let start = Instant::now();
        self.info.active_waiter_starts.lock().unwrap().push(start);
        let mut edge_src: Option<String> = None;
        crate::stack::with_top(|src| {
            edge_src = Some(src.to_string());
            crate::registry::edge(src, &self.info.node_id);
        });
        let result = Arc::clone(&self.inner).acquire_many_owned(n).await;
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
        if result.is_ok() {
            self.info.acquires.fetch_add(1, Ordering::Relaxed);
            let waited_nanos = start.elapsed().as_nanos() as u64;
            self.info
                .total_wait_nanos
                .fetch_add(waited_nanos, Ordering::Relaxed);
            update_max_wait(&self.info.max_wait_nanos, waited_nanos);
        }
        result
    }

    pub fn try_acquire(
        &self,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let result = self.inner.try_acquire();
        if result.is_ok() {
            self.info.acquires.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub fn try_acquire_many(
        &self,
        n: u32,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let result = self.inner.try_acquire_many(n);
        if result.is_ok() {
            self.info.acquires.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub fn try_acquire_owned(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let result = Arc::clone(&self.inner).try_acquire_owned();
        if result.is_ok() {
            self.info.acquires.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub fn try_acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let result = Arc::clone(&self.inner).try_acquire_many_owned(n);
        if result.is_ok() {
            self.info.acquires.fetch_add(1, Ordering::Relaxed);
        }
        result
    }
}

// ── Graph emission ──────────────────────────────────────

pub(super) fn emit_semaphore_nodes(graph: &mut peeps_types::GraphSnapshot) {
    let now = Instant::now();
    let reg = SEMAPHORE_REGISTRY.lock().unwrap();

    for info in reg.iter().filter_map(|w| w.upgrade()) {
        let name = &info.name;
        let permits_available = (info.available_permits)() as u64;
        let waiters = info.waiters.load(Ordering::Relaxed);
        let acquires = info.acquires.load(Ordering::Relaxed);
        let high_waiters_watermark = info.high_waiters_watermark.load(Ordering::Relaxed);

        let oldest_wait_ns = {
            let starts = info.active_waiter_starts.lock().unwrap();
            starts
                .iter()
                .map(|&s| now.duration_since(s).as_nanos() as u64)
                .max()
                .unwrap_or(0)
        };

        let attrs = SemaphoreAttrs {
            name,
            permits_total: info.permits_total,
            permits_available,
            waiters,
            acquires,
            oldest_wait_ns,
            high_waiters_watermark,
            source: &info.location,
        };

        graph.nodes.push(crate::registry::make_node(
            info.node_id.clone(),
            NodeKind::Semaphore,
            Some(name.clone()),
            facet_json::to_string(&attrs).unwrap(),
            info.created_at_ns,
        ));
    }
}
