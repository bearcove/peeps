// ── Diagnostics-enabled lock wrappers ────────────────────

#[cfg(feature = "diagnostics")]
mod diag {
    use std::ops::{Deref, DerefMut};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Instant;

    use crate::registry::{AcquireKind, LockInfo, WaiterOrHolder, WaiterToken};

    // ── DiagnosticRwLock ─────────────────────────────────

    pub struct DiagnosticRwLock<T> {
        inner: parking_lot::RwLock<T>,
        info: Arc<LockInfo>,
    }

    impl<T> DiagnosticRwLock<T> {
        pub fn new(name: &'static str, value: T) -> Self {
            Self {
                inner: parking_lot::RwLock::new(value),
                info: LockInfo::new(name),
            }
        }

        pub fn read(&self) -> DiagnosticRwLockReadGuard<'_, T> {
            let waiter_id = self.info.add_waiter(AcquireKind::Read);
            let guard = self.inner.read();
            let holder_id = self
                .info
                .promote_waiter_to_holder(waiter_id, AcquireKind::Read);
            DiagnosticRwLockReadGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: &self.info,
                holder_id,
            }
        }

        pub fn write(&self) -> DiagnosticRwLockWriteGuard<'_, T> {
            let waiter_id = self.info.add_waiter(AcquireKind::Write);
            let guard = self.inner.write();
            let holder_id = self
                .info
                .promote_waiter_to_holder(waiter_id, AcquireKind::Write);
            DiagnosticRwLockWriteGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: &self.info,
                holder_id,
            }
        }
    }

    pub struct DiagnosticRwLockReadGuard<'a, T> {
        guard: std::mem::ManuallyDrop<parking_lot::RwLockReadGuard<'a, T>>,
        info: &'a Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticRwLockReadGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticRwLockReadGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    pub struct DiagnosticRwLockWriteGuard<'a, T> {
        guard: std::mem::ManuallyDrop<parking_lot::RwLockWriteGuard<'a, T>>,
        info: &'a Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticRwLockWriteGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticRwLockWriteGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    impl<T> DerefMut for DiagnosticRwLockWriteGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }

    // ── DiagnosticMutex ──────────────────────────────────

    pub struct DiagnosticMutex<T> {
        inner: parking_lot::Mutex<T>,
        info: Arc<LockInfo>,
    }

    impl<T> DiagnosticMutex<T> {
        pub fn new(name: &'static str, value: T) -> Self {
            Self {
                inner: parking_lot::Mutex::new(value),
                info: LockInfo::new(name),
            }
        }

        pub fn lock(&self) -> DiagnosticMutexGuard<'_, T> {
            let waiter_id = self.info.add_waiter(AcquireKind::Mutex);
            let guard = self.inner.lock();
            let holder_id = self
                .info
                .promote_waiter_to_holder(waiter_id, AcquireKind::Mutex);
            DiagnosticMutexGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: &self.info,
                holder_id,
            }
        }
    }

    pub struct DiagnosticMutexGuard<'a, T> {
        guard: std::mem::ManuallyDrop<parking_lot::MutexGuard<'a, T>>,
        info: &'a Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticMutexGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticMutexGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    impl<T> DerefMut for DiagnosticMutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }

    // ── DiagnosticAsyncRwLock ────────────────────────────

    pub struct DiagnosticAsyncRwLock<T> {
        inner: tokio::sync::RwLock<T>,
        info: Arc<LockInfo>,
    }

    impl<T> DiagnosticAsyncRwLock<T> {
        pub fn new(name: &'static str, value: T) -> Self {
            Self {
                inner: tokio::sync::RwLock::new(value),
                info: LockInfo::new(name),
            }
        }

        pub async fn read(&self) -> DiagnosticAsyncRwLockReadGuard<'_, T> {
            let mut token = WaiterToken::new(&self.info, AcquireKind::Read);
            let guard = self.inner.read().await;
            let holder_id = self
                .info
                .promote_waiter_to_holder(token.id, AcquireKind::Read);
            token.disarm();
            DiagnosticAsyncRwLockReadGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: Arc::clone(&self.info),
                holder_id,
            }
        }

        pub async fn write(&self) -> DiagnosticAsyncRwLockWriteGuard<'_, T> {
            let mut token = WaiterToken::new(&self.info, AcquireKind::Write);
            let guard = self.inner.write().await;
            let holder_id = self
                .info
                .promote_waiter_to_holder(token.id, AcquireKind::Write);
            token.disarm();
            DiagnosticAsyncRwLockWriteGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: Arc::clone(&self.info),
                holder_id,
            }
        }

        pub fn try_read(
            &self,
        ) -> Result<DiagnosticAsyncRwLockReadGuard<'_, T>, tokio::sync::TryLockError> {
            match self.inner.try_read() {
                Ok(guard) => {
                    let holder_id = {
                        let id = self.info.next_id.fetch_add(1, Ordering::Relaxed);
                        let mut holders = self.info.holders.lock().unwrap();
                        holders.push(WaiterOrHolder {
                            id,
                            kind: AcquireKind::Read,
                            since: Instant::now(),
                            backtrace: std::backtrace::Backtrace::force_capture(),
                            peeps_task_id: peeps_tasks::current_task_id(),
                        });
                        self.info.total_acquires.fetch_add(1, Ordering::Relaxed);
                        id
                    };
                    Ok(DiagnosticAsyncRwLockReadGuard {
                        guard: std::mem::ManuallyDrop::new(guard),
                        info: Arc::clone(&self.info),
                        holder_id,
                    })
                }
                Err(e) => Err(e),
            }
        }

        pub fn try_write(
            &self,
        ) -> Result<DiagnosticAsyncRwLockWriteGuard<'_, T>, tokio::sync::TryLockError> {
            match self.inner.try_write() {
                Ok(guard) => {
                    let holder_id = {
                        let id = self.info.next_id.fetch_add(1, Ordering::Relaxed);
                        let mut holders = self.info.holders.lock().unwrap();
                        holders.push(WaiterOrHolder {
                            id,
                            kind: AcquireKind::Write,
                            since: Instant::now(),
                            backtrace: std::backtrace::Backtrace::force_capture(),
                            peeps_task_id: peeps_tasks::current_task_id(),
                        });
                        self.info.total_acquires.fetch_add(1, Ordering::Relaxed);
                        id
                    };
                    Ok(DiagnosticAsyncRwLockWriteGuard {
                        guard: std::mem::ManuallyDrop::new(guard),
                        info: Arc::clone(&self.info),
                        holder_id,
                    })
                }
                Err(e) => Err(e),
            }
        }
    }

    pub struct DiagnosticAsyncRwLockReadGuard<'a, T> {
        guard: std::mem::ManuallyDrop<tokio::sync::RwLockReadGuard<'a, T>>,
        info: Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticAsyncRwLockReadGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticAsyncRwLockReadGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    pub struct DiagnosticAsyncRwLockWriteGuard<'a, T> {
        guard: std::mem::ManuallyDrop<tokio::sync::RwLockWriteGuard<'a, T>>,
        info: Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticAsyncRwLockWriteGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticAsyncRwLockWriteGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    impl<T> DerefMut for DiagnosticAsyncRwLockWriteGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }

    // ── DiagnosticAsyncMutex ─────────────────────────────

    pub struct DiagnosticAsyncMutex<T> {
        inner: tokio::sync::Mutex<T>,
        info: Arc<LockInfo>,
    }

    impl<T> DiagnosticAsyncMutex<T> {
        pub fn new(name: &'static str, value: T) -> Self {
            Self {
                inner: tokio::sync::Mutex::new(value),
                info: LockInfo::new(name),
            }
        }

        pub async fn lock(&self) -> DiagnosticAsyncMutexGuard<'_, T> {
            let mut token = WaiterToken::new(&self.info, AcquireKind::Mutex);
            let guard = self.inner.lock().await;
            let holder_id = self
                .info
                .promote_waiter_to_holder(token.id, AcquireKind::Mutex);
            token.disarm();
            DiagnosticAsyncMutexGuard {
                guard: std::mem::ManuallyDrop::new(guard),
                info: Arc::clone(&self.info),
                holder_id,
            }
        }

        pub fn try_lock(
            &self,
        ) -> Result<DiagnosticAsyncMutexGuard<'_, T>, tokio::sync::TryLockError> {
            match self.inner.try_lock() {
                Ok(guard) => {
                    let holder_id = {
                        let id = self.info.next_id.fetch_add(1, Ordering::Relaxed);
                        let mut holders = self.info.holders.lock().unwrap();
                        holders.push(WaiterOrHolder {
                            id,
                            kind: AcquireKind::Mutex,
                            since: Instant::now(),
                            backtrace: std::backtrace::Backtrace::force_capture(),
                            peeps_task_id: peeps_tasks::current_task_id(),
                        });
                        self.info.total_acquires.fetch_add(1, Ordering::Relaxed);
                        id
                    };
                    Ok(DiagnosticAsyncMutexGuard {
                        guard: std::mem::ManuallyDrop::new(guard),
                        info: Arc::clone(&self.info),
                        holder_id,
                    })
                }
                Err(e) => Err(e),
            }
        }
    }

    pub struct DiagnosticAsyncMutexGuard<'a, T> {
        guard: std::mem::ManuallyDrop<tokio::sync::MutexGuard<'a, T>>,
        info: Arc<LockInfo>,
        holder_id: u64,
    }

    impl<T> Drop for DiagnosticAsyncMutexGuard<'_, T> {
        fn drop(&mut self) {
            unsafe { std::mem::ManuallyDrop::drop(&mut self.guard) };
            self.info.remove_holder(self.holder_id);
        }
    }

    impl<T> Deref for DiagnosticAsyncMutexGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.guard
        }
    }

    impl<T> DerefMut for DiagnosticAsyncMutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.guard
        }
    }
}

// ── Zero-cost stubs (no diagnostics) ─────────────────────

#[cfg(not(feature = "diagnostics"))]
mod stub {
    pub struct DiagnosticRwLock<T>(parking_lot::RwLock<T>);

    impl<T> DiagnosticRwLock<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(parking_lot::RwLock::new(value))
        }

        #[inline]
        pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, T> {
            self.0.read()
        }

        #[inline]
        pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, T> {
            self.0.write()
        }
    }

    pub struct DiagnosticMutex<T>(parking_lot::Mutex<T>);

    impl<T> DiagnosticMutex<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(parking_lot::Mutex::new(value))
        }

        #[inline]
        pub fn lock(&self) -> parking_lot::MutexGuard<'_, T> {
            self.0.lock()
        }
    }

    pub struct DiagnosticAsyncRwLock<T>(tokio::sync::RwLock<T>);

    impl<T> DiagnosticAsyncRwLock<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(tokio::sync::RwLock::new(value))
        }

        #[inline]
        pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, T> {
            self.0.read().await
        }

        #[inline]
        pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, T> {
            self.0.write().await
        }

        #[inline]
        pub fn try_read(
            &self,
        ) -> Result<tokio::sync::RwLockReadGuard<'_, T>, tokio::sync::TryLockError> {
            self.0.try_read()
        }

        #[inline]
        pub fn try_write(
            &self,
        ) -> Result<tokio::sync::RwLockWriteGuard<'_, T>, tokio::sync::TryLockError> {
            self.0.try_write()
        }
    }

    pub struct DiagnosticAsyncMutex<T>(tokio::sync::Mutex<T>);

    impl<T> DiagnosticAsyncMutex<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(tokio::sync::Mutex::new(value))
        }

        #[inline]
        pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, T> {
            self.0.lock().await
        }

        #[inline]
        pub fn try_lock(
            &self,
        ) -> Result<tokio::sync::MutexGuard<'_, T>, tokio::sync::TryLockError> {
            self.0.try_lock()
        }
    }
}

// ── Re-exports ──────────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub use diag::*;

#[cfg(not(feature = "diagnostics"))]
pub use stub::*;
