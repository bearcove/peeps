use peeps_types::{EntityBody, NotifyEntity, OperationKind};
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use super::super::db::runtime_db;
use super::super::futures::instrument_operation_on_with_source;
use super::super::handles::EntityHandle;
use super::super::{CrateContext, UnqualSource};

#[derive(Clone)]
pub struct Notify {
    inner: Arc<tokio::sync::Notify>,
    handle: EntityHandle,
    waiter_count: Arc<AtomicU32>,
}

impl Notify {
    pub fn new(name: impl Into<String>, source: UnqualSource) -> Self {
        let name = name.into();
        let handle = EntityHandle::new(
            name,
            EntityBody::Notify(NotifyEntity { waiter_count: 0 }),
            source,
        );
        Self {
            inner: Arc::new(tokio::sync::Notify::new()),
            handle,
            waiter_count: Arc::new(AtomicU32::new(0)),
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn notified_with_cx(&self, cx: CrateContext) -> impl Future<Output = ()> + '_ {
        self.notified_with_source(UnqualSource::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn notified_with_source(
        &self,
        source: UnqualSource,
        cx: CrateContext,
    ) -> impl Future<Output = ()> + '_ {
        async move {
            let waiters = self
                .waiter_count
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            if let Ok(mut db) = runtime_db().lock() {
                db.update_notify_waiter_count(self.handle.id(), waiters);
            }

            instrument_operation_on_with_source(
                &self.handle,
                OperationKind::NotifyWait,
                self.inner.notified(),
                source,
                cx,
            )
            .await;

            let waiters = self
                .waiter_count
                .fetch_sub(1, Ordering::Relaxed)
                .saturating_sub(1);
            if let Ok(mut db) = runtime_db().lock() {
                db.update_notify_waiter_count(self.handle.id(), waiters);
            }
        }
    }

    #[track_caller]
    pub fn notify_one(&self) {
        self.inner.notify_one();
    }

    #[track_caller]
    pub fn notify_waiters(&self) {
        self.inner.notify_waiters();
    }
}

#[macro_export]
macro_rules! notify {
    ($name:expr $(,)?) => {
        $crate::Notify::new($name, $crate::Source::caller())
    };
}
