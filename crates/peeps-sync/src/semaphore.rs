
// ── Diagnostics-enabled implementation ───────────────────

#[cfg(feature = "diagnostics")]
mod diag {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use peeps_types::SemaphoreSnapshot;

    pub(crate) struct WaiterEntry {
        pub(crate) task_id: u64,
        pub(crate) started_at: Instant,
    }

    pub(crate) struct SemaphoreInfo {
        pub(crate) name: String,
        pub(crate) permits_total: u64,
        pub(crate) waiters: AtomicU64,
        pub(crate) acquires: AtomicU64,
        pub(crate) total_wait_nanos: AtomicU64,
        pub(crate) max_wait_nanos: AtomicU64,
        pub(crate) high_waiters_watermark: AtomicU64,
        pub(crate) created_at: Instant,
        pub(crate) creator_task_id: Option<u64>,
        pub(crate) available_permits: Box<dyn Fn() -> usize + Send + Sync>,
        pub(crate) active_waiters: Mutex<Vec<WaiterEntry>>,
    }

    impl SemaphoreInfo {
        pub(crate) fn snapshot(&self, now: Instant) -> SemaphoreSnapshot {
            let acquires = self.acquires.load(Ordering::Relaxed);
            let total_wait_nanos = self.total_wait_nanos.load(Ordering::Relaxed);
            let avg_wait_secs = if acquires == 0 {
                0.0
            } else {
                (total_wait_nanos as f64 / acquires as f64) / 1_000_000_000.0
            };
            let (top_waiter_task_ids, oldest_wait_secs) = {
                let waiters = self.active_waiters.lock().unwrap();
                let ids: Vec<u64> = waiters.iter().map(|w| w.task_id).collect();
                let oldest = waiters
                    .iter()
                    .map(|w| now.duration_since(w.started_at).as_secs_f64())
                    .fold(0.0_f64, f64::max);
                (ids, oldest)
            };
            SemaphoreSnapshot {
                name: self.name.clone(),
                permits_total: self.permits_total,
                permits_available: (self.available_permits)() as u64,
                waiters: self.waiters.load(Ordering::Relaxed),
                acquires,
                avg_wait_secs,
                max_wait_secs: self.max_wait_nanos.load(Ordering::Relaxed) as f64
                    / 1_000_000_000.0,
                age_secs: now.duration_since(self.created_at).as_secs_f64(),
                creator_task_id: self.creator_task_id,
                creator_task_name: self.creator_task_id.and_then(peeps_tasks::task_name),
                top_waiter_task_ids,
                oldest_wait_secs,
            }
        }
    }

    fn update_max_wait(max_wait_nanos: &AtomicU64, observed: u64) {
        let mut current = max_wait_nanos.load(Ordering::Relaxed);
        while observed > current {
            match max_wait_nanos.compare_exchange_weak(
                current,
                observed,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(next) => current = next,
            }
        }
    }

    pub struct DiagnosticSemaphore {
        inner: Arc<tokio::sync::Semaphore>,
        info: Arc<SemaphoreInfo>,
    }

    impl Clone for DiagnosticSemaphore {
        fn clone(&self) -> Self {
            Self {
                inner: Arc::clone(&self.inner),
                info: Arc::clone(&self.info),
            }
        }
    }

    impl DiagnosticSemaphore {
        pub fn new(name: impl Into<String>, permits: usize) -> Self {
            let inner = Arc::new(tokio::sync::Semaphore::new(permits));
            let inner_for_snapshot = Arc::clone(&inner);
            let info = Arc::new(SemaphoreInfo {
                name: name.into(),
                permits_total: permits as u64,
                waiters: AtomicU64::new(0),
                acquires: AtomicU64::new(0),
                total_wait_nanos: AtomicU64::new(0),
                max_wait_nanos: AtomicU64::new(0),
                high_waiters_watermark: AtomicU64::new(0),
                created_at: Instant::now(),
                creator_task_id: peeps_tasks::current_task_id(),
                available_permits: Box::new(move || inner_for_snapshot.available_permits()),
                active_waiters: Mutex::new(Vec::new()),
            });
            crate::registry::prune_and_register_semaphore(&info);
            Self { inner, info }
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
        }

        pub async fn acquire(
            &self,
        ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
            let task_id = peeps_tasks::current_task_id().unwrap_or(0);
            let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
            update_max_wait(&self.info.high_waiters_watermark, new_waiters);
            let start = Instant::now();
            self.info
                .active_waiters
                .lock()
                .unwrap()
                .push(WaiterEntry {
                    task_id,
                    started_at: start,
                });
            let result = self.inner.acquire().await;
            self.info.waiters.fetch_sub(1, Ordering::Relaxed);
            {
                let mut waiters = self.info.active_waiters.lock().unwrap();
                if let Some(pos) = waiters
                    .iter()
                    .position(|w| w.task_id == task_id && w.started_at == start)
                {
                    waiters.swap_remove(pos);
                }
            }
            if result.is_ok() {
                self.info.acquires.fetch_add(1, Ordering::Relaxed);
                let waited_nanos = start.elapsed().as_nanos() as u64;
                self.info
                    .total_wait_nanos
                    .fetch_add(waited_nanos, Ordering::Relaxed);
                update_max_wait(&self.info.max_wait_nanos, waited_nanos);
            }
            result
        }

        pub async fn acquire_many(
            &self,
            n: u32,
        ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
            let task_id = peeps_tasks::current_task_id().unwrap_or(0);
            let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
            update_max_wait(&self.info.high_waiters_watermark, new_waiters);
            let start = Instant::now();
            self.info
                .active_waiters
                .lock()
                .unwrap()
                .push(WaiterEntry {
                    task_id,
                    started_at: start,
                });
            let result = self.inner.acquire_many(n).await;
            self.info.waiters.fetch_sub(1, Ordering::Relaxed);
            {
                let mut waiters = self.info.active_waiters.lock().unwrap();
                if let Some(pos) = waiters
                    .iter()
                    .position(|w| w.task_id == task_id && w.started_at == start)
                {
                    waiters.swap_remove(pos);
                }
            }
            if result.is_ok() {
                self.info.acquires.fetch_add(1, Ordering::Relaxed);
                let waited_nanos = start.elapsed().as_nanos() as u64;
                self.info
                    .total_wait_nanos
                    .fetch_add(waited_nanos, Ordering::Relaxed);
                update_max_wait(&self.info.max_wait_nanos, waited_nanos);
            }
            result
        }

        pub async fn acquire_owned(
            &self,
        ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
            let task_id = peeps_tasks::current_task_id().unwrap_or(0);
            let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
            update_max_wait(&self.info.high_waiters_watermark, new_waiters);
            let start = Instant::now();
            self.info
                .active_waiters
                .lock()
                .unwrap()
                .push(WaiterEntry {
                    task_id,
                    started_at: start,
                });
            let result = Arc::clone(&self.inner).acquire_owned().await;
            self.info.waiters.fetch_sub(1, Ordering::Relaxed);
            {
                let mut waiters = self.info.active_waiters.lock().unwrap();
                if let Some(pos) = waiters
                    .iter()
                    .position(|w| w.task_id == task_id && w.started_at == start)
                {
                    waiters.swap_remove(pos);
                }
            }
            if result.is_ok() {
                self.info.acquires.fetch_add(1, Ordering::Relaxed);
                let waited_nanos = start.elapsed().as_nanos() as u64;
                self.info
                    .total_wait_nanos
                    .fetch_add(waited_nanos, Ordering::Relaxed);
                update_max_wait(&self.info.max_wait_nanos, waited_nanos);
            }
            result
        }

        pub async fn acquire_many_owned(
            &self,
            n: u32,
        ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
            let task_id = peeps_tasks::current_task_id().unwrap_or(0);
            let new_waiters = self.info.waiters.fetch_add(1, Ordering::Relaxed) + 1;
            update_max_wait(&self.info.high_waiters_watermark, new_waiters);
            let start = Instant::now();
            self.info
                .active_waiters
                .lock()
                .unwrap()
                .push(WaiterEntry {
                    task_id,
                    started_at: start,
                });
            let result = Arc::clone(&self.inner).acquire_many_owned(n).await;
            self.info.waiters.fetch_sub(1, Ordering::Relaxed);
            {
                let mut waiters = self.info.active_waiters.lock().unwrap();
                if let Some(pos) = waiters
                    .iter()
                    .position(|w| w.task_id == task_id && w.started_at == start)
                {
                    waiters.swap_remove(pos);
                }
            }
            if result.is_ok() {
                self.info.acquires.fetch_add(1, Ordering::Relaxed);
                let waited_nanos = start.elapsed().as_nanos() as u64;
                self.info
                    .total_wait_nanos
                    .fetch_add(waited_nanos, Ordering::Relaxed);
                update_max_wait(&self.info.max_wait_nanos, waited_nanos);
            }
            result
        }

        pub fn try_acquire(
            &self,
        ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
            let result = self.inner.try_acquire();
            if result.is_ok() {
                self.info.acquires.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn try_acquire_many(
            &self,
            n: u32,
        ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
            let result = self.inner.try_acquire_many(n);
            if result.is_ok() {
                self.info.acquires.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn try_acquire_owned(
            &self,
        ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
            let result = Arc::clone(&self.inner).try_acquire_owned();
            if result.is_ok() {
                self.info.acquires.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn try_acquire_many_owned(
            &self,
            n: u32,
        ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
            let result = Arc::clone(&self.inner).try_acquire_many_owned(n);
            if result.is_ok() {
                self.info.acquires.fetch_add(1, Ordering::Relaxed);
            }
            result
        }
    }
}

// ── Zero-cost stub (no diagnostics) ─────────────────────

#[cfg(not(feature = "diagnostics"))]
mod stub {
    pub struct DiagnosticSemaphore(std::sync::Arc<tokio::sync::Semaphore>);

    impl Clone for DiagnosticSemaphore {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl DiagnosticSemaphore {
        #[inline]
        pub fn new(_name: impl Into<String>, permits: usize) -> Self {
            Self(std::sync::Arc::new(tokio::sync::Semaphore::new(permits)))
        }

        #[inline]
        pub fn available_permits(&self) -> usize {
            self.0.available_permits()
        }

        #[inline]
        pub fn close(&self) {
            self.0.close()
        }

        #[inline]
        pub fn is_closed(&self) -> bool {
            self.0.is_closed()
        }

        #[inline]
        pub fn add_permits(&self, n: usize) {
            self.0.add_permits(n)
        }

        #[inline]
        pub async fn acquire(
            &self,
        ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
            self.0.acquire().await
        }

        #[inline]
        pub async fn acquire_many(
            &self,
            n: u32,
        ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
            self.0.acquire_many(n).await
        }

        #[inline]
        pub async fn acquire_owned(
            &self,
        ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
            self.0.clone().acquire_owned().await
        }

        #[inline]
        pub async fn acquire_many_owned(
            &self,
            n: u32,
        ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
            self.0.clone().acquire_many_owned(n).await
        }

        #[inline]
        pub fn try_acquire(
            &self,
        ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
            self.0.try_acquire()
        }

        #[inline]
        pub fn try_acquire_many(
            &self,
            n: u32,
        ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
            self.0.try_acquire_many(n)
        }

        #[inline]
        pub fn try_acquire_owned(
            &self,
        ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
            self.0.clone().try_acquire_owned()
        }

        #[inline]
        pub fn try_acquire_many_owned(
            &self,
            n: u32,
        ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
            self.0.clone().try_acquire_many_owned(n)
        }
    }
}

// ── Re-exports ──────────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) use diag::SemaphoreInfo;

#[cfg(feature = "diagnostics")]
pub use diag::DiagnosticSemaphore;

#[cfg(not(feature = "diagnostics"))]
pub use stub::DiagnosticSemaphore;
