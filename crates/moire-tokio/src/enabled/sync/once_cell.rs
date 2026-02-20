// r[impl api.once-cell]
use moire_types::{OnceCellEntity, OnceCellState};
use std::future::Future;

use moire_runtime::{instrument_operation_on, EntityHandle};

/// Instrumented version of [`tokio::sync::OnceCell`].
pub struct OnceCell<T> {
    inner: tokio::sync::OnceCell<T>,
    handle: EntityHandle<moire_types::OnceCell>,
}

impl<T> OnceCell<T> {
    /// Creates a new instrumented once-cell, matching [`tokio::sync::OnceCell::new`].
    pub fn new(name: impl Into<String>) -> Self {
        let handle = EntityHandle::new(
            name.into(),
            OnceCellEntity {
                waiter_count: 0,
                state: OnceCellState::Empty,
            },
        );
        Self {
            inner: tokio::sync::OnceCell::new(),
            handle,
        }
    }

    /// Returns a reference if initialized, matching [`tokio::sync::OnceCell::get`].
    pub fn get(&self) -> Option<&T> {
        self.inner.get()
    }

    /// Returns whether the cell is initialized, matching [`tokio::sync::OnceCell::initialized`].
    pub fn initialized(&self) -> bool {
        self.inner.initialized()
    }

    /// Gets or initializes the value asynchronously, matching [`tokio::sync::OnceCell::get_or_init`].
    pub async fn get_or_init<'a, F, Fut>(&'a self, f: F) -> &'a T
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = T> + 'a,
    {
                let _ = self.handle.mutate(|body| {
            body.waiter_count = body.waiter_count.saturating_add(1);
            body.state = OnceCellState::Initializing;
        });

        let result =
            instrument_operation_on(&self.handle, self.inner.get_or_init(f))
                .await;

        let initialized = self.inner.initialized();
        let _ = self.handle.mutate(|body| {
            body.waiter_count = body.waiter_count.saturating_sub(1);
            body.state = if initialized {
                OnceCellState::Initialized
            } else if body.waiter_count > 0 {
                OnceCellState::Initializing
            } else {
                OnceCellState::Empty
            };
        });

        result
    }
    /// Gets or tries to initialize the value asynchronously, matching [`tokio::sync::OnceCell::get_or_try_init`].
    pub async fn get_or_try_init<'a, F, Fut, E>(&'a self, f: F) -> Result<&'a T, E>
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = Result<T, E>> + 'a,
    {
                let _ = self.handle.mutate(|body| {
            body.waiter_count = body.waiter_count.saturating_add(1);
            body.state = OnceCellState::Initializing;
        });

        let result = instrument_operation_on(
            &self.handle,
            self.inner.get_or_try_init(f), 
        )
        .await;

        let initialized = self.inner.initialized();
        let _ = self.handle.mutate(|body| {
            body.waiter_count = body.waiter_count.saturating_sub(1);
            body.state = if initialized {
                OnceCellState::Initialized
            } else if body.waiter_count > 0 {
                OnceCellState::Initializing
            } else {
                OnceCellState::Empty
            };
        });

        result
    }

    /// Sets the value, matching [`tokio::sync::OnceCell::set`].
    pub fn set(&self, value: T) -> Result<(), T> {
        let result = self.inner.set(value).map_err(|e| match e {
            tokio::sync::SetError::AlreadyInitializedError(v) => v,
            tokio::sync::SetError::InitializingError(v) => v,
        });
        let initialized = self.inner.initialized();
        let _ = self.handle.mutate(|body| {
            body.state = if initialized {
                OnceCellState::Initialized
            } else if body.waiter_count > 0 {
                OnceCellState::Initializing
            } else {
                OnceCellState::Empty
            };
        });
        result
    }
}
