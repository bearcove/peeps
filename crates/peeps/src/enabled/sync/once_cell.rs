use peeps_types::{EntityBody, OnceCellEntity, OnceCellState, OperationKind};
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};

use super::super::db::runtime_db;
use super::super::futures::instrument_operation_on_with_source;
use super::super::handles::EntityHandle;
use super::super::{PeepsContext, Source};

pub struct OnceCell<T> {
    inner: tokio::sync::OnceCell<T>,
    handle: EntityHandle,
    waiter_count: AtomicU32,
}

impl<T> OnceCell<T> {
    pub fn new(name: impl Into<String>, source: Source) -> Self {
        let handle = EntityHandle::new(
            name.into(),
            EntityBody::OnceCell(OnceCellEntity {
                waiter_count: 0,
                state: OnceCellState::Empty,
            }),
            source,
        );
        Self {
            inner: tokio::sync::OnceCell::new(),
            handle,
            waiter_count: AtomicU32::new(0),
        }
    }

    #[track_caller]
    pub fn get(&self) -> Option<&T> {
        self.inner.get()
    }

    #[track_caller]
    pub fn initialized(&self) -> bool {
        self.inner.initialized()
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn get_or_init_with_cx<'a, F, Fut>(
        &'a self,
        f: F,
        cx: PeepsContext,
    ) -> impl Future<Output = &'a T> + 'a
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = T> + 'a,
    {
        self.get_or_init_with_source(f, Source::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn get_or_init_with_source<'a, F, Fut>(
        &'a self,
        f: F,
        source: Source,
        cx: PeepsContext,
    ) -> impl Future<Output = &'a T> + 'a
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = T> + 'a,
    {
        async move {
            let waiters = self
                .waiter_count
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            if let Ok(mut db) = runtime_db().lock() {
                db.update_once_cell_state(self.handle.id(), waiters, OnceCellState::Initializing);
            }

            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::OncecellWait,
                self.inner.get_or_init(f),
                source,
                cx,
            )
            .await;

            let waiters = self
                .waiter_count
                .fetch_sub(1, Ordering::Relaxed)
                .saturating_sub(1);
            let state = if self.inner.initialized() {
                OnceCellState::Initialized
            } else if waiters > 0 {
                OnceCellState::Initializing
            } else {
                OnceCellState::Empty
            };
            if let Ok(mut db) = runtime_db().lock() {
                db.update_once_cell_state(self.handle.id(), waiters, state);
            }

            result
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn get_or_try_init_with_cx<'a, F, Fut, E>(
        &'a self,
        f: F,
        cx: PeepsContext,
    ) -> impl Future<Output = Result<&'a T, E>> + 'a
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = Result<T, E>> + 'a,
    {
        self.get_or_try_init_with_source(f, Source::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn get_or_try_init_with_source<'a, F, Fut, E>(
        &'a self,
        f: F,
        source: Source,
        cx: PeepsContext,
    ) -> impl Future<Output = Result<&'a T, E>> + 'a
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = Result<T, E>> + 'a,
    {
        async move {
            let waiters = self
                .waiter_count
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            if let Ok(mut db) = runtime_db().lock() {
                db.update_once_cell_state(self.handle.id(), waiters, OnceCellState::Initializing);
            }

            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::OncecellWait,
                self.inner.get_or_try_init(f),
                source,
                cx,
            )
            .await;

            let waiters = self
                .waiter_count
                .fetch_sub(1, Ordering::Relaxed)
                .saturating_sub(1);
            let state = if self.inner.initialized() {
                OnceCellState::Initialized
            } else if waiters > 0 {
                OnceCellState::Initializing
            } else {
                OnceCellState::Empty
            };
            if let Ok(mut db) = runtime_db().lock() {
                db.update_once_cell_state(self.handle.id(), waiters, state);
            }

            result
        }
    }

    #[track_caller]
    pub fn set(&self, value: T) -> Result<(), T> {
        let result = self.inner.set(value).map_err(|e| match e {
            tokio::sync::SetError::AlreadyInitializedError(v) => v,
            tokio::sync::SetError::InitializingError(v) => v,
        });
        let state = if self.inner.initialized() {
            OnceCellState::Initialized
        } else if self.waiter_count.load(Ordering::Relaxed) > 0 {
            OnceCellState::Initializing
        } else {
            OnceCellState::Empty
        };
        if let Ok(mut db) = runtime_db().lock() {
            db.update_once_cell_state(
                self.handle.id(),
                self.waiter_count.load(Ordering::Relaxed),
                state,
            );
        }
        result
    }
}

#[macro_export]
macro_rules! once_cell {
    ($name:expr $(,)?) => {
        $crate::OnceCell::new($name, $crate::Source::caller())
    };
}
