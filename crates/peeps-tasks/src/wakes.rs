#[cfg(feature = "diagnostics")]
use peeps_types::{FutureId, TaskId};

// ── Diagnostics statics ─────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) static WAKE_REGISTRY: std::sync::Mutex<
    Option<std::collections::HashMap<(Option<TaskId>, TaskId), WakeEdgeInfo>>,
> = std::sync::Mutex::new(None);

#[cfg(feature = "diagnostics")]
pub(crate) static FUTURE_WAKE_EDGE_REGISTRY: std::sync::Mutex<
    Option<std::collections::HashMap<(Option<TaskId>, FutureId), FutureWakeEdgeInfo>>,
> = std::sync::Mutex::new(None);

#[cfg(feature = "diagnostics")]
pub(crate) static FUTURE_RESUME_EDGE_REGISTRY: std::sync::Mutex<
    Option<std::collections::HashMap<(FutureId, TaskId), FutureResumeEdgeInfo>>,
> = std::sync::Mutex::new(None);

// ── Types (diagnostics only) ────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) struct WakeEdgeInfo {
    pub(crate) wake_count: u64,
    pub(crate) last_wake_at: std::time::Instant,
}

#[cfg(feature = "diagnostics")]
pub(crate) struct FutureWakeEdgeInfo {
    pub(crate) wake_count: u64,
    pub(crate) last_wake_at: std::time::Instant,
}

#[cfg(feature = "diagnostics")]
pub(crate) struct FutureResumeEdgeInfo {
    pub(crate) future_resource: String,
    pub(crate) resume_count: u64,
    pub(crate) last_resume_at: std::time::Instant,
}

// ── InstrumentedWake ────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) struct InstrumentedWake {
    pub(crate) inner: std::task::Waker,
    pub(crate) target_task_id: TaskId,
}

#[cfg(feature = "diagnostics")]
impl std::task::Wake for InstrumentedWake {
    fn wake(self: std::sync::Arc<Self>) {
        let source_task_id = crate::tasks::current_task_id();
        record_wake(source_task_id, self.target_task_id);
        self.inner.wake_by_ref();
    }

    fn wake_by_ref(self: &std::sync::Arc<Self>) {
        let source_task_id = crate::tasks::current_task_id();
        record_wake(source_task_id, self.target_task_id);
        self.inner.wake_by_ref();
    }
}

// ── PeepableWake ────────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) struct PeepableWake {
    pub(crate) inner: std::task::Waker,
    pub(crate) future_id: FutureId,
    pub(crate) future_resource: String,
    pub(crate) target_task_id: Option<TaskId>,
}

#[cfg(feature = "diagnostics")]
impl std::task::Wake for PeepableWake {
    fn wake(self: std::sync::Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &std::sync::Arc<Self>) {
        let source_task_id = crate::tasks::current_task_id();
        record_future_wake_edge(source_task_id, self.future_id);
        if let Some(target) = self.target_task_id {
            record_future_resume_edge(self.future_id, &self.future_resource, target);
        }
        self.inner.wake_by_ref();
    }
}

// ── Recording functions ─────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) fn record_wake(source_task_id: Option<TaskId>, target_task_id: TaskId) {
    let mut registry = WAKE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_mut() else {
        return;
    };
    let entry = edges
        .entry((source_task_id, target_task_id))
        .or_insert(WakeEdgeInfo {
            wake_count: 0,
            last_wake_at: std::time::Instant::now(),
        });
    entry.wake_count += 1;
    entry.last_wake_at = std::time::Instant::now();
}

#[cfg(feature = "diagnostics")]
pub(crate) fn record_future_wake_edge(source_task_id: Option<TaskId>, future_id: FutureId) {
    let mut registry = FUTURE_WAKE_EDGE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_mut() else {
        return;
    };
    let entry = edges
        .entry((source_task_id, future_id))
        .or_insert(FutureWakeEdgeInfo {
            wake_count: 0,
            last_wake_at: std::time::Instant::now(),
        });
    entry.wake_count += 1;
    entry.last_wake_at = std::time::Instant::now();
}

#[cfg(feature = "diagnostics")]
pub(crate) fn record_future_resume_edge(
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
            last_resume_at: std::time::Instant::now(),
        });
    entry.resume_count += 1;
    entry.last_resume_at = std::time::Instant::now();
}

// ── Init ────────────────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) fn init() {
    *WAKE_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
    *FUTURE_WAKE_EDGE_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
    *FUTURE_RESUME_EDGE_REGISTRY.lock().unwrap() = Some(std::collections::HashMap::new());
}

#[cfg(not(feature = "diagnostics"))]
pub(crate) fn init() {}
