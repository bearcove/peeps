use peeps_types::{EdgeKind, EntityBody, EntityId, OperationKind, SemaphoreEntity};
use std::collections::BTreeMap;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use super::super::db::runtime_db;
use super::super::futures::instrument_operation_on_with_source;
use super::super::handles::{current_causal_target, AsEntityRef, EntityHandle, EntityRef};
use super::super::{CrateContext, UnqualSource};

#[derive(Clone)]
pub struct Semaphore {
    inner: Arc<tokio::sync::Semaphore>,
    handle: EntityHandle,
    max_permits: Arc<AtomicU32>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityId, u32>>>,
}

pub struct SemaphorePermit<'a> {
    inner: Option<tokio::sync::SemaphorePermit<'a>>,
    semaphore_id: EntityId,
    holder_future_id: Option<EntityId>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityId, u32>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    max_permits: Arc<AtomicU32>,
}

pub struct OwnedSemaphorePermit {
    inner: Option<tokio::sync::OwnedSemaphorePermit>,
    semaphore_id: EntityId,
    holder_future_id: Option<EntityId>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityId, u32>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    max_permits: Arc<AtomicU32>,
}

pub(super) fn sync_semaphore_state(
    semaphore_id: &EntityId,
    semaphore: &Arc<tokio::sync::Semaphore>,
    max_permits: u32,
) {
    let available = semaphore.available_permits().min(u32::MAX as usize) as u32;
    let handed_out_permits = max_permits.saturating_sub(available);
    if let Ok(mut db) = runtime_db().lock() {
        db.update_semaphore_state(semaphore_id, max_permits, handed_out_permits);
    }
}

pub(super) fn release_semaphore_holder_edge(
    semaphore_id: &EntityId,
    holder_future_id: &mut Option<EntityId>,
    holder_counts: &Arc<StdMutex<BTreeMap<EntityId, u32>>>,
) {
    let Some(holder_id) = holder_future_id.take() else {
        return;
    };

    let should_remove = if let Ok(mut counts) = holder_counts.lock() {
        match counts.get_mut(&holder_id) {
            None => false,
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => {
                counts.remove(&holder_id);
                true
            }
        }
    } else {
        false
    };

    if should_remove {
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_edge(semaphore_id, &holder_id, EdgeKind::Holds);
        }
    }
}

impl Semaphore {
    pub fn new(name: impl Into<String>, permits: usize, source: UnqualSource) -> Self {
        let max_permits = permits.min(u32::MAX as usize) as u32;
        let handle = EntityHandle::new(
            name.into(),
            EntityBody::Semaphore(SemaphoreEntity {
                max_permits,
                handed_out_permits: 0,
            }),
            source,
        );
        Self {
            inner: Arc::new(tokio::sync::Semaphore::new(permits)),
            handle,
            max_permits: Arc::new(AtomicU32::new(max_permits)),
            holder_counts: Arc::new(StdMutex::new(BTreeMap::new())),
        }
    }

    #[track_caller]
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    #[track_caller]
    pub fn close(&self) {
        self.inner.close();
    }

    #[track_caller]
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    #[track_caller]
    pub fn add_permits(&self, n: usize) {
        self.inner.add_permits(n);
        let delta = n.min(u32::MAX as usize) as u32;
        let max = self
            .max_permits
            .fetch_add(delta, Ordering::Relaxed)
            .saturating_add(delta);
        self.sync_state(max);
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn acquire_with_cx(
        &self,
        cx: CrateContext,
    ) -> impl Future<Output = Result<SemaphorePermit<'_>, tokio::sync::AcquireError>> + '_ {
        self.acquire_with_source(UnqualSource::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn acquire_with_source(
        &self,
        source: UnqualSource,
        cx: CrateContext,
    ) -> impl Future<Output = Result<SemaphorePermit<'_>, tokio::sync::AcquireError>> + '_ {
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        async move {
            let permit = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Acquire,
                self.inner.acquire(),
                source,
                cx,
            )
            .await?;
            if let Some(holder_id) = holder_future_id.as_ref() {
                self.note_holder_acquired(holder_id);
            }
            self.sync_state(self.max_permits.load(Ordering::Relaxed));
            Ok(SemaphorePermit {
                inner: Some(permit),
                semaphore_id: self.handle.id().clone(),
                holder_future_id,
                holder_counts: Arc::clone(&self.holder_counts),
                semaphore: Arc::clone(&self.inner),
                max_permits: Arc::clone(&self.max_permits),
            })
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn acquire_many_with_cx(
        &self,
        n: u32,
        cx: CrateContext,
    ) -> impl Future<Output = Result<SemaphorePermit<'_>, tokio::sync::AcquireError>> + '_ {
        self.acquire_many_with_source(n, UnqualSource::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn acquire_many_with_source(
        &self,
        n: u32,
        source: UnqualSource,
        cx: CrateContext,
    ) -> impl Future<Output = Result<SemaphorePermit<'_>, tokio::sync::AcquireError>> + '_ {
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        async move {
            let permit = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Acquire,
                self.inner.acquire_many(n),
                source,
                cx,
            )
            .await?;
            if let Some(holder_id) = holder_future_id.as_ref() {
                self.note_holder_acquired(holder_id);
            }
            self.sync_state(self.max_permits.load(Ordering::Relaxed));
            Ok(SemaphorePermit {
                inner: Some(permit),
                semaphore_id: self.handle.id().clone(),
                holder_future_id,
                holder_counts: Arc::clone(&self.holder_counts),
                semaphore: Arc::clone(&self.inner),
                max_permits: Arc::clone(&self.max_permits),
            })
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn acquire_owned_with_cx(
        &self,
        cx: CrateContext,
    ) -> impl Future<Output = Result<OwnedSemaphorePermit, tokio::sync::AcquireError>> + '_ {
        self.acquire_owned_with_source(UnqualSource::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn acquire_owned_with_source(
        &self,
        source: UnqualSource,
        cx: CrateContext,
    ) -> impl Future<Output = Result<OwnedSemaphorePermit, tokio::sync::AcquireError>> + '_ {
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        async move {
            let permit = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Acquire,
                Arc::clone(&self.inner).acquire_owned(),
                source,
                cx,
            )
            .await?;
            if let Some(holder_id) = holder_future_id.as_ref() {
                self.note_holder_acquired(holder_id);
            }
            self.sync_state(self.max_permits.load(Ordering::Relaxed));
            Ok(OwnedSemaphorePermit {
                inner: Some(permit),
                semaphore_id: self.handle.id().clone(),
                holder_future_id,
                holder_counts: Arc::clone(&self.holder_counts),
                semaphore: Arc::clone(&self.inner),
                max_permits: Arc::clone(&self.max_permits),
            })
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn acquire_many_owned_with_cx(
        &self,
        n: u32,
        cx: CrateContext,
    ) -> impl Future<Output = Result<OwnedSemaphorePermit, tokio::sync::AcquireError>> + '_ {
        self.acquire_many_owned_with_source(n, UnqualSource::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn acquire_many_owned_with_source(
        &self,
        n: u32,
        source: UnqualSource,
        cx: CrateContext,
    ) -> impl Future<Output = Result<OwnedSemaphorePermit, tokio::sync::AcquireError>> + '_ {
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        async move {
            let permit = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Acquire,
                Arc::clone(&self.inner).acquire_many_owned(n),
                source,
                cx,
            )
            .await?;
            if let Some(holder_id) = holder_future_id.as_ref() {
                self.note_holder_acquired(holder_id);
            }
            self.sync_state(self.max_permits.load(Ordering::Relaxed));
            Ok(OwnedSemaphorePermit {
                inner: Some(permit),
                semaphore_id: self.handle.id().clone(),
                holder_future_id,
                holder_counts: Arc::clone(&self.holder_counts),
                semaphore: Arc::clone(&self.inner),
                max_permits: Arc::clone(&self.max_permits),
            })
        }
    }

    #[track_caller]
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire()?;
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        if let Some(holder_id) = holder_future_id.as_ref() {
            self.note_holder_acquired(holder_id);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(SemaphorePermit {
            inner: Some(permit),
            semaphore_id: self.handle.id().clone(),
            holder_future_id,
            holder_counts: Arc::clone(&self.holder_counts),
            semaphore: Arc::clone(&self.inner),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[track_caller]
    pub fn try_acquire_many(
        &self,
        n: u32,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire_many(n)?;
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        if let Some(holder_id) = holder_future_id.as_ref() {
            self.note_holder_acquired(holder_id);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(SemaphorePermit {
            inner: Some(permit),
            semaphore_id: self.handle.id().clone(),
            holder_future_id,
            holder_counts: Arc::clone(&self.holder_counts),
            semaphore: Arc::clone(&self.inner),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[track_caller]
    pub fn try_acquire_owned(&self) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let permit = Arc::clone(&self.inner).try_acquire_owned()?;
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        if let Some(holder_id) = holder_future_id.as_ref() {
            self.note_holder_acquired(holder_id);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(OwnedSemaphorePermit {
            inner: Some(permit),
            semaphore_id: self.handle.id().clone(),
            holder_future_id,
            holder_counts: Arc::clone(&self.holder_counts),
            semaphore: Arc::clone(&self.inner),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[track_caller]
    pub fn try_acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let permit = Arc::clone(&self.inner).try_acquire_many_owned(n)?;
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        if let Some(holder_id) = holder_future_id.as_ref() {
            self.note_holder_acquired(holder_id);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(OwnedSemaphorePermit {
            inner: Some(permit),
            semaphore_id: self.handle.id().clone(),
            holder_future_id,
            holder_counts: Arc::clone(&self.holder_counts),
            semaphore: Arc::clone(&self.inner),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    fn sync_state(&self, max_permits: u32) {
        sync_semaphore_state(self.handle.id(), &self.inner, max_permits);
    }

    fn note_holder_acquired(&self, holder_id: &EntityId) {
        let should_insert = if let Ok(mut holder_counts) = self.holder_counts.lock() {
            let count = holder_counts.entry(holder_id.clone()).or_insert(0);
            *count = count.saturating_add(1);
            *count == 1
        } else {
            false
        };
        if should_insert {
            if let Ok(mut db) = runtime_db().lock() {
                db.upsert_edge(self.handle.id(), holder_id, EdgeKind::Holds);
            }
        }
    }
}

impl AsEntityRef for Semaphore {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
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

#[macro_export]
macro_rules! semaphore {
    ($name:expr, $permits:expr $(,)?) => {
        $crate::Semaphore::new($name, $permits, $crate::Source::caller())
    };
}
