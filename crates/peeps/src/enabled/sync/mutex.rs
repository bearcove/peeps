use peeps_types::{EdgeKind, EntityBody, EntityId, LockEntity, LockKind};
use std::ops::{Deref, DerefMut};

use super::super::db::runtime_db;
use super::super::handles::{current_causal_target, AsEntityRef, EntityHandle, EntityRef};
use super::super::{CrateContext, UnqualSource, HELD_MUTEX_STACK};

pub struct Mutex<T> {
    inner: parking_lot::Mutex<T>,
    handle: EntityHandle,
}

pub struct MutexGuard<'a, T> {
    inner: parking_lot::MutexGuard<'a, T>,
    lock_id: EntityId,
    owner_future_id: Option<EntityId>,
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
    pub fn new(name: &'static str, value: T, source: UnqualSource) -> Self {
        let handle = EntityHandle::new(
            name,
            EntityBody::Lock(LockEntity {
                kind: LockKind::Mutex,
            }),
            source,
        );
        Self {
            inner: parking_lot::Mutex::new(value),
            handle,
        }
    }

    #[track_caller]
    pub fn lock_with_cx(&self, cx: CrateContext) -> MutexGuard<'_, T> {
        self.lock_with_source(UnqualSource::caller(), cx)
    }

    pub fn lock_with_source(&self, _source: UnqualSource, _cx: CrateContext) -> MutexGuard<'_, T> {
        if let Some(inner) = self.inner.try_lock() {
            return self.wrap_guard(inner);
        }

        let pending_edges = self.record_pending_wait_edges();
        let inner = self.inner.lock();
        self.clear_pending_wait_edges(pending_edges);
        self.wrap_guard(inner)
    }

    #[track_caller]
    pub fn try_lock_with_cx(&self, cx: CrateContext) -> Option<MutexGuard<'_, T>> {
        self.try_lock_with_source(UnqualSource::caller(), cx)
    }

    pub fn try_lock_with_source(
        &self,
        _source: UnqualSource,
        _cx: CrateContext,
    ) -> Option<MutexGuard<'_, T>> {
        self.inner.try_lock().map(|inner| self.wrap_guard(inner))
    }

    fn wrap_guard<'a>(&self, inner: parking_lot::MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let lock_id = EntityId::new(self.handle.id().as_str());
        let owner_future_id =
            current_causal_target().map(|target| EntityId::new(target.id().as_str()));
        if let Some(owner_id) = owner_future_id.as_ref() {
            if let Ok(mut db) = runtime_db().lock() {
                db.upsert_edge(owner_id, &lock_id, EdgeKind::Touches);
                db.upsert_edge(&lock_id, owner_id, EdgeKind::Needs);
            }
        }
        HELD_MUTEX_STACK.with(|stack| {
            stack.borrow_mut().push(EntityId::new(lock_id.as_str()));
        });
        MutexGuard {
            inner,
            lock_id,
            owner_future_id,
        }
    }

    fn record_pending_wait_edges(&self) -> Vec<(EntityId, EntityId)> {
        let dst = EntityId::new(self.handle.id().as_str());
        let mut edges = Vec::<(EntityId, EntityId)>::new();

        if let Some(waiter) = current_causal_target() {
            if waiter.id().as_str() != dst.as_str() {
                edges.push((
                    EntityId::new(waiter.id().as_str()),
                    EntityId::new(dst.as_str()),
                ));
            }
        }

        edges.sort_unstable_by(|(lhs_src, lhs_dst), (rhs_src, rhs_dst)| {
            lhs_src
                .as_str()
                .cmp(rhs_src.as_str())
                .then_with(|| lhs_dst.as_str().cmp(rhs_dst.as_str()))
        });
        edges.dedup();

        if let Ok(mut db) = runtime_db().lock() {
            for (src, dst) in &edges {
                db.upsert_edge(src, dst, EdgeKind::Touches);
                db.upsert_edge(src, dst, EdgeKind::Needs);
            }
        }

        edges
    }

    fn clear_pending_wait_edges(&self, edges: Vec<(EntityId, EntityId)>) {
        if let Ok(mut db) = runtime_db().lock() {
            for (src, dst) in edges {
                db.remove_edge(&src, &dst, EdgeKind::Needs);
            }
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
        if let Some(owner_id) = self.owner_future_id.as_ref() {
            if let Ok(mut db) = runtime_db().lock() {
                db.remove_edge(&self.lock_id, owner_id, EdgeKind::Needs);
            }
        }
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

#[macro_export]
macro_rules! mutex {
    ($name:expr, $value:expr $(,)?) => {{
        $crate::Mutex::new($name, $value, $crate::Source::caller())
    }};
}
