use std::cell::RefCell;
use std::future::Future;

use moire_runtime::{
    capture_backtrace_id, instrument_future, register_current_task_scope, EntityHandle,
    FUTURE_CAUSAL_STACK,
};
use moire_types::EntityBody;

/// Instrumented equivalent of [`tokio::task::JoinSet`], used to track joined task sets.
pub struct JoinSet<T> {
    pub(super) inner: tokio::task::JoinSet<T>,
    pub(super) handle: EntityHandle,
}

// r[impl api.joinset]
impl<T> JoinSet<T>
where
    T: Send + 'static,
{
    /// Creates an instrumented join set with a caller-specified name.
    pub fn new() -> Self {
        let source = capture_backtrace_id();
        Self {
            inner: tokio::task::JoinSet::new(),
            handle: EntityHandle::new(
                "joinset",
                EntityBody::Future(moire_types::FutureEntity {}),
                source,
            ),
        }
    }

    /// Creates an instrumented join set equivalent to [`tokio::task::JoinSet::new`].
    pub fn named(name: impl Into<String>) -> Self {
        let source = capture_backtrace_id();
        let name = name.into();
        let handle = EntityHandle::new(
            format!("joinset.{name}"),
            EntityBody::Future(moire_types::FutureEntity {}),
            source,
        );
        Self {
            inner: tokio::task::JoinSet::new(),
            handle,
        }
    }

    /// Spawns a future into the set, matching [`tokio::task::JoinSet::spawn`].
    pub fn spawn<F>(&mut self, label: &'static str, future: F)
    where
        F: Future<Output = T> + Send + 'static,
    {
        let source = capture_backtrace_id();
        let joinset_handle = self.handle.clone();
        self.inner.spawn(
            FUTURE_CAUSAL_STACK.scope(RefCell::new(Vec::new()), async move {
                let _task_scope = register_current_task_scope(label, source);
                instrument_future(
                    label,
                    future,
                    source,
                    Some(joinset_handle.entity_ref()),
                    None,
                )
                .await
            }),
        );
    }

    /// Returns whether the set is empty, matching [`tokio::task::JoinSet::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the number of tasks still tracked, like [`tokio::task::JoinSet::len`].
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Aborts all in-flight tasks, equivalent to [`tokio::task::JoinSet::abort_all`].
    pub fn abort_all(&mut self) {
        self.inner.abort_all();
    }

    /// Waits for one task to complete, matching [`tokio::task::JoinSet::join_next`].
    pub fn join_next(
        &mut self,
    ) -> impl Future<Output = Option<Result<T, tokio::task::JoinError>>> + '_ {
        let source = capture_backtrace_id();
        let handle = self.handle.clone();
        let fut = self.inner.join_next();
        instrument_future(
            "joinset.join_next",
            fut,
            source,
            Some(handle.entity_ref()),
            None,
        )
    }
}
