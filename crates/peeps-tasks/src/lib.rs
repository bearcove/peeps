//! Task instrumentation for Tokio spawned tasks.
//!
//! When the `diagnostics` feature is enabled, wraps spawned tasks to capture
//! timing, poll events, and backtraces. When disabled, `spawn_tracked` is
//! a zero-cost wrapper around `tokio::spawn`.

use std::future::Future;

pub use peeps_types::{
    FutureId, FuturePollEdgeSnapshot, FutureResumeEdgeSnapshot, FutureSpawnEdgeSnapshot,
    FutureWaitSnapshot, FutureWakeEdgeSnapshot, PollEvent, PollResult, TaskId, TaskSnapshot,
    TaskState, WakeEdgeSnapshot,
};

// ── Zero-cost stubs (no diagnostics) ─────────────────────────────

#[cfg(not(feature = "diagnostics"))]
mod imp {
    use super::*;

    #[inline(always)]
    pub fn init_task_tracking() {}

    #[inline(always)]
    pub fn current_task_id() -> Option<TaskId> {
        None
    }

    #[inline(always)]
    pub fn task_name(_id: TaskId) -> Option<String> {
        None
    }

    #[inline(always)]
    #[track_caller]
    pub fn spawn_tracked<F>(
        _name: impl Into<String>,
        future: F,
    ) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(future)
    }

    #[inline(always)]
    pub fn snapshot_all_tasks() -> Vec<TaskSnapshot> {
        Vec::new()
    }

    #[inline(always)]
    pub fn snapshot_wake_edges() -> Vec<WakeEdgeSnapshot> {
        Vec::new()
    }

    #[inline(always)]
    pub fn snapshot_future_wake_edges() -> Vec<FutureWakeEdgeSnapshot> {
        Vec::new()
    }

    #[inline(always)]
    pub fn snapshot_future_waits() -> Vec<FutureWaitSnapshot> {
        Vec::new()
    }

    #[inline(always)]
    pub fn snapshot_future_spawn_edges() -> Vec<FutureSpawnEdgeSnapshot> {
        Vec::new()
    }

    #[inline(always)]
    pub fn snapshot_future_poll_edges() -> Vec<FuturePollEdgeSnapshot> {
        Vec::new()
    }

    #[inline(always)]
    pub fn snapshot_future_resume_edges() -> Vec<FutureResumeEdgeSnapshot> {
        Vec::new()
    }

    pub struct PeepableFuture<F> {
        inner: F,
    }

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

    pub fn peepable<F>(future: F, _resource: impl Into<String>) -> PeepableFuture<F>
    where
        F: Future,
    {
        PeepableFuture { inner: future }
    }

    #[inline(always)]
    pub fn cleanup_completed_tasks() {}
}

// ── Tracing implementation (diagnostics enabled) ─────────────────

#[cfg(feature = "diagnostics")]
mod imp {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::task::Wake;
    use std::task::{Context, Poll};
    use std::time::Instant;

    use backtrace::Backtrace;

    tokio::task_local! {
        static CURRENT_TASK_ID: TaskId;
    }

    pub fn current_task_id() -> Option<TaskId> {
        CURRENT_TASK_ID.try_with(|id| *id).ok()
    }

    pub fn task_name(id: TaskId) -> Option<String> {
        let registry = TASK_REGISTRY.lock().unwrap();
        let tasks = registry.as_ref()?;
        tasks.iter().find(|t| t.id == id).map(|t| t.name.clone())
    }

    static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);
    static TASK_REGISTRY: Mutex<Option<Vec<Arc<TaskInfo>>>> = Mutex::new(None);
    static WAKE_REGISTRY: Mutex<
        Option<std::collections::HashMap<(Option<TaskId>, TaskId), WakeEdgeInfo>>,
    > = Mutex::new(None);
    static FUTURE_WAKE_EDGE_REGISTRY: Mutex<
        Option<std::collections::HashMap<(Option<TaskId>, FutureId), FutureWakeEdgeInfo>>,
    > = Mutex::new(None);
    static NEXT_FUTURE_ID: AtomicU64 = AtomicU64::new(1);
    static FUTURE_WAIT_REGISTRY: Mutex<Option<std::collections::HashMap<FutureId, FutureWaitInfo>>> =
        Mutex::new(None);
    static FUTURE_SPAWN_EDGE_REGISTRY: Mutex<Option<Vec<FutureSpawnEdgeInfo>>> = Mutex::new(None);
    static FUTURE_POLL_EDGE_REGISTRY: Mutex<
        Option<std::collections::HashMap<(TaskId, FutureId), FuturePollEdgeInfo>>,
    > = Mutex::new(None);
    static FUTURE_RESUME_EDGE_REGISTRY: Mutex<
        Option<std::collections::HashMap<(FutureId, TaskId), FutureResumeEdgeInfo>>,
    > = Mutex::new(None);

    std::thread_local! {
        static CURRENT_POLLING_FUTURE: std::cell::Cell<Option<FutureId>> = const { std::cell::Cell::new(None) };
    }

    struct TaskInfo {
        id: TaskId,
        name: String,
        parent_id: Option<TaskId>,
        spawned_at: Instant,
        spawn_backtrace: String,
        state: Mutex<TaskInfoState>,
    }

    struct TaskInfoState {
        state: TaskState,
        poll_events: Vec<PollEventInternal>,
    }

    struct PollEventInternal {
        started_at: Instant,
        duration: Option<std::time::Duration>,
        result: PollResult,
        backtrace: Option<String>,
    }

    struct WakeEdgeInfo {
        wake_count: u64,
        last_wake_at: Instant,
    }

    struct FutureWakeEdgeInfo {
        wake_count: u64,
        last_wake_at: Instant,
    }

    struct FutureWaitInfo {
        resource: String,
        created_at: Instant,
        created_by_task_id: Option<TaskId>,
        last_polled_by_task_id: Option<TaskId>,
        pending_count: u64,
        ready_count: u64,
        total_pending: std::time::Duration,
        last_seen: Instant,
    }

    struct FutureSpawnEdgeInfo {
        parent_future_id: FutureId,
        parent_resource: String,
        child_future_id: FutureId,
        child_resource: String,
        created_by_task_id: Option<TaskId>,
        created_at: Instant,
    }

    struct FuturePollEdgeInfo {
        resource: String,
        poll_count: u64,
        total_poll: std::time::Duration,
        last_poll_at: Instant,
    }

    struct FutureResumeEdgeInfo {
        future_resource: String,
        resume_count: u64,
        last_resume_at: Instant,
    }

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
            created_at: Instant::now(),
        });
    }

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
                last_poll_at: Instant::now(),
            });
        entry.poll_count += 1;
        entry.total_poll += poll_duration;
        entry.last_poll_at = Instant::now();
    }

    fn record_future_resume_edge(
        future_id: FutureId,
        future_resource: &str,
        target_task_id: TaskId,
    ) {
        let mut registry = FUTURE_RESUME_EDGE_REGISTRY.lock().unwrap();
        let Some(edges) = registry.as_mut() else {
            return;
        };
        let entry = edges
            .entry((future_id, target_task_id))
            .or_insert(FutureResumeEdgeInfo {
                future_resource: future_resource.to_string(),
                resume_count: 0,
                last_resume_at: Instant::now(),
            });
        entry.resume_count += 1;
        entry.last_resume_at = Instant::now();
    }

    fn record_wake(source_task_id: Option<TaskId>, target_task_id: TaskId) {
        let mut registry = WAKE_REGISTRY.lock().unwrap();
        let Some(edges) = registry.as_mut() else {
            return;
        };
        let entry = edges
            .entry((source_task_id, target_task_id))
            .or_insert(WakeEdgeInfo {
                wake_count: 0,
                last_wake_at: Instant::now(),
            });
        entry.wake_count += 1;
        entry.last_wake_at = Instant::now();
    }

    fn record_future_wake_edge(source_task_id: Option<TaskId>, future_id: FutureId) {
        let mut registry = FUTURE_WAKE_EDGE_REGISTRY.lock().unwrap();
        let Some(edges) = registry.as_mut() else {
            return;
        };
        let entry = edges
            .entry((source_task_id, future_id))
            .or_insert(FutureWakeEdgeInfo {
                wake_count: 0,
                last_wake_at: Instant::now(),
            });
        entry.wake_count += 1;
        entry.last_wake_at = Instant::now();
    }

    fn register_future(future_id: FutureId, resource: String, created_by_task_id: Option<TaskId>) {
        let mut registry = FUTURE_WAIT_REGISTRY.lock().unwrap();
        let Some(waits) = registry.as_mut() else {
            return;
        };
        waits.entry(future_id).or_insert(FutureWaitInfo {
            resource,
            created_at: Instant::now(),
            created_by_task_id,
            last_polled_by_task_id: created_by_task_id,
            pending_count: 0,
            ready_count: 0,
            total_pending: std::time::Duration::from_secs(0),
            last_seen: Instant::now(),
        });
    }

    fn future_resource_by_id(future_id: FutureId) -> Option<String> {
        let registry = FUTURE_WAIT_REGISTRY.lock().unwrap();
        let waits = registry.as_ref()?;
        waits.get(&future_id).map(|w| w.resource.clone())
    }

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
        entry.last_seen = Instant::now();
    }

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
        entry.last_seen = Instant::now();
    }

    struct InstrumentedWake {
        inner: std::task::Waker,
        target_task_id: TaskId,
    }

    impl Wake for InstrumentedWake {
        fn wake(self: Arc<Self>) {
            let source_task_id = current_task_id();
            record_wake(source_task_id, self.target_task_id);
            self.inner.wake_by_ref();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            let source_task_id = current_task_id();
            record_wake(source_task_id, self.target_task_id);
            self.inner.wake_by_ref();
        }
    }

    struct PeepableWake {
        inner: std::task::Waker,
        future_id: FutureId,
        future_resource: String,
        target_task_id: Option<TaskId>,
    }

    impl Wake for PeepableWake {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            let source_task_id = current_task_id();
            record_future_wake_edge(source_task_id, self.future_id);
            if let Some(target) = self.target_task_id {
                record_future_resume_edge(self.future_id, &self.future_resource, target);
            }
            self.inner.wake_by_ref();
        }
    }

    pub struct PeepableFuture<F> {
        future_id: FutureId,
        resource: String,
        inner: F,
        pending_since: Option<Instant>,
    }

    impl<F> Future for PeepableFuture<F>
    where
        F: Future,
    {
        type Output = F::Output;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            // SAFETY: we never move fields out of `self` after pinning.
            #[allow(unsafe_code)]
            let this = unsafe { self.get_unchecked_mut() };
            // SAFETY: `inner` is pinned together with `self`.
            #[allow(unsafe_code)]
            let inner = unsafe { Pin::new_unchecked(&mut this.inner) };
            let task_id = current_task_id();
            let poll_start = Instant::now();

            // Set thread-local so child peepable() calls can record spawn edges.
            let prev = CURRENT_POLLING_FUTURE.with(|c| {
                let prev = c.get();
                c.set(Some(this.future_id));
                prev
            });

            let peepable_waker = std::task::Waker::from(Arc::new(PeepableWake {
                inner: cx.waker().clone(),
                future_id: this.future_id,
                future_resource: this.resource.clone(),
                target_task_id: task_id,
            }));
            let mut peepable_cx = Context::from_waker(&peepable_waker);
            let result = inner.poll(&mut peepable_cx);

            // Restore previous thread-local.
            CURRENT_POLLING_FUTURE.with(|c| c.set(prev));

            let poll_duration = poll_start.elapsed();

            // Record poll edge.
            if let Some(tid) = task_id {
                record_future_poll_edge(tid, this.future_id, &this.resource, poll_duration);
            }

            match result {
                Poll::Pending => {
                    if this.pending_since.is_none() {
                        this.pending_since = Some(Instant::now());
                    }
                    record_future_pending(this.future_id, task_id);
                    Poll::Pending
                }
                Poll::Ready(value) => {
                    let pending_duration = this
                        .pending_since
                        .take()
                        .map(|t| t.elapsed())
                        .unwrap_or_default();
                    record_future_ready(this.future_id, task_id, pending_duration);
                    Poll::Ready(value)
                }
            }
        }
    }

    pub fn peepable<F>(future: F, resource: impl Into<String>) -> PeepableFuture<F>
    where
        F: Future,
    {
        let future_id = NEXT_FUTURE_ID.fetch_add(1, Ordering::Relaxed);
        let resource = resource.into();
        let task_id = current_task_id();
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

    impl TaskInfo {
        fn snapshot(&self, now: Instant, registry: &[Arc<TaskInfo>]) -> TaskSnapshot {
            let state = self.state.lock().unwrap();
            let age = now.duration_since(self.spawned_at);

            let parent_task_name = self.parent_id.and_then(|pid| {
                registry
                    .iter()
                    .find(|t| t.id == pid)
                    .map(|t| t.name.clone())
            });

            TaskSnapshot {
                id: self.id,
                name: self.name.clone(),
                state: state.state,
                spawned_at_secs: self.spawned_at.elapsed().as_secs_f64() - age.as_secs_f64(),
                age_secs: age.as_secs_f64(),
                spawn_backtrace: self.spawn_backtrace.clone(),
                poll_events: state
                    .poll_events
                    .iter()
                    .map(|e| PollEvent {
                        started_at_secs: e.started_at.duration_since(self.spawned_at).as_secs_f64(),
                        duration_secs: e.duration.map(|d| d.as_secs_f64()),
                        result: e.result,
                        backtrace: e.backtrace.clone(),
                    })
                    .collect(),
                parent_task_id: self.parent_id,
                parent_task_name,
            }
        }

        fn record_poll_start(&self, backtrace: Option<String>) {
            let mut state = self.state.lock().unwrap();
            state.state = TaskState::Polling;

            if state.poll_events.len() >= 16 {
                state.poll_events.remove(0);
            }

            state.poll_events.push(PollEventInternal {
                started_at: Instant::now(),
                duration: None,
                result: PollResult::Pending,
                backtrace,
            });
        }

        fn record_poll_end(&self, result: PollResult) {
            let mut state = self.state.lock().unwrap();

            if let Some(last) = state.poll_events.last_mut() {
                last.duration = Some(last.started_at.elapsed());
                last.result = result;
            }

            state.state = match result {
                PollResult::Ready => TaskState::Completed,
                PollResult::Pending => TaskState::Pending,
            };
        }
    }

    struct TrackedFuture<F> {
        inner: F,
        task_info: Arc<TaskInfo>,
    }

    impl<F: Future> Future for TrackedFuture<F> {
        type Output = F::Output;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            // SAFETY: projecting through TrackedFuture to inner future.
            // We never move out of inner.
            let this = unsafe { self.get_unchecked_mut() };
            let inner = unsafe { Pin::new_unchecked(&mut this.inner) };

            let backtrace = Some(format!("{:?}", Backtrace::new()));
            this.task_info.record_poll_start(backtrace);

            let task_id = this.task_info.id;
            let instrumented_waker = std::task::Waker::from(Arc::new(InstrumentedWake {
                inner: cx.waker().clone(),
                target_task_id: task_id,
            }));
            let mut instrumented_cx = Context::from_waker(&instrumented_waker);
            let result = CURRENT_TASK_ID.sync_scope(task_id, || inner.poll(&mut instrumented_cx));

            let poll_result = match result {
                Poll::Ready(_) => PollResult::Ready,
                Poll::Pending => PollResult::Pending,
            };

            this.task_info.record_poll_end(poll_result);

            result
        }
    }

    pub fn init_task_tracking() {
        *TASK_REGISTRY.lock().unwrap() = Some(Vec::new());
        *WAKE_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
        *FUTURE_WAKE_EDGE_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
        *FUTURE_WAIT_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
        *FUTURE_SPAWN_EDGE_REGISTRY.lock().unwrap() = Some(Vec::new());
        *FUTURE_POLL_EDGE_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
        *FUTURE_RESUME_EDGE_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
    }

    #[inline]
    fn decorate_task_name(name: String, caller: &'static std::panic::Location<'static>) -> String {
        // Keep names concise but attributable: "<name> @ file.rs:line"
        let file = std::path::Path::new(caller.file())
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(caller.file());
        format!("{name} @ {file}:{}", caller.line())
    }

    #[track_caller]
    pub fn spawn_tracked<F>(
        name: impl Into<String>,
        future: F,
    ) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let name = decorate_task_name(name.into(), std::panic::Location::caller());
        let task_id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
        let parent_id = current_task_id();
        let spawn_backtrace = format!("{:?}", Backtrace::new());

        let task_info = Arc::new(TaskInfo {
            id: task_id,
            name,
            parent_id,
            spawned_at: Instant::now(),
            spawn_backtrace,
            state: Mutex::new(TaskInfoState {
                state: TaskState::Pending,
                poll_events: Vec::new(),
            }),
        });

        if let Some(registry) = TASK_REGISTRY.lock().unwrap().as_mut() {
            // Prune completed tasks older than 30s to keep the registry bounded.
            let cutoff = Instant::now() - std::time::Duration::from_secs(30);
            registry.retain(|task| {
                let state = task.state.lock().unwrap();
                state.state != TaskState::Completed || task.spawned_at > cutoff
            });
            registry.push(Arc::clone(&task_info));
        }

        let tracked = TrackedFuture {
            inner: future,
            task_info,
        };

        tokio::spawn(tracked)
    }

    pub fn snapshot_all_tasks() -> Vec<TaskSnapshot> {
        let now = Instant::now();
        let registry = TASK_REGISTRY.lock().unwrap();
        let Some(tasks) = registry.as_ref() else {
            return Vec::new();
        };

        let cutoff = now - std::time::Duration::from_secs(30);

        tasks
            .iter()
            .filter(|task| {
                let state = task.state.lock().unwrap();
                state.state != TaskState::Completed || task.spawned_at > cutoff
            })
            .map(|task| task.snapshot(now, tasks))
            .collect()
    }

    pub fn snapshot_wake_edges() -> Vec<WakeEdgeSnapshot> {
        let now = Instant::now();

        let task_lookup: std::collections::HashMap<TaskId, String> = {
            let registry = TASK_REGISTRY.lock().unwrap();
            let Some(tasks) = registry.as_ref() else {
                return Vec::new();
            };
            tasks
                .iter()
                .map(|task| (task.id, task.name.clone()))
                .collect()
        };

        let registry = WAKE_REGISTRY.lock().unwrap();
        let Some(edges) = registry.as_ref() else {
            return Vec::new();
        };

        let mut out: Vec<WakeEdgeSnapshot> = edges
            .iter()
            .map(
                |((source_task_id, target_task_id), edge)| WakeEdgeSnapshot {
                    source_task_id: *source_task_id,
                    source_task_name: source_task_id.and_then(|id| task_lookup.get(&id).cloned()),
                    target_task_id: *target_task_id,
                    target_task_name: task_lookup.get(target_task_id).cloned(),
                    wake_count: edge.wake_count,
                    last_wake_age_secs: now.duration_since(edge.last_wake_at).as_secs_f64(),
                },
            )
            .collect();
        out.sort_by(|a, b| b.wake_count.cmp(&a.wake_count));
        out
    }

    pub fn snapshot_future_wake_edges() -> Vec<FutureWakeEdgeSnapshot> {
        let now = Instant::now();

        let task_lookup: std::collections::HashMap<TaskId, String> = {
            let registry = TASK_REGISTRY.lock().unwrap();
            let Some(tasks) = registry.as_ref() else {
                return Vec::new();
            };
            tasks
                .iter()
                .map(|task| (task.id, task.name.clone()))
                .collect()
        };

        let future_meta: std::collections::HashMap<FutureId, (String, Option<TaskId>)> = {
            let registry = FUTURE_WAIT_REGISTRY.lock().unwrap();
            let Some(waits) = registry.as_ref() else {
                return Vec::new();
            };
            waits
                .iter()
                .map(|(future_id, wait)| {
                    (
                        *future_id,
                        (wait.resource.clone(), wait.last_polled_by_task_id.or(wait.created_by_task_id)),
                    )
                })
                .collect()
        };

        let registry = FUTURE_WAKE_EDGE_REGISTRY.lock().unwrap();
        let Some(edges) = registry.as_ref() else {
            return Vec::new();
        };

        let mut out: Vec<FutureWakeEdgeSnapshot> = edges
            .iter()
            .map(|((source_task_id, future_id), edge)| {
                let (future_resource, target_task_id) = future_meta
                    .get(future_id)
                    .cloned()
                    .unwrap_or_else(|| ("future".to_string(), None));
                FutureWakeEdgeSnapshot {
                    source_task_id: *source_task_id,
                    source_task_name: source_task_id.and_then(|id| task_lookup.get(&id).cloned()),
                    future_id: *future_id,
                    future_resource,
                    target_task_id,
                    target_task_name: target_task_id.and_then(|id| task_lookup.get(&id).cloned()),
                    wake_count: edge.wake_count,
                    last_wake_age_secs: now.duration_since(edge.last_wake_at).as_secs_f64(),
                }
            })
            .collect();
        out.sort_by(|a, b| b.wake_count.cmp(&a.wake_count));
        out
    }

    pub fn snapshot_future_waits() -> Vec<FutureWaitSnapshot> {
        let now = Instant::now();
        let task_lookup: std::collections::HashMap<TaskId, String> = {
            let registry = TASK_REGISTRY.lock().unwrap();
            let Some(tasks) = registry.as_ref() else {
                return Vec::new();
            };
            tasks.iter().map(|task| (task.id, task.name.clone())).collect()
        };

        let registry = FUTURE_WAIT_REGISTRY.lock().unwrap();
        let Some(waits) = registry.as_ref() else {
            return Vec::new();
        };

        let mut out: Vec<FutureWaitSnapshot> = waits
            .iter()
            .map(|(future_id, wait)| FutureWaitSnapshot {
                future_id: *future_id,
                task_id: wait.last_polled_by_task_id.or(wait.created_by_task_id).unwrap_or(0),
                task_name: wait
                    .last_polled_by_task_id
                    .or(wait.created_by_task_id)
                    .and_then(|id| task_lookup.get(&id).cloned()),
                resource: wait.resource.clone(),
                created_by_task_id: wait.created_by_task_id,
                created_by_task_name: wait
                    .created_by_task_id
                    .and_then(|id| task_lookup.get(&id).cloned()),
                created_age_secs: now.duration_since(wait.created_at).as_secs_f64(),
                last_polled_by_task_id: wait.last_polled_by_task_id,
                last_polled_by_task_name: wait
                    .last_polled_by_task_id
                    .and_then(|id| task_lookup.get(&id).cloned()),
                pending_count: wait.pending_count,
                ready_count: wait.ready_count,
                total_pending_secs: wait.total_pending.as_secs_f64(),
                last_seen_age_secs: now.duration_since(wait.last_seen).as_secs_f64(),
            })
            .collect();
        out.sort_by(|a, b| {
            b.total_pending_secs
                .partial_cmp(&a.total_pending_secs)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }

    pub fn snapshot_future_spawn_edges() -> Vec<FutureSpawnEdgeSnapshot> {
        let now = Instant::now();
        let task_lookup: std::collections::HashMap<TaskId, String> = {
            let registry = TASK_REGISTRY.lock().unwrap();
            let Some(tasks) = registry.as_ref() else {
                return Vec::new();
            };
            tasks.iter().map(|task| (task.id, task.name.clone())).collect()
        };

        let registry = FUTURE_SPAWN_EDGE_REGISTRY.lock().unwrap();
        let Some(edges) = registry.as_ref() else {
            return Vec::new();
        };

        edges
            .iter()
            .map(|edge| FutureSpawnEdgeSnapshot {
                parent_future_id: edge.parent_future_id,
                parent_resource: edge.parent_resource.clone(),
                child_future_id: edge.child_future_id,
                child_resource: edge.child_resource.clone(),
                created_by_task_id: edge.created_by_task_id,
                created_by_task_name: edge
                    .created_by_task_id
                    .and_then(|id| task_lookup.get(&id).cloned()),
                created_age_secs: now.duration_since(edge.created_at).as_secs_f64(),
            })
            .collect()
    }

    pub fn snapshot_future_poll_edges() -> Vec<FuturePollEdgeSnapshot> {
        let now = Instant::now();
        let task_lookup: std::collections::HashMap<TaskId, String> = {
            let registry = TASK_REGISTRY.lock().unwrap();
            let Some(tasks) = registry.as_ref() else {
                return Vec::new();
            };
            tasks.iter().map(|task| (task.id, task.name.clone())).collect()
        };

        let registry = FUTURE_POLL_EDGE_REGISTRY.lock().unwrap();
        let Some(edges) = registry.as_ref() else {
            return Vec::new();
        };

        edges
            .iter()
            .map(|((task_id, future_id), edge)| FuturePollEdgeSnapshot {
                task_id: *task_id,
                task_name: task_lookup.get(task_id).cloned(),
                future_id: *future_id,
                future_resource: edge.resource.clone(),
                poll_count: edge.poll_count,
                total_poll_secs: edge.total_poll.as_secs_f64(),
                last_poll_age_secs: now.duration_since(edge.last_poll_at).as_secs_f64(),
            })
            .collect()
    }

    pub fn snapshot_future_resume_edges() -> Vec<FutureResumeEdgeSnapshot> {
        let now = Instant::now();
        let task_lookup: std::collections::HashMap<TaskId, String> = {
            let registry = TASK_REGISTRY.lock().unwrap();
            let Some(tasks) = registry.as_ref() else {
                return Vec::new();
            };
            tasks.iter().map(|task| (task.id, task.name.clone())).collect()
        };

        let registry = FUTURE_RESUME_EDGE_REGISTRY.lock().unwrap();
        let Some(edges) = registry.as_ref() else {
            return Vec::new();
        };

        edges
            .iter()
            .map(|((future_id, target_task_id), edge)| FutureResumeEdgeSnapshot {
                future_id: *future_id,
                future_resource: edge.future_resource.clone(),
                target_task_id: *target_task_id,
                target_task_name: task_lookup.get(target_task_id).cloned(),
                resume_count: edge.resume_count,
                last_resume_age_secs: now.duration_since(edge.last_resume_at).as_secs_f64(),
            })
            .collect()
    }

    pub fn cleanup_completed_tasks() {
        let now = Instant::now();
        let cutoff = now - std::time::Duration::from_secs(30);

        if let Some(registry) = TASK_REGISTRY.lock().unwrap().as_mut() {
            registry.retain(|task| {
                let state = task.state.lock().unwrap();
                state.state != TaskState::Completed || task.spawned_at > cutoff
            });
        }

        let live_ids: std::collections::HashSet<TaskId> = {
            let registry = TASK_REGISTRY.lock().unwrap();
            let Some(tasks) = registry.as_ref() else {
                return;
            };
            tasks.iter().map(|task| task.id).collect()
        };
        if let Some(edges) = WAKE_REGISTRY.lock().unwrap().as_mut() {
            edges.retain(|(_, target), edge| {
                let recent = edge.last_wake_at > cutoff;
                live_ids.contains(target) || recent
            });
        }
        let live_future_ids: std::collections::HashSet<FutureId> = {
            let waits = FUTURE_WAIT_REGISTRY.lock().unwrap();
            waits
                .as_ref()
                .map(|waits| waits.keys().copied().collect())
                .unwrap_or_default()
        };
        if let Some(edges) = FUTURE_WAKE_EDGE_REGISTRY.lock().unwrap().as_mut() {
            edges.retain(|(_, future_id), edge| {
                let recent = edge.last_wake_at > cutoff;
                recent || live_future_ids.contains(future_id)
            });
        }
        if let Some(waits) = FUTURE_WAIT_REGISTRY.lock().unwrap().as_mut() {
            waits.retain(|_, wait| {
                let recent = wait.last_seen > cutoff;
                let creator_live = wait
                    .created_by_task_id
                    .is_some_and(|task_id| live_ids.contains(&task_id));
                let last_polled_live = wait
                    .last_polled_by_task_id
                    .is_some_and(|task_id| live_ids.contains(&task_id));
                creator_live || last_polled_live || recent
            });
        }
        // Cleanup new registries
        if let Some(edges) = FUTURE_SPAWN_EDGE_REGISTRY.lock().unwrap().as_mut() {
            edges.retain(|edge| {
                let recent = edge.created_at > cutoff;
                recent || live_future_ids.contains(&edge.child_future_id)
            });
        }
        if let Some(edges) = FUTURE_POLL_EDGE_REGISTRY.lock().unwrap().as_mut() {
            edges.retain(|(task_id, future_id), edge| {
                let recent = edge.last_poll_at > cutoff;
                recent || (live_ids.contains(task_id) && live_future_ids.contains(future_id))
            });
        }
        if let Some(edges) = FUTURE_RESUME_EDGE_REGISTRY.lock().unwrap().as_mut() {
            edges.retain(|(future_id, target_task_id), edge| {
                let recent = edge.last_resume_at > cutoff;
                recent
                    || (live_future_ids.contains(future_id) && live_ids.contains(target_task_id))
            });
        }
    }
}

// ── Public API (delegates to imp) ────────────────────────────────

/// Initialize the task tracking registry. No-op without `diagnostics`.
pub fn init_task_tracking() {
    imp::init_task_tracking();
}

/// Returns the current peeps task ID, if running inside a tracked task.
/// Returns `None` outside of a tracked task or without `diagnostics`.
pub fn current_task_id() -> Option<TaskId> {
    imp::current_task_id()
}

/// Look up a task's name by ID. Returns `None` if not found or without `diagnostics`.
pub fn task_name(id: TaskId) -> Option<String> {
    imp::task_name(id)
}

/// Spawn a tracked task with the given name.
///
/// With `diagnostics`: captures spawn backtrace and records poll events.
/// Without `diagnostics`: zero-cost wrapper around `tokio::spawn`.
#[track_caller]
pub fn spawn_tracked<F>(name: impl Into<String>, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    imp::spawn_tracked(name, future)
}

/// Collect snapshots of all tracked tasks. Empty without `diagnostics`.
pub fn snapshot_all_tasks() -> Vec<TaskSnapshot> {
    imp::snapshot_all_tasks()
}

/// Collect snapshots of wake/dependency edges between tasks.
pub fn snapshot_wake_edges() -> Vec<WakeEdgeSnapshot> {
    imp::snapshot_wake_edges()
}

/// Collect snapshots of wake/dependency edges from tasks to instrumented futures.
pub fn snapshot_future_wake_edges() -> Vec<FutureWakeEdgeSnapshot> {
    imp::snapshot_future_wake_edges()
}

/// Collect snapshots of annotated future wait states.
pub fn snapshot_future_waits() -> Vec<FutureWaitSnapshot> {
    imp::snapshot_future_waits()
}

/// Collect snapshots of future-to-future spawn/composition edges.
pub fn snapshot_future_spawn_edges() -> Vec<FutureSpawnEdgeSnapshot> {
    imp::snapshot_future_spawn_edges()
}

/// Collect snapshots of task-polls-future edges.
pub fn snapshot_future_poll_edges() -> Vec<FuturePollEdgeSnapshot> {
    imp::snapshot_future_poll_edges()
}

/// Collect snapshots of future-resumes-task edges.
pub fn snapshot_future_resume_edges() -> Vec<FutureResumeEdgeSnapshot> {
    imp::snapshot_future_resume_edges()
}

/// Wrapper future produced by [`peepable`] or [`PeepableFutureExt::peepable`].
pub struct PeepableFuture<F> {
    inner: imp::PeepableFuture<F>,
}

impl<F> Future for PeepableFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        #[allow(unsafe_code)]
        unsafe {
            let this = self.get_unchecked_mut();
            std::pin::Pin::new_unchecked(&mut this.inner).poll(cx)
        }
    }
}

/// Mark a future as an instrumented wait on a named resource.
pub fn peepable<F>(future: F, resource: impl Into<String>) -> PeepableFuture<F>
where
    F: Future,
{
    PeepableFuture {
        inner: imp::peepable(future, resource),
    }
}

pub trait PeepableFutureExt: Future + Sized {
    fn peepable(self, resource: impl Into<String>) -> PeepableFuture<Self> {
        peepable(self, resource)
    }
}

impl<F: Future> PeepableFutureExt for F {}

/// Remove completed tasks from the registry. No-op without `diagnostics`.
pub fn cleanup_completed_tasks() {
    imp::cleanup_completed_tasks()
}
