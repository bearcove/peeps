pub use join_handle::JoinHandle;
pub use joinset::JoinSet;

pub mod join_handle;
pub mod joinset;

use std::future::IntoFuture;

/// No-op extension trait matching the enabled `FutureExt`.
pub trait FutureExt: IntoFuture + Sized {
    fn named(self, _name: impl Into<String>) -> Self {
        self
    }
}

impl<F: IntoFuture + Sized> FutureExt for F {}

/// Spawns a task, equivalent to [`tokio::task::spawn`].
pub fn spawn<T, F>(future: F) -> JoinHandle<T>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    JoinHandle(tokio::task::spawn(future))
}

/// Spawns a blocking task, equivalent to [`tokio::task::spawn_blocking`].
pub fn spawn_blocking<T, F>(f: F) -> JoinHandle<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    JoinHandle(tokio::task::spawn_blocking(f))
}
