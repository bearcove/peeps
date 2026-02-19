use peeps_types::{EdgeKind, EntityBody, SemaphoreEntity};
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use super::super::{Source, SourceRight};
use peeps_runtime::{
    current_causal_target, instrument_operation_on_with_source, AsEntityRef, EdgeHandle,
    EntityHandle, EntityRef, WeakEntityHandle,
};

#[derive(Clone)]
pub struct Semaphore {
    inner: Arc<tokio::sync::Semaphore>,
    handle: EntityHandle<peeps_types::Semaphore>,
    max_permits: Arc<AtomicU32>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityRef, HolderEdge>>>,
}

struct HolderEdge {
    count: u32,
    _edge: EdgeHandle,
}

pub struct SemaphorePermit<'a> {
    inner: Option<tokio::sync::SemaphorePermit<'a>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    semaphore_handle: WeakEntityHandle<peeps_types::Semaphore>,
    holder_ref: Option<EntityRef>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityRef, HolderEdge>>>,
    max_permits: Arc<AtomicU32>,
}

pub struct OwnedSemaphorePermit {
    inner: Option<tokio::sync::OwnedSemaphorePermit>,
    semaphore: Arc<tokio::sync::Semaphore>,
    semaphore_handle: WeakEntityHandle<peeps_types::Semaphore>,
    holder_ref: Option<EntityRef>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityRef, HolderEdge>>>,
    max_permits: Arc<AtomicU32>,
}

impl Semaphore {
    pub fn new(name: impl Into<String>, permits: usize, source: SourceRight) -> Self {
        let max_permits = permits.min(u32::MAX as usize) as u32;
        let handle = EntityHandle::new(
            name.into(),
            EntityBody::Semaphore(SemaphoreEntity {
                max_permits,
                handed_out_permits: 0,
            }),
            Source::new(source.into_string(), None),
        )
        .into_typed::<peeps_types::Semaphore>();
        Self {
            inner: Arc::new(tokio::sync::Semaphore::new(permits)),
            handle,
            max_permits: Arc::new(AtomicU32::new(max_permits)),
            holder_counts: Arc::new(StdMutex::new(BTreeMap::new())),
        }
    }

    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    pub fn close(&self) {
        self.inner.close();
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    pub fn add_permits(&self, n: usize) {
        self.inner.add_permits(n);
        let delta = n.min(u32::MAX as usize) as u32;
        let max = self
            .max_permits
            .fetch_add(delta, Ordering::Relaxed)
            .saturating_add(delta);
        self.sync_state(max);
    }

    #[doc(hidden)]
    pub async fn acquire_with_source(
        &self,
        source: Source,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        let holder_ref = current_causal_target();
        let permit =
            instrument_operation_on_with_source(&self.handle, self.inner.acquire(), &source)
                .await?;
        if let Some(holder_ref) = holder_ref.as_ref() {
            self.note_holder_acquired_with_source(holder_ref, &source);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(SemaphorePermit {
            inner: Some(permit),
            semaphore: Arc::clone(&self.inner),
            semaphore_handle: self.handle.downgrade(),
            holder_ref,
            holder_counts: Arc::clone(&self.holder_counts),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[doc(hidden)]
    pub async fn acquire_many_with_source(
        &self,
        n: u32,
        source: Source,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        let holder_ref = current_causal_target();
        let permit =
            instrument_operation_on_with_source(&self.handle, self.inner.acquire_many(n), &source)
                .await?;
        if let Some(holder_ref) = holder_ref.as_ref() {
            self.note_holder_acquired_with_source(holder_ref, &source);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(SemaphorePermit {
            inner: Some(permit),
            semaphore: Arc::clone(&self.inner),
            semaphore_handle: self.handle.downgrade(),
            holder_ref,
            holder_counts: Arc::clone(&self.holder_counts),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[doc(hidden)]
    pub async fn acquire_owned_with_source(
        &self,
        source: Source,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        let holder_ref = current_causal_target();
        let permit = instrument_operation_on_with_source(
            &self.handle,
            Arc::clone(&self.inner).acquire_owned(),
            &source,
        )
        .await?;
        if let Some(holder_ref) = holder_ref.as_ref() {
            self.note_holder_acquired_with_source(holder_ref, &source);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(OwnedSemaphorePermit {
            inner: Some(permit),
            semaphore: Arc::clone(&self.inner),
            semaphore_handle: self.handle.downgrade(),
            holder_ref,
            holder_counts: Arc::clone(&self.holder_counts),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[doc(hidden)]
    pub async fn acquire_many_owned_with_source(
        &self,
        n: u32,
        source: Source,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        let holder_ref = current_causal_target();
        let permit = instrument_operation_on_with_source(
            &self.handle,
            Arc::clone(&self.inner).acquire_many_owned(n),
            &source,
        )
        .await?;
        if let Some(holder_ref) = holder_ref.as_ref() {
            self.note_holder_acquired_with_source(holder_ref, &source);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(OwnedSemaphorePermit {
            inner: Some(permit),
            semaphore: Arc::clone(&self.inner),
            semaphore_handle: self.handle.downgrade(),
            holder_ref,
            holder_counts: Arc::clone(&self.holder_counts),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire()?;
        let holder_ref = current_causal_target();
        if let Some(holder_ref) = holder_ref.as_ref() {
            self.note_holder_acquired(holder_ref);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(SemaphorePermit {
            inner: Some(permit),
            semaphore: Arc::clone(&self.inner),
            semaphore_handle: self.handle.downgrade(),
            holder_ref,
            holder_counts: Arc::clone(&self.holder_counts),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    pub fn try_acquire_many(
        &self,
        n: u32,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire_many(n)?;
        let holder_ref = current_causal_target();
        if let Some(holder_ref) = holder_ref.as_ref() {
            self.note_holder_acquired(holder_ref);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(SemaphorePermit {
            inner: Some(permit),
            semaphore: Arc::clone(&self.inner),
            semaphore_handle: self.handle.downgrade(),
            holder_ref,
            holder_counts: Arc::clone(&self.holder_counts),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    pub fn try_acquire_owned(&self) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let permit = Arc::clone(&self.inner).try_acquire_owned()?;
        let holder_ref = current_causal_target();
        if let Some(holder_ref) = holder_ref.as_ref() {
            self.note_holder_acquired(holder_ref);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(OwnedSemaphorePermit {
            inner: Some(permit),
            semaphore: Arc::clone(&self.inner),
            semaphore_handle: self.handle.downgrade(),
            holder_ref,
            holder_counts: Arc::clone(&self.holder_counts),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    pub fn try_acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let permit = Arc::clone(&self.inner).try_acquire_many_owned(n)?;
        let holder_ref = current_causal_target();
        if let Some(holder_ref) = holder_ref.as_ref() {
            self.note_holder_acquired(holder_ref);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(OwnedSemaphorePermit {
            inner: Some(permit),
            semaphore: Arc::clone(&self.inner),
            semaphore_handle: self.handle.downgrade(),
            holder_ref,
            holder_counts: Arc::clone(&self.holder_counts),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    fn sync_state(&self, max_permits: u32) {
        let available = self.inner.available_permits().min(u32::MAX as usize) as u32;
        let handed_out = max_permits.saturating_sub(available);
        let _ = self.handle.mutate(|body| {
            body.max_permits = max_permits;
            body.handed_out_permits = handed_out;
        });
    }

    fn note_holder_acquired(&self, holder_ref: &EntityRef) {
        if let Ok(mut holder_counts) = self.holder_counts.lock() {
            if let Some(entry) = holder_counts.get_mut(holder_ref) {
                entry.count = entry.count.saturating_add(1);
                return;
            }
            let edge = self.handle.link_to_owned(holder_ref, EdgeKind::Holds);
            holder_counts.insert(
                holder_ref.clone(),
                HolderEdge {
                    count: 1,
                    _edge: edge,
                },
            );
        }
    }

    fn note_holder_acquired_with_source(&self, holder_ref: &EntityRef, source: &Source) {
        if let Ok(mut holder_counts) = self.holder_counts.lock() {
            if let Some(entry) = holder_counts.get_mut(holder_ref) {
                entry.count = entry.count.saturating_add(1);
                return;
            }
            let edge =
                self.handle
                    .link_to_owned_with_source(holder_ref, EdgeKind::Holds, source.clone());
            holder_counts.insert(
                holder_ref.clone(),
                HolderEdge {
                    count: 1,
                    _edge: edge,
                },
            );
        }
    }
}

impl AsEntityRef for Semaphore {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

fn holder_released(
    holder_ref: &mut Option<EntityRef>,
    holder_counts: &Arc<StdMutex<BTreeMap<EntityRef, HolderEdge>>>,
) {
    let Some(holder_ref) = holder_ref.take() else {
        return;
    };
    if let Ok(mut counts) = holder_counts.lock() {
        if let Some(entry) = counts.get_mut(&holder_ref) {
            if entry.count > 1 {
                entry.count -= 1;
            } else {
                counts.remove(&holder_ref);
            }
        }
    }
}

fn sync_state_from_permit(
    semaphore_handle: &WeakEntityHandle<peeps_types::Semaphore>,
    semaphore: &Arc<tokio::sync::Semaphore>,
    max_permits: &Arc<AtomicU32>,
) {
    let max = max_permits.load(Ordering::Relaxed);
    let available = semaphore.available_permits().min(u32::MAX as usize) as u32;
    let handed_out = max.saturating_sub(available);
    let _ = semaphore_handle.mutate(|body| {
        body.max_permits = max;
        body.handed_out_permits = handed_out;
    });
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
        holder_released(&mut self.holder_ref, &self.holder_counts);
        sync_state_from_permit(&self.semaphore_handle, &self.semaphore, &self.max_permits);
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
        holder_released(&mut self.holder_ref, &self.holder_counts);
        sync_state_from_permit(&self.semaphore_handle, &self.semaphore, &self.max_permits);
    }
}
