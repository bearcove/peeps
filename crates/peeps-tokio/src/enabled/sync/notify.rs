use peeps_types::{EntityBody, NotifyEntity};
use std::sync::Arc;

use super::super::{local_source, Source, SourceRight};
use peeps_runtime::{instrument_operation_on_with_source, EntityHandle};

#[derive(Clone)]
pub struct Notify {
    inner: Arc<tokio::sync::Notify>,
    handle: EntityHandle<peeps_types::Notify>,
}

impl Notify {
    pub fn new(name: impl Into<String>, source: SourceRight) -> Self {
        let handle = EntityHandle::new(
            name.into(),
            EntityBody::Notify(NotifyEntity { waiter_count: 0 }),
            local_source(source),
        )
        .into_typed::<peeps_types::Notify>();
        Self {
            inner: Arc::new(tokio::sync::Notify::new()),
            handle,
        }
    }

    #[doc(hidden)]
    pub async fn notified_with_source(&self, source: Source) {
        let _ = self
            .handle
            .mutate(|body| body.waiter_count = body.waiter_count.saturating_add(1));

        instrument_operation_on_with_source(&self.handle, self.inner.notified(), &source).await;

        let _ = self
            .handle
            .mutate(|body| body.waiter_count = body.waiter_count.saturating_sub(1));
    }

    pub fn notify_one(&self) {
        self.inner.notify_one();
    }

    pub fn notify_waiters(&self) {
        self.inner.notify_waiters();
    }
}
