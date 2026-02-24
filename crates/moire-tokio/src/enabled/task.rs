//! Instrumented task spawning and management, mirroring [`tokio::task`].
//!
//! This module mirrors the structure of `tokio::task` and can be used as a
//! drop-in replacement. Spawned tasks and join sets are registered as named
//! entities in the Moiré runtime graph, so the dashboard can show the full
//! dependency graph between tasks and what each one is currently waiting on.
//!
//! # Available items
//!
//! | Item | Tokio equivalent |
//! |---|---|
//! | [`JoinSet`] | [`tokio::task::JoinSet`] |
//! | [`JoinHandle`] | [`tokio::task::JoinHandle`] |
//! | [`spawn`] | [`tokio::task::spawn`] |
//! | [`spawn_blocking`] | [`tokio::task::spawn_blocking`] |
//! | [`FutureExt`] | *(moire extension)* |

pub mod join_handle;
pub mod joinset;

pub use self::join_handle::*;
pub use self::joinset::*;

use std::cell::RefCell;
use std::future::{Future, IntoFuture};

use moire_runtime::{
    EntityHandle, FUTURE_CAUSAL_STACK, InstrumentedFuture, instrument_future,
    instrument_future_with_handle, register_current_task_scope,
};
use moire_types::FutureEntity;

/// Extension trait for attaching a diagnostic name to any future.
///
/// Calling `.named("my_task")` wraps the future in Moiré's instrumentation so
/// it appears as a named entity in the runtime graph. Works on anything that
/// implements `IntoFuture` — raw futures, `JoinHandle`, etc.
///
/// ```rust,no_run
/// use moire::task::{spawn, FutureExt as _};
///
/// spawn(fetch_data().named("fetch_data"));
/// ```
pub trait FutureExt: IntoFuture + Sized {
    /// Wraps this future with a diagnostic name visible in the Moiré dashboard.
    fn named(self, name: impl Into<String>) -> InstrumentedFuture<Self::IntoFuture> {
        instrument_future(name, self.into_future(), None, None)
    }
}

impl<F: IntoFuture + Sized> FutureExt for F {}

/// Spawns a task, equivalent to [`tokio::task::spawn`].
pub fn spawn<T, F>(future: F) -> JoinHandle<T>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let handle = EntityHandle::new("task.spawn", FutureEntity {});
    let future_handle = handle.clone();
    let fut = FUTURE_CAUSAL_STACK.scope(RefCell::new(Vec::new()), async move {
        let _task_scope = register_current_task_scope("spawn");
        instrument_future_with_handle(future_handle, future, None, None).await
    });
    JoinHandle::new(tokio::spawn(fut), handle)
}

/// Spawns a blocking task, equivalent to [`tokio::task::spawn_blocking`].
pub fn spawn_blocking<T, F>(f: F) -> JoinHandle<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let handle = EntityHandle::new("task.spawn_blocking", FutureEntity {});
    let inner = tokio::task::spawn_blocking(move || {
        let _task_scope = register_current_task_scope("spawn_blocking");
        f()
    });
    JoinHandle::new(inner, handle)
}
