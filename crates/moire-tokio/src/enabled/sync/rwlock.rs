// r[impl api.rwlock]
use moire_types::{EdgeKind, EntityBody, LockEntity, LockKind};

use moire_runtime::capture_backtrace_id;
use moire_runtime::{current_causal_target, AsEntityRef, EntityHandle, EntityRef};

/// Instrumented version of [`parking_lot::RwLock`].
pub struct RwLock<T> {
    inner: parking_lot::RwLock<T>,
    handle: EntityHandle<moire_types::Lock>,
}

impl<T> RwLock<T> {
    /// Creates a new instrumented read-write lock, matching [`parking_lot::RwLock::new`].
    pub fn new(name: &'static str, value: T) -> Self {
        let source = capture_backtrace_id();
        let handle = EntityHandle::new(
            name,
            EntityBody::Lock(LockEntity {
                kind: LockKind::RwLock,
            }),
            source,
        )
        .into_typed::<moire_types::Lock>();
        Self {
            inner: parking_lot::RwLock::new(value),
            handle,
        }
    }
    /// Acquires a shared read guard, equivalent to [`parking_lot::RwLock::read`].
    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, T> {
        let source = capture_backtrace_id();
        if let Some(caller) = current_causal_target() {
            self.handle
                .link_to_with_source(&caller, EdgeKind::Polls, source);
        }
        self.inner.read()
    }
    /// Acquires an exclusive write guard, equivalent to [`parking_lot::RwLock::write`].
    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, T> {
        let source = capture_backtrace_id();
        if let Some(caller) = current_causal_target() {
            self.handle
                .link_to_with_source(&caller, EdgeKind::Polls, source);
        }
        self.inner.write()
    }
    /// Attempts a non-blocking read lock, matching [`parking_lot::RwLock::try_read`].
    pub fn try_read(&self) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        let source = capture_backtrace_id();
        if let Some(caller) = current_causal_target() {
            self.handle
                .link_to_with_source(&caller, EdgeKind::Polls, source);
        }
        self.inner.try_read()
    }
    /// Attempts a non-blocking write lock, matching [`parking_lot::RwLock::try_write`].
    pub fn try_write(&self) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        let source = capture_backtrace_id();
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
