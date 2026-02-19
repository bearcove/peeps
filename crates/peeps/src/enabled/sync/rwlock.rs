use peeps_types::{EntityBody, LockEntity, LockKind};

use super::super::handles::{AsEntityRef, EntityHandle, EntityRef};
use super::super::{CrateContext, Source, UnqualSource};

pub struct RwLock<T> {
    inner: parking_lot::RwLock<T>,
    handle: EntityHandle,
}

impl<T> RwLock<T> {
    pub fn new(name: &'static str, value: T, source: UnqualSource) -> Self {
        let handle = EntityHandle::new(
            name,
            EntityBody::Lock(LockEntity {
                kind: LockKind::RwLock,
            }),
            source,
        );
        Self {
            inner: parking_lot::RwLock::new(value),
            handle,
        }
    }

    #[track_caller]
    pub fn read_with_cx(&self, cx: CrateContext) -> parking_lot::RwLockReadGuard<'_, T> {
        self.read_with_source(cx.join(UnqualSource::caller()))
    }

    pub fn read_with_source(&self, _source: Source) -> parking_lot::RwLockReadGuard<'_, T> {
        self.inner.read()
    }

    #[track_caller]
    pub fn write_with_cx(&self, cx: CrateContext) -> parking_lot::RwLockWriteGuard<'_, T> {
        self.write_with_source(cx.join(UnqualSource::caller()))
    }

    pub fn write_with_source(&self, _source: Source) -> parking_lot::RwLockWriteGuard<'_, T> {
        self.inner.write()
    }

    #[track_caller]
    pub fn try_read_with_cx(
        &self,
        cx: CrateContext,
    ) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        self.try_read_with_source(cx.join(UnqualSource::caller()))
    }

    pub fn try_read_with_source(
        &self,
        _source: Source,
    ) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        self.inner.try_read()
    }

    #[track_caller]
    pub fn try_write_with_cx(
        &self,
        cx: CrateContext,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        self.try_write_with_source(cx.join(UnqualSource::caller()))
    }

    pub fn try_write_with_source(
        &self,
        _source: Source,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        self.inner.try_write()
    }
}

impl<T> AsEntityRef for RwLock<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

#[macro_export]
macro_rules! rwlock {
    ($name:expr, $value:expr $(,)?) => {{
        $crate::RwLock::new($name, $value, $crate::Source::caller())
    }};
}
