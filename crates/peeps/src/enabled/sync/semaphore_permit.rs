use peeps_types::EntityId;
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use super::semaphore::{release_semaphore_holder_edge, sync_semaphore_state};

pub struct SemaphorePermit<'a> {
    pub(super) inner: Option<tokio::sync::SemaphorePermit<'a>>,
    pub(super) semaphore_id: EntityId,
    pub(super) holder_future_id: Option<EntityId>,
    pub(super) holder_counts: Arc<StdMutex<BTreeMap<EntityId, u32>>>,
    pub(super) semaphore: Arc<tokio::sync::Semaphore>,
    pub(super) max_permits: Arc<AtomicU32>,
}

pub struct OwnedSemaphorePermit {
    pub(super) inner: Option<tokio::sync::OwnedSemaphorePermit>,
    pub(super) semaphore_id: EntityId,
    pub(super) holder_future_id: Option<EntityId>,
    pub(super) holder_counts: Arc<StdMutex<BTreeMap<EntityId, u32>>>,
    pub(super) semaphore: Arc<tokio::sync::Semaphore>,
    pub(super) max_permits: Arc<AtomicU32>,
}

impl<'a> Deref for SemaphorePermit<'a> {
    type Target = tokio::sync::SemaphorePermit<'a>;

    fn deref(&self) -> &Self::Target {
        self.inner
            .as_ref()
            .expect("semaphore permit accessed after drop")
    }
}

impl<'a> DerefMut for SemaphorePermit<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
            .as_mut()
            .expect("semaphore permit accessed after drop")
    }
}

impl<'a> Drop for SemaphorePermit<'a> {
    fn drop(&mut self) {
        let _ = self.inner.take();
        release_semaphore_holder_edge(
            &self.semaphore_id,
            &mut self.holder_future_id,
            &self.holder_counts,
        );
        sync_semaphore_state(
            &self.semaphore_id,
            &self.semaphore,
            self.max_permits.load(Ordering::Relaxed),
        );
    }
}

impl Deref for OwnedSemaphorePermit {
    type Target = tokio::sync::OwnedSemaphorePermit;

    fn deref(&self) -> &Self::Target {
        self.inner
            .as_ref()
            .expect("owned semaphore permit accessed after drop")
    }
}

impl DerefMut for OwnedSemaphorePermit {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
            .as_mut()
            .expect("owned semaphore permit accessed after drop")
    }
}

impl Drop for OwnedSemaphorePermit {
    fn drop(&mut self) {
        let _ = self.inner.take();
        release_semaphore_holder_edge(
            &self.semaphore_id,
            &mut self.holder_future_id,
            &self.holder_counts,
        );
        sync_semaphore_state(
            &self.semaphore_id,
            &self.semaphore,
            self.max_permits.load(Ordering::Relaxed),
        );
    }
}
