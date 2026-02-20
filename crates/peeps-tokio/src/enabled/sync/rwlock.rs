use peeps_types::{EdgeKind, EntityBody, LockEntity, LockKind};

use super::super::SourceId;
use peeps_runtime::{current_causal_target, AsEntityRef, EntityHandle, EntityRef};

pub struct RwLock<T> {
    inner: parking_lot::RwLock<T>,
    handle: EntityHandle<peeps_types::Lock>,
}

impl<T> RwLock<T> {
    #[doc(hidden)]
    pub fn new_with_source(name: &'static str, value: T, source: SourceId) -> Self {
        let handle = EntityHandle::new(
            name,
            EntityBody::Lock(LockEntity {
                kind: LockKind::RwLock,
            }),
            source,
        )
        .into_typed::<peeps_types::Lock>();
        Self {
            inner: parking_lot::RwLock::new(value),
            handle,
        }
    }

    #[doc(hidden)]
    pub fn read_with_source(&self, source: SourceId) -> parking_lot::RwLockReadGuard<'_, T> {
        if let Some(caller) = current_causal_target() {
            self.handle
                .link_to_with_source(&caller, EdgeKind::Polls, source);
        }
        self.inner.read()
    }

    #[doc(hidden)]
    pub fn write_with_source(&self, source: SourceId) -> parking_lot::RwLockWriteGuard<'_, T> {
        if let Some(caller) = current_causal_target() {
            self.handle
                .link_to_with_source(&caller, EdgeKind::Polls, source);
        }
        self.inner.write()
    }

    #[doc(hidden)]
    pub fn try_read_with_source(
        &self,
        source: SourceId,
    ) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        if let Some(caller) = current_causal_target() {
            self.handle
                .link_to_with_source(&caller, EdgeKind::Polls, source);
        }
        self.inner.try_read()
    }

    #[doc(hidden)]
    pub fn try_write_with_source(
        &self,
        source: SourceId,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        if let Some(caller) = current_causal_target() {
            self.handle
                .link_to_with_source(&caller, EdgeKind::Polls, source);
        }
        self.inner.try_write()
    }
}

impl<T> AsEntityRef for RwLock<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
