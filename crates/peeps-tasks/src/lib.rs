//! Task instrumentation for Tokio spawned tasks.
//!
//! When the `diagnostics` feature is enabled, wraps spawned tasks to capture
//! timing, poll events, and backtraces. When disabled, `spawn_tracked` is
//! a zero-cost wrapper around `tokio::spawn`.

use std::future::Future;

pub use peeps_types::{PollEvent, PollResult, TaskId, TaskSnapshot, TaskState};

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
    pub fn cleanup_completed_tasks() {}
}

// ── Tracing implementation (diagnostics enabled) ─────────────────

#[cfg(feature = "diagnostics")]
mod imp {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
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

    impl TaskInfo {
        fn snapshot(&self, now: Instant, registry: &[Arc<TaskInfo>]) -> TaskSnapshot {
            let state = self.state.lock().unwrap();
            let age = now.duration_since(self.spawned_at);

            let parent_task_name = self.parent_id.and_then(|pid| {
                registry.iter().find(|t| t.id == pid).map(|t| t.name.clone())
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
            let result = CURRENT_TASK_ID.sync_scope(task_id, || inner.poll(cx));

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
    }

    pub fn spawn_tracked<F>(
        name: impl Into<String>,
        future: F,
    ) -> tokio::task::JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let name = name.into();
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

    pub fn cleanup_completed_tasks() {
        let now = Instant::now();
        let cutoff = now - std::time::Duration::from_secs(30);

        if let Some(registry) = TASK_REGISTRY.lock().unwrap().as_mut() {
            registry.retain(|task| {
                let state = task.state.lock().unwrap();
                state.state != TaskState::Completed || task.spawned_at > cutoff
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

/// Remove completed tasks from the registry. No-op without `diagnostics`.
pub fn cleanup_completed_tasks() {
    imp::cleanup_completed_tasks()
}
