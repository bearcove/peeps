use std::future::Future;

use peeps_types::TaskId;

// ── Diagnostics statics ─────────────────────────────────

#[cfg(feature = "diagnostics")]
tokio::task_local! {
    pub(crate) static CURRENT_TASK_ID: TaskId;
}

#[cfg(feature = "diagnostics")]
pub(crate) static NEXT_TASK_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

#[cfg(feature = "diagnostics")]
pub(crate) static TASK_REGISTRY: std::sync::Mutex<Option<Vec<std::sync::Arc<TaskInfo>>>> =
    std::sync::Mutex::new(None);

// ── Types (diagnostics only) ────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) struct TaskInfo {
    pub(crate) id: TaskId,
    pub(crate) name: String,
    pub(crate) parent_id: Option<TaskId>,
    pub(crate) spawned_at: std::time::Instant,
    pub(crate) spawn_backtrace: String,
    pub(crate) state: std::sync::Mutex<TaskInfoState>,
}

#[cfg(feature = "diagnostics")]
pub(crate) struct TaskInfoState {
    pub(crate) state: TaskState,
    pub(crate) poll_events: Vec<PollEventInternal>,
}

#[cfg(feature = "diagnostics")]
pub(crate) struct PollEventInternal {
    pub(crate) started_at: std::time::Instant,
    pub(crate) duration: Option<std::time::Duration>,
    pub(crate) result: PollResult,
    pub(crate) backtrace: Option<String>,
}

#[cfg(feature = "diagnostics")]
impl TaskInfo {
    pub(crate) fn snapshot(
        &self,
        now: std::time::Instant,
        registry: &[std::sync::Arc<TaskInfo>],
    ) -> TaskSnapshot {
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
                    started_at_secs: e
                        .started_at
                        .duration_since(self.spawned_at)
                        .as_secs_f64(),
                    duration_secs: e.duration.map(|d| d.as_secs_f64()),
                    result: e.result,
                    backtrace: e.backtrace.clone(),
                })
                .collect(),
            parent_task_id: self.parent_id,
            parent_task_name,
        }
    }

    pub(crate) fn record_poll_start(&self, backtrace: Option<String>) {
        let mut state = self.state.lock().unwrap();
        state.state = TaskState::Polling;

        if state.poll_events.len() >= 16 {
            state.poll_events.remove(0);
        }

        state.poll_events.push(PollEventInternal {
            started_at: std::time::Instant::now(),
            duration: None,
            result: PollResult::Pending,
            backtrace,
        });
    }

    pub(crate) fn record_poll_end(&self, result: PollResult) {
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

// ── TrackedFuture (diagnostics only) ────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) struct TrackedFuture<F> {
    pub(crate) inner: F,
    pub(crate) task_info: std::sync::Arc<TaskInfo>,
}

#[cfg(feature = "diagnostics")]
impl<F: Future> Future for TrackedFuture<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: projecting through TrackedFuture to inner future.
        // We never move out of inner.
        #[allow(unsafe_code)]
        let this = unsafe { self.get_unchecked_mut() };
        #[allow(unsafe_code)]
        let inner = unsafe { std::pin::Pin::new_unchecked(&mut this.inner) };

        let backtrace = Some(format!("{:?}", backtrace::Backtrace::new()));
        this.task_info.record_poll_start(backtrace);

        let task_id = this.task_info.id;
        let instrumented_waker =
            std::task::Waker::from(std::sync::Arc::new(crate::wakes::InstrumentedWake {
                inner: cx.waker().clone(),
                target_task_id: task_id,
            }));
        let mut instrumented_cx = std::task::Context::from_waker(&instrumented_waker);
        let result =
            CURRENT_TASK_ID.sync_scope(task_id, || inner.poll(&mut instrumented_cx));

        let poll_result = match result {
            std::task::Poll::Ready(_) => PollResult::Ready,
            std::task::Poll::Pending => PollResult::Pending,
        };

        this.task_info.record_poll_end(poll_result);

        result
    }
}

// ── Init ────────────────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) fn init() {
    *TASK_REGISTRY.lock().unwrap() = Some(Vec::new());
}

#[cfg(not(feature = "diagnostics"))]
pub(crate) fn init() {}

// ── Public API ──────────────────────────────────────────

pub fn current_task_id() -> Option<TaskId> {
    #[cfg(feature = "diagnostics")]
    {
        CURRENT_TASK_ID.try_with(|id| *id).ok()
    }
    #[cfg(not(feature = "diagnostics"))]
    {
        None
    }
}

pub fn task_name(id: TaskId) -> Option<String> {
    #[cfg(feature = "diagnostics")]
    {
        let registry = TASK_REGISTRY.lock().unwrap();
        let tasks = registry.as_ref()?;
        tasks.iter().find(|t| t.id == id).map(|t| t.name.clone())
    }
    #[cfg(not(feature = "diagnostics"))]
    {
        let _ = id;
        None
    }
}

#[cfg(feature = "diagnostics")]
#[inline]
fn decorate_task_name(name: String, caller: &'static std::panic::Location<'static>) -> String {
    let file = std::path::Path::new(caller.file())
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(caller.file());
    format!("{name} @ {file}:{}", caller.line())
}

#[cfg(feature = "diagnostics")]
#[track_caller]
pub fn spawn_tracked<F>(
    name: impl Into<String>,
    future: F,
) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Instant;

    let name = decorate_task_name(name.into(), std::panic::Location::caller());
    let task_id = NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed);
    let parent_id = current_task_id();
    let spawn_backtrace = format!("{:?}", backtrace::Backtrace::new());

    let task_info = Arc::new(TaskInfo {
        id: task_id,
        name,
        parent_id,
        spawned_at: Instant::now(),
        spawn_backtrace,
        state: std::sync::Mutex::new(TaskInfoState {
            state: TaskState::Pending,
            poll_events: Vec::new(),
        }),
    });

    if let Some(registry) = TASK_REGISTRY.lock().unwrap().as_mut() {
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

#[cfg(not(feature = "diagnostics"))]
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
