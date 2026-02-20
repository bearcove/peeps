// r[impl api.mutex]
use moire_types::{EdgeKind, EntityBody, LockEntity, LockKind};
use std::ops::{Deref, DerefMut};

use super::super::capture_backtrace_id;
use moire_runtime::{
    current_causal_target, AsEntityRef, EdgeHandle, EntityHandle, EntityRef, HELD_MUTEX_STACK,
};

/// Instrumented version of [`parking_lot::Mutex`], preserving lock semantics with diagnostics.
pub struct Mutex<T> {
    inner: parking_lot::Mutex<T>,
    handle: EntityHandle<moire_types::Lock>,
}

/// Guard returned by [`Mutex`], equivalent to [`parking_lot::MutexGuard`].
pub struct MutexGuard<'a, T> {
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

impl<T> Mutex<T> {
    /// Creates a new instrumented mutex, equivalent to [`parking_lot::Mutex::new`].
    pub fn new(name: &'static str, value: T) -> Self {
        let source = capture_backtrace_id();
        let handle = EntityHandle::new(
            name,
            EntityBody::Lock(LockEntity {
                kind: LockKind::Mutex,
            }),
            source,
        )
        .into_typed::<moire_types::Lock>();
        Self {
            inner: parking_lot::Mutex::new(value),
            handle,
        }
    }
    /// Acquires the lock, matching [`parking_lot::Mutex::lock`].
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self._lock()
    }

    #[doc(hidden)]
    /// Internal helper for lock acquisition with ownership edge tracking.
    pub fn _lock(&self) -> MutexGuard<'_, T> {
        let source = capture_backtrace_id();
        let owner_ref = current_causal_target();

        if let Some(inner) = self.inner.try_lock() {
            return self.wrap_guard(inner, owner_ref.as_ref(), None);
        }

        let waiting_edge = owner_ref.as_ref().map(|owner| {
            self.handle
                .link_to_owned_with_source(owner, EdgeKind::WaitingOn, source)
        });
        let inner = self.inner.lock();
        drop(waiting_edge);

        self.wrap_guard(inner, owner_ref.as_ref(), None)
    }
    /// Attempts lock acquisition without blocking, matching [`parking_lot::Mutex::try_lock`].
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self._try_lock()
    }

    #[doc(hidden)]
    /// Internal helper for non-blocking lock attempt.
    pub fn _try_lock(&self) -> Option<MutexGuard<'_, T>> {
        let owner_ref = current_causal_target();
        self.inner
            .try_lock()
            .map(|inner| self.wrap_guard(inner, owner_ref.as_ref(), Some(EdgeKind::Polls)))
    }

    fn wrap_guard<'a>(
        &self,
        inner: parking_lot::MutexGuard<'a, T>,
        owner_ref: Option<&EntityRef>,
        pre_edge_kind: Option<EdgeKind>,
    ) -> MutexGuard<'a, T> {
        let source = capture_backtrace_id();
        if let (Some(owner), Some(kind)) = (owner_ref, pre_edge_kind) {
            self.handle.link_to_with_source(owner, kind, source);
        }

        let holds_edge = owner_ref.map(|owner| {
            self.handle
                .link_to_owned_with_source(owner, EdgeKind::Holds, source)
        });
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

impl<T> AsEntityRef for Mutex<T> {
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
