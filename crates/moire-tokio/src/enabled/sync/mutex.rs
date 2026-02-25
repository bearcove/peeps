// r[impl api.mutex]
use moire_types::{EdgeKind, LockEntity, LockKind};
use std::fmt;
use std::ops::{Deref, DerefMut};

use moire_runtime::{
    AsEntityRef, EdgeHandle, EntityHandle, EntityRef, HELD_MUTEX_STACK,
    current_causal_target_with_task_fallback, instrument_operation_on_with_actor,
};

/// Instrumented version of [`tokio::sync::Mutex`].
pub struct Mutex<T> {
    inner: tokio::sync::Mutex<T>,
    handle: EntityHandle<moire_types::Lock>,
}

/// Guard returned by [`Mutex`], equivalent to [`tokio::sync::MutexGuard`].
pub struct MutexGuard<'a, T> {
    inner: tokio::sync::MutexGuard<'a, T>,
    lock_id: moire_types::EntityId,
    holds_edge: Option<EdgeHandle>,
}

/// Instrumented version of [`parking_lot::Mutex`], preserving lock semantics with diagnostics.
pub struct SyncMutex<T> {
    inner: parking_lot::Mutex<T>,
    handle: EntityHandle<moire_types::Lock>,
}

/// Guard returned by [`SyncMutex`], equivalent to [`parking_lot::MutexGuard`].
pub struct SyncMutexGuard<'a, T> {
    inner: parking_lot::MutexGuard<'a, T>,
    lock_id: moire_types::EntityId,
    holds_edge: Option<EdgeHandle>,
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a, T> Deref for SyncMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for SyncMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> Mutex<T> {
    /// Creates a new instrumented async mutex, equivalent to [`tokio::sync::Mutex::new`].
    pub fn new(name: &'static str, value: T) -> Self {
        let handle = EntityHandle::new(
            name,
            LockEntity {
                kind: LockKind::Mutex,
            },
        );
        Self {
            inner: tokio::sync::Mutex::new(value),
            handle,
        }
    }

    /// Acquires the lock asynchronously, matching [`tokio::sync::Mutex::lock`].
    pub async fn lock(&self) -> MutexGuard<'_, T> {
        let owner_ref = current_causal_target_with_task_fallback();
        let inner =
            instrument_operation_on_with_actor(&self.handle, owner_ref.as_ref(), self.inner.lock())
                .await;
        self.wrap_guard(inner, owner_ref.as_ref(), None)
    }

    /// Attempts lock acquisition without waiting, matching [`tokio::sync::Mutex::try_lock`].
    pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, tokio::sync::TryLockError> {
        let owner_ref = current_causal_target_with_task_fallback();
        self.inner
            .try_lock()
            .map(|inner| self.wrap_guard(inner, owner_ref.as_ref(), Some(EdgeKind::Polls)))
    }

    fn wrap_guard<'a>(
        &self,
        inner: tokio::sync::MutexGuard<'a, T>,
        owner_ref: Option<&EntityRef>,
        pre_edge_kind: Option<EdgeKind>,
    ) -> MutexGuard<'a, T> {
        if let (Some(owner), Some(kind)) = (owner_ref, pre_edge_kind) {
            self.handle.link_to(owner, kind);
        }

        let holds_edge = owner_ref.map(|owner| self.handle.link_to_owned(owner, EdgeKind::Holds));
        let lock_id = self.handle.id().clone();

        HELD_MUTEX_STACK.with(|stack| {
            stack.borrow_mut().push(lock_id.clone());
        });

        MutexGuard {
            inner,
            lock_id,
            holds_edge,
        }
    }
}

impl<T> SyncMutex<T> {
    /// Creates a new instrumented sync mutex, equivalent to [`parking_lot::Mutex::new`].
    pub fn new(name: &'static str, value: T) -> Self {
        let handle = EntityHandle::new(
            name,
            LockEntity {
                kind: LockKind::Mutex,
            },
        );
        Self {
            inner: parking_lot::Mutex::new(value),
            handle,
        }
    }

    /// Acquires the lock, matching [`parking_lot::Mutex::lock`].
    pub fn lock(&self) -> SyncMutexGuard<'_, T> {
        let owner_ref = current_causal_target_with_task_fallback();

        if let Some(inner) = self.inner.try_lock() {
            return self.wrap_guard(inner, owner_ref.as_ref(), None);
        }

        let waiting_edge = owner_ref
            .as_ref()
            .map(|owner| owner.link_to_owned(&self.handle, EdgeKind::WaitingOn));
        let inner = self.inner.lock();
        drop(waiting_edge);

        self.wrap_guard(inner, owner_ref.as_ref(), None)
    }

    /// Attempts lock acquisition without blocking, matching [`parking_lot::Mutex::try_lock`].
    pub fn try_lock(&self) -> Option<SyncMutexGuard<'_, T>> {
        let owner_ref = current_causal_target_with_task_fallback();
        self.inner
            .try_lock()
            .map(|inner| self.wrap_guard(inner, owner_ref.as_ref(), Some(EdgeKind::Polls)))
    }

    fn wrap_guard<'a>(
        &self,
        inner: parking_lot::MutexGuard<'a, T>,
        owner_ref: Option<&EntityRef>,
        pre_edge_kind: Option<EdgeKind>,
    ) -> SyncMutexGuard<'a, T> {
        if let (Some(owner), Some(kind)) = (owner_ref, pre_edge_kind) {
            self.handle.link_to(owner, kind);
        }

        let holds_edge = owner_ref.map(|owner| self.handle.link_to_owned(owner, EdgeKind::Holds));
        let lock_id = self.handle.id().clone();

        HELD_MUTEX_STACK.with(|stack| {
            stack.borrow_mut().push(lock_id.clone());
        });

        SyncMutexGuard {
            inner,
            lock_id,
            holds_edge,
        }
    }
}

impl<T> AsEntityRef for Mutex<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for SyncMutex<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        let _ = self.holds_edge.take();
        HELD_MUTEX_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(pos) = stack
                .iter()
                .rposition(|id| id.as_str() == self.lock_id.as_str())
            {
                stack.remove(pos);
            }
        });
    }
}

impl<'a, T> Drop for SyncMutexGuard<'a, T> {
    fn drop(&mut self) {
        let _ = self.holds_edge.take();
        HELD_MUTEX_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(pos) = stack
                .iter()
                .rposition(|id| id.as_str() == self.lock_id.as_str())
            {
                stack.remove(pos);
            }
        });
    }
}

impl<T: fmt::Debug> fmt::Debug for Mutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: fmt::Debug> fmt::Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: fmt::Debug> fmt::Debug for SyncMutex<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T: fmt::Debug> fmt::Debug for SyncMutexGuard<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
