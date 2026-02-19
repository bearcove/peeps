use peeps_types::{EntityBody, OnceCellEntity, OnceCellState};
use std::future::Future;

use super::super::{local_source, Source, SourceRight};
use peeps_runtime::{instrument_operation_on_with_source, EntityHandle};

pub struct OnceCell<T> {
    inner: tokio::sync::OnceCell<T>,
    handle: EntityHandle<peeps_types::OnceCell>,
}

impl<T> OnceCell<T> {
    pub fn new(name: impl Into<String>, source: SourceRight) -> Self {
        let handle = EntityHandle::new(
            name.into(),
            EntityBody::OnceCell(OnceCellEntity {
                waiter_count: 0,
                state: OnceCellState::Empty,
            }),
            local_source(source),
        )
        .into_typed::<peeps_types::OnceCell>();
        Self {
            inner: tokio::sync::OnceCell::new(),
            handle,
        }
    }

    pub fn get(&self) -> Option<&T> {
        self.inner.get()
    }

    pub fn initialized(&self) -> bool {
        self.inner.initialized()
    }

    #[doc(hidden)]
    pub async fn get_or_init_with_source<'a, F, Fut>(&'a self, f: F, source: Source) -> &'a T
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = T> + 'a,
    {
        let _ = self.handle.mutate(|body| {
            body.waiter_count = body.waiter_count.saturating_add(1);
            body.state = OnceCellState::Initializing;
        });

        let result =
            instrument_operation_on_with_source(&self.handle, self.inner.get_or_init(f), &source)
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

    #[doc(hidden)]
    pub async fn get_or_try_init_with_source<'a, F, Fut, E>(
        &'a self,
        f: F,
        source: Source,
    ) -> Result<&'a T, E>
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = Result<T, E>> + 'a,
    {
        let _ = self.handle.mutate(|body| {
            body.waiter_count = body.waiter_count.saturating_add(1);
            body.state = OnceCellState::Initializing;
        });

        let result = instrument_operation_on_with_source(
            &self.handle,
            self.inner.get_or_try_init(f),
            &source,
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
