// r[impl api.semaphore]
use moire_types::{EdgeKind, SemaphoreEntity};
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use moire_runtime::{
    current_causal_target, instrument_operation_on, AsEntityRef, EdgeHandle,
    EntityHandle, EntityRef, WeakEntityHandle,
};

#[derive(Clone)]
/// Instrumented version of [`tokio::sync::Semaphore`].
pub struct Semaphore {
    inner: Arc<tokio::sync::Semaphore>,
    handle: EntityHandle<moire_types::Semaphore>,
    max_permits: Arc<AtomicU32>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityRef, HolderEdge>>>,
}

struct HolderEdge {
    count: u32,
    _edge: EdgeHandle,
}

/// Instrumented equivalent of [`tokio::sync::SemaphorePermit`].
pub struct SemaphorePermit<'a> {
    inner: Option<tokio::sync::SemaphorePermit<'a>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    semaphore_handle: WeakEntityHandle<moire_types::Semaphore>,
    holder_ref: Option<EntityRef>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityRef, HolderEdge>>>,
    max_permits: Arc<AtomicU32>,
}

/// Instrumented equivalent of [`tokio::sync::OwnedSemaphorePermit`].
pub struct OwnedSemaphorePermit {
    inner: Option<tokio::sync::OwnedSemaphorePermit>,
    semaphore: Arc<tokio::sync::Semaphore>,
    semaphore_handle: WeakEntityHandle<moire_types::Semaphore>,
    holder_ref: Option<EntityRef>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityRef, HolderEdge>>>,
    max_permits: Arc<AtomicU32>,
}

impl Semaphore {
    /// Creates a new semaphore, matching [`tokio::sync::Semaphore::new`].
    pub fn new(name: impl Into<String>, permits: usize) -> Self {
        let max_permits = permits.min(u32::MAX as usize) as u32;
        let handle = EntityHandle::new(
            name.into(),
            SemaphoreEntity {
                max_permits,
                handed_out_permits: 0,
            },
        );
        Self {
            inner: Arc::new(tokio::sync::Semaphore::new(permits)),
            handle,
            max_permits: Arc::new(AtomicU32::new(max_permits)),
            holder_counts: Arc::new(StdMutex::new(BTreeMap::new())),
        }
    }

    /// Returns available permits, matching [`tokio::sync::Semaphore::available_permits`].
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    /// Closes the semaphore, matching [`tokio::sync::Semaphore::close`].
    pub fn close(&self) {
        self.inner.close();
    }

    /// Returns whether the semaphore is closed, matching [`tokio::sync::Semaphore::is_closed`].
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Adds permits, equivalent to [`tokio::sync::Semaphore::add_permits`].
    pub fn add_permits(&self, n: usize) {
        self.inner.add_permits(n);
        let delta = n.min(u32::MAX as usize) as u32;
        let max = self
            .max_permits
            .fetch_add(delta, Ordering::Relaxed)
            .saturating_add(delta);
        self.sync_state(max);
    }
    /// Acquires a permit asynchronously, matching [`tokio::sync::Semaphore::acquire`].
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
                let holder_ref = current_causal_target();
        let permit =
            instrument_operation_on(&self.handle, self.inner.acquire()).await?;
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
    /// Acquires multiple permits asynchronously, matching [`tokio::sync::Semaphore::acquire_many`].
    pub async fn acquire_many(
        &self,
        n: u32,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
                let holder_ref = current_causal_target();
        let permit =
            instrument_operation_on(&self.handle, self.inner.acquire_many(n))
                .await?;
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
    /// Acquires an owned permit asynchronously, matching [`tokio::sync::Semaphore::acquire_owned`].
    pub async fn acquire_owned(&self) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
                let holder_ref = current_causal_target();
        let permit = instrument_operation_on(
            &self.handle,
            Arc::clone(&self.inner).acquire_owned(), 
        )
        .await?;
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
    /// Acquires multiple owned permits asynchronously, matching [`tokio::sync::Semaphore::acquire_many_owned`].
    pub async fn acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
                let holder_ref = current_causal_target();
        let permit = instrument_operation_on(
            &self.handle,
            Arc::clone(&self.inner).acquire_many_owned(n), 
        )
        .await?;
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

    /// Tries to acquire a permit immediately, matching [`tokio::sync::Semaphore::try_acquire`].
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire()?;
        let holder_ref = current_causal_target();
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

    /// Tries to acquire multiple permits immediately, matching [`tokio::sync::Semaphore::try_acquire_many`].
    pub fn try_acquire_many(
        &self,
        n: u32,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire_many(n)?;
        let holder_ref = current_causal_target();
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

    /// Tries to acquire an owned permit immediately, matching [`tokio::sync::Semaphore::try_acquire_owned`].
    pub fn try_acquire_owned(&self) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let permit = Arc::clone(&self.inner).try_acquire_owned()?;
        let holder_ref = current_causal_target();
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

    /// Tries to acquire multiple owned permits immediately, matching [`tokio::sync::Semaphore::try_acquire_many_owned`].
    pub fn try_acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let permit = Arc::clone(&self.inner).try_acquire_many_owned(n)?;
        let holder_ref = current_causal_target();
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
            let edge = self
                .handle
                .link_to_owned(holder_ref, EdgeKind::Holds);
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
    semaphore_handle: &WeakEntityHandle<moire_types::Semaphore>,
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
