use std::future::Future;


// ── Diagnostics statics ─────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) static NEXT_FUTURE_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

#[cfg(feature = "diagnostics")]
pub(crate) static FUTURE_WAIT_REGISTRY: std::sync::Mutex<
    Option<std::collections::HashMap<FutureId, FutureWaitInfo>>,
> = std::sync::Mutex::new(None);

#[cfg(feature = "diagnostics")]
pub(crate) static FUTURE_SPAWN_EDGE_REGISTRY: std::sync::Mutex<
    Option<Vec<FutureSpawnEdgeInfo>>,
> = std::sync::Mutex::new(None);

#[cfg(feature = "diagnostics")]
pub(crate) static FUTURE_POLL_EDGE_REGISTRY: std::sync::Mutex<
    Option<std::collections::HashMap<(TaskId, FutureId), FuturePollEdgeInfo>>,
> = std::sync::Mutex::new(None);

#[cfg(feature = "diagnostics")]
std::thread_local! {
    pub(crate) static CURRENT_POLLING_FUTURE: std::cell::Cell<Option<FutureId>> = const { std::cell::Cell::new(None) };
}

// ── Types (diagnostics only) ────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) struct FutureWaitInfo {
    pub(crate) resource: String,
    pub(crate) created_at: std::time::Instant,
    pub(crate) created_by_task_id: Option<TaskId>,
    pub(crate) last_polled_by_task_id: Option<TaskId>,
    pub(crate) pending_count: u64,
    pub(crate) ready_count: u64,
    pub(crate) total_pending: std::time::Duration,
    pub(crate) last_seen: std::time::Instant,
}

#[cfg(feature = "diagnostics")]
pub(crate) struct FutureSpawnEdgeInfo {
    pub(crate) parent_future_id: FutureId,
    pub(crate) parent_resource: String,
    pub(crate) child_future_id: FutureId,
    pub(crate) child_resource: String,
    pub(crate) created_by_task_id: Option<TaskId>,
    pub(crate) created_at: std::time::Instant,
}

#[cfg(feature = "diagnostics")]
pub(crate) struct FuturePollEdgeInfo {
    pub(crate) resource: String,
    pub(crate) poll_count: u64,
    pub(crate) total_poll: std::time::Duration,
    pub(crate) last_poll_at: std::time::Instant,
}

// ── Recording functions ─────────────────────────────────

#[cfg(feature = "diagnostics")]
fn register_future(future_id: FutureId, resource: String, created_by_task_id: Option<TaskId>) {
    let mut registry = FUTURE_WAIT_REGISTRY.lock().unwrap();
    let Some(waits) = registry.as_mut() else {
        return;
    };
    waits.entry(future_id).or_insert(FutureWaitInfo {
        resource,
        created_at: std::time::Instant::now(),
        created_by_task_id,
        last_polled_by_task_id: created_by_task_id,
        pending_count: 0,
        ready_count: 0,
        total_pending: std::time::Duration::from_secs(0),
        last_seen: std::time::Instant::now(),
    });
}

#[cfg(feature = "diagnostics")]
fn future_resource_by_id(future_id: FutureId) -> Option<String> {
    let registry = FUTURE_WAIT_REGISTRY.lock().unwrap();
    let waits = registry.as_ref()?;
    waits.get(&future_id).map(|w| w.resource.clone())
}

#[cfg(feature = "diagnostics")]
fn record_future_pending(future_id: FutureId, task_id: Option<TaskId>) {
    let mut registry = FUTURE_WAIT_REGISTRY.lock().unwrap();
    let Some(waits) = registry.as_mut() else {
        return;
    };
    let Some(entry) = waits.get_mut(&future_id) else {
        return;
    };
    entry.last_polled_by_task_id = task_id.or(entry.last_polled_by_task_id);
    entry.pending_count += 1;
    entry.last_seen = std::time::Instant::now();
}

#[cfg(feature = "diagnostics")]
fn record_future_ready(
    future_id: FutureId,
    task_id: Option<TaskId>,
    pending_duration: std::time::Duration,
) {
    let mut registry = FUTURE_WAIT_REGISTRY.lock().unwrap();
    let Some(waits) = registry.as_mut() else {
        return;
    };
    let Some(entry) = waits.get_mut(&future_id) else {
        return;
    };
    entry.last_polled_by_task_id = task_id.or(entry.last_polled_by_task_id);
    entry.ready_count += 1;
    entry.total_pending += pending_duration;
    entry.last_seen = std::time::Instant::now();
}

#[cfg(feature = "diagnostics")]
fn record_future_spawn_edge(
    parent_future_id: FutureId,
    parent_resource: &str,
    child_future_id: FutureId,
    child_resource: &str,
    created_by_task_id: Option<TaskId>,
) {
    let mut registry = FUTURE_SPAWN_EDGE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_mut() else {
        return;
    };
    edges.push(FutureSpawnEdgeInfo {
        parent_future_id,
        parent_resource: parent_resource.to_string(),
        child_future_id,
        child_resource: child_resource.to_string(),
        created_by_task_id,
        created_at: std::time::Instant::now(),
    });
}

#[cfg(feature = "diagnostics")]
fn record_future_poll_edge(
    task_id: TaskId,
    future_id: FutureId,
    resource: &str,
    poll_duration: std::time::Duration,
) {
    let mut registry = FUTURE_POLL_EDGE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_mut() else {
        return;
    };
    let entry = edges
        .entry((task_id, future_id))
        .or_insert(FuturePollEdgeInfo {
            resource: resource.to_string(),
            poll_count: 0,
            total_poll: std::time::Duration::ZERO,
            last_poll_at: std::time::Instant::now(),
        });
    entry.poll_count += 1;
    entry.total_poll += poll_duration;
    entry.last_poll_at = std::time::Instant::now();
}

// ── PeepableFuture (no diagnostics) ─────────────────────

#[cfg(not(feature = "diagnostics"))]
pub(crate) struct PeepableFuture<F> {
    inner: F,
}

#[cfg(not(feature = "diagnostics"))]
impl<F> Future for PeepableFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: we never move `inner` after pinning `Self`.
        #[allow(unsafe_code)]
        unsafe {
            let this = self.get_unchecked_mut();
            std::pin::Pin::new_unchecked(&mut this.inner).poll(cx)
        }
    }
}

#[cfg(not(feature = "diagnostics"))]
pub(crate) fn peepable<F>(future: F, _resource: impl Into<String>) -> PeepableFuture<F>
where
    F: Future,
{
    PeepableFuture { inner: future }
}

// ── PeepableFuture (diagnostics) ────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) struct PeepableFuture<F> {
    future_id: FutureId,
    resource: String,
    inner: F,
    pending_since: Option<std::time::Instant>,
}

#[cfg(feature = "diagnostics")]
impl<F> Future for PeepableFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: we never move fields out of `self` after pinning.
        #[allow(unsafe_code)]
        let this = unsafe { self.get_unchecked_mut() };
        // SAFETY: `inner` is pinned together with `self`.
        #[allow(unsafe_code)]
        let inner = unsafe { std::pin::Pin::new_unchecked(&mut this.inner) };
        let task_id = crate::tasks::current_task_id();
        let poll_start = std::time::Instant::now();

        // Set thread-local so child peepable() calls can record spawn edges.
        let prev = CURRENT_POLLING_FUTURE.with(|c| {
            let prev = c.get();
            c.set(Some(this.future_id));
            prev
        });

        let peepable_waker =
            std::task::Waker::from(std::sync::Arc::new(crate::wakes::PeepableWake {
                inner: cx.waker().clone(),
                future_id: this.future_id,
                future_resource: this.resource.clone(),
                target_task_id: task_id,
            }));
        let mut peepable_cx = std::task::Context::from_waker(&peepable_waker);
        let result = inner.poll(&mut peepable_cx);

        // Restore previous thread-local.
        CURRENT_POLLING_FUTURE.with(|c| c.set(prev));

        let poll_duration = poll_start.elapsed();

        // Record poll edge.
        if let Some(tid) = task_id {
            record_future_poll_edge(tid, this.future_id, &this.resource, poll_duration);
        }

        match result {
            std::task::Poll::Pending => {
                if this.pending_since.is_none() {
                    this.pending_since = Some(std::time::Instant::now());
                }
                record_future_pending(this.future_id, task_id);
                std::task::Poll::Pending
            }
            std::task::Poll::Ready(value) => {
                let pending_duration = this
                    .pending_since
                    .take()
                    .map(|t| t.elapsed())
                    .unwrap_or_default();
                record_future_ready(this.future_id, task_id, pending_duration);
                std::task::Poll::Ready(value)
            }
        }
    }
}

#[cfg(feature = "diagnostics")]
pub(crate) fn peepable<F>(future: F, resource: impl Into<String>) -> PeepableFuture<F>
where
    F: Future,
{
    use std::sync::atomic::Ordering;

    let future_id = NEXT_FUTURE_ID.fetch_add(1, Ordering::Relaxed);
    let resource = resource.into();
    let task_id = crate::tasks::current_task_id();
    register_future(future_id, resource.clone(), task_id);

    // If created during another PeepableFuture's poll, record spawn edge.
    CURRENT_POLLING_FUTURE.with(|c| {
        if let Some(parent_id) = c.get() {
            let parent_resource =
                future_resource_by_id(parent_id).unwrap_or_else(|| "unknown".to_string());
            record_future_spawn_edge(parent_id, &parent_resource, future_id, &resource, task_id);
        }
    });

    PeepableFuture {
        future_id,
        resource,
        inner: future,
        pending_since: None,
    }
}

// ── Init ────────────────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) fn init() {
    *FUTURE_WAIT_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
    *FUTURE_SPAWN_EDGE_REGISTRY.lock().unwrap() = Some(Vec::new());
    *FUTURE_POLL_EDGE_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
}

#[cfg(not(feature = "diagnostics"))]
pub(crate) fn init() {}
