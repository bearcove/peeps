//! Task instrumentation for Tokio spawned tasks.
//!
//! When the `diagnostics` feature is enabled, wraps spawned tasks to capture
//! timing, poll events, and backtraces. When disabled, `spawn_tracked` is
//! a zero-cost wrapper around `tokio::spawn`.

use std::future::Future;

mod futures;
mod snapshot;
mod tasks;
mod wakes;

pub use peeps_types::{
    FutureId, FuturePollEdgeSnapshot, FutureResumeEdgeSnapshot, FutureSpawnEdgeSnapshot,
    FutureWaitSnapshot, FutureWakeEdgeSnapshot, PollEvent, PollResult, TaskId, TaskSnapshot,
    TaskState, WakeEdgeSnapshot,
};

// ── Public API (delegates to modules) ────────────────────

/// Initialize the task tracking registry. No-op without `diagnostics`.
pub fn init_task_tracking() {
    tasks::init();
    wakes::init();
    futures::init();
}

/// Returns the current peeps task ID, if running inside a tracked task.
/// Returns `None` outside of a tracked task or without `diagnostics`.
pub fn current_task_id() -> Option<TaskId> {
    tasks::current_task_id()
}

/// Look up a task's name by ID. Returns `None` if not found or without `diagnostics`.
pub fn task_name(id: TaskId) -> Option<String> {
    tasks::task_name(id)
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
    tasks::spawn_tracked(name, future)
}

/// Collect snapshots of all tracked tasks. Empty without `diagnostics`.
pub fn snapshot_all_tasks() -> Vec<TaskSnapshot> {
    snapshot::snapshot_all_tasks()
}

/// Collect snapshots of wake/dependency edges between tasks.
pub fn snapshot_wake_edges() -> Vec<WakeEdgeSnapshot> {
    snapshot::snapshot_wake_edges()
}

/// Collect snapshots of wake/dependency edges from tasks to instrumented futures.
pub fn snapshot_future_wake_edges() -> Vec<FutureWakeEdgeSnapshot> {
    snapshot::snapshot_future_wake_edges()
}

/// Collect snapshots of annotated future wait states.
pub fn snapshot_future_waits() -> Vec<FutureWaitSnapshot> {
    snapshot::snapshot_future_waits()
}

/// Collect snapshots of future-to-future spawn/composition edges.
pub fn snapshot_future_spawn_edges() -> Vec<FutureSpawnEdgeSnapshot> {
    snapshot::snapshot_future_spawn_edges()
}

/// Collect snapshots of task-polls-future edges.
pub fn snapshot_future_poll_edges() -> Vec<FuturePollEdgeSnapshot> {
    snapshot::snapshot_future_poll_edges()
}

/// Collect snapshots of future-resumes-task edges.
pub fn snapshot_future_resume_edges() -> Vec<FutureResumeEdgeSnapshot> {
    snapshot::snapshot_future_resume_edges()
}

/// Wrapper future produced by [`peepable`] or [`PeepableFutureExt::peepable`].
pub struct PeepableFuture<F> {
    inner: futures::PeepableFuture<F>,
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
        inner: futures::peepable(future, resource),
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
    snapshot::cleanup_completed_tasks()
}
