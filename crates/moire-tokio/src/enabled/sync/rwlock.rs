// r[impl api.rwlock]
use moire_types::{EdgeKind, LockEntity, LockKind};
use std::fmt;
use std::ops::{Deref, DerefMut};

use moire_runtime::{
    AsEntityRef, EdgeHandle, EntityHandle, EntityRef, current_causal_target_with_task_fallback,
    instrument_operation_on_with_actor,
};

/// Instrumented version of [`tokio::sync::RwLock`].
pub struct RwLock<T> {
    inner: tokio::sync::RwLock<T>,
    handle: EntityHandle<moire_types::Lock>,
}

/// Read guard returned by [`RwLock::read`].
pub struct RwLockReadGuard<'a, T> {
    inner: tokio::sync::RwLockReadGuard<'a, T>,
    holds_edge: Option<EdgeHandle>,
}

/// Write guard returned by [`RwLock::write`].
pub struct RwLockWriteGuard<'a, T> {
    inner: tokio::sync::RwLockWriteGuard<'a, T>,
    holds_edge: Option<EdgeHandle>,
}

/// Instrumented version of [`parking_lot::RwLock`].
pub struct SyncRwLock<T> {
    inner: parking_lot::RwLock<T>,
    handle: EntityHandle<moire_types::Lock>,
}

impl<'a, T> Deref for RwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> Deref for RwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> RwLock<T> {
    /// Creates a new instrumented async read-write lock, matching [`tokio::sync::RwLock::new`].
    pub fn new(name: &'static str, value: T) -> Self {
        let handle = EntityHandle::new(
            name,
            LockEntity {
                kind: LockKind::RwLock,
            },
        );
        Self {
            inner: tokio::sync::RwLock::new(value),
            handle,
        }
    }

    /// Acquires a shared read guard asynchronously, matching [`tokio::sync::RwLock::read`].
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        let owner_ref = current_causal_target_with_task_fallback();
        let inner =
            instrument_operation_on_with_actor(&self.handle, owner_ref.as_ref(), self.inner.read())
                .await;
        self.wrap_read_guard(inner, owner_ref.as_ref(), None)
    }

    /// Acquires an exclusive write guard asynchronously, matching [`tokio::sync::RwLock::write`].
    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        let owner_ref = current_causal_target_with_task_fallback();
        let inner = instrument_operation_on_with_actor(
            &self.handle,
            owner_ref.as_ref(),
            self.inner.write(),
        )
        .await;
        self.wrap_write_guard(inner, owner_ref.as_ref(), None)
    }

    /// Attempts a non-blocking read lock, matching [`tokio::sync::RwLock::try_read`].
    pub fn try_read(&self) -> Result<RwLockReadGuard<'_, T>, tokio::sync::TryLockError> {
        let owner_ref = current_causal_target_with_task_fallback();
        self.inner
            .try_read()
            .map(|inner| self.wrap_read_guard(inner, owner_ref.as_ref(), Some(EdgeKind::Polls)))
    }

    /// Attempts a non-blocking write lock, matching [`tokio::sync::RwLock::try_write`].
    pub fn try_write(&self) -> Result<RwLockWriteGuard<'_, T>, tokio::sync::TryLockError> {
        let owner_ref = current_causal_target_with_task_fallback();
        self.inner
            .try_write()
            .map(|inner| self.wrap_write_guard(inner, owner_ref.as_ref(), Some(EdgeKind::Polls)))
    }

    fn wrap_read_guard<'a>(
        &self,
        inner: tokio::sync::RwLockReadGuard<'a, T>,
        owner_ref: Option<&EntityRef>,
        pre_edge_kind: Option<EdgeKind>,
    ) -> RwLockReadGuard<'a, T> {
        if let (Some(owner), Some(kind)) = (owner_ref, pre_edge_kind) {
            self.handle.link_to(owner, kind);
        }
        let holds_edge = owner_ref.map(|owner| self.handle.link_to_owned(owner, EdgeKind::Holds));
        RwLockReadGuard { inner, holds_edge }
    }

    fn wrap_write_guard<'a>(
        &self,
        inner: tokio::sync::RwLockWriteGuard<'a, T>,
        owner_ref: Option<&EntityRef>,
        pre_edge_kind: Option<EdgeKind>,
    ) -> RwLockWriteGuard<'a, T> {
        if let (Some(owner), Some(kind)) = (owner_ref, pre_edge_kind) {
            self.handle.link_to(owner, kind);
        }
        let holds_edge = owner_ref.map(|owner| self.handle.link_to_owned(owner, EdgeKind::Holds));
        RwLockWriteGuard { inner, holds_edge }
    }
}

impl<T> SyncRwLock<T> {
    /// Creates a new instrumented sync read-write lock, matching [`parking_lot::RwLock::new`].
    pub fn new(name: &'static str, value: T) -> Self {
        let handle = EntityHandle::new(
            name,
            LockEntity {
                kind: LockKind::RwLock,
            },
        );
        Self {
            inner: parking_lot::RwLock::new(value),
            handle,
        }
    }

    /// Acquires a shared read guard, equivalent to [`parking_lot::RwLock::read`].
    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, T> {
        if let Some(caller) = current_causal_target_with_task_fallback() {
            self.handle.link_to(&caller, EdgeKind::Polls);
        }
        self.inner.read()
    }

    /// Acquires an exclusive write guard, equivalent to [`parking_lot::RwLock::write`].
    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, T> {
        if let Some(caller) = current_causal_target_with_task_fallback() {
            self.handle.link_to(&caller, EdgeKind::Polls);
        }
        self.inner.write()
    }

    /// Attempts a non-blocking read lock, matching [`parking_lot::RwLock::try_read`].
    pub fn try_read(&self) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        if let Some(caller) = current_causal_target_with_task_fallback() {
            self.handle.link_to(&caller, EdgeKind::Polls);
        }
        self.inner.try_read()
    }

    /// Attempts a non-blocking write lock, matching [`parking_lot::RwLock::try_write`].
    pub fn try_write(&self) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        if let Some(caller) = current_causal_target_with_task_fallback() {
            self.handle.link_to(&caller, EdgeKind::Polls);
        }
        self.inner.try_write()
    }
}

impl<T> AsEntityRef for RwLock<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for SyncRwLock<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        let _ = self.holds_edge.take();
    }
}

impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        let _ = self.holds_edge.take();
    }
}

impl<T: fmt::Debug> fmt::Debug for RwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: fmt::Debug> fmt::Debug for RwLockReadGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: fmt::Debug> fmt::Debug for RwLockWriteGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: fmt::Debug> fmt::Debug for SyncRwLock<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
