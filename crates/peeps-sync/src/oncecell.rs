
// ── Diagnostics-enabled implementation ───────────────────

#[cfg(feature = "diagnostics")]
mod diag {
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use peeps_types::{OnceCellSnapshot, OnceCellState};

    const ONCE_EMPTY: u8 = 0;
    const ONCE_INITIALIZING: u8 = 1;
    const ONCE_INITIALIZED: u8 = 2;

    pub(crate) struct OnceCellInfo {
        pub(crate) name: String,
        pub(crate) state: AtomicU8,
        pub(crate) created_at: Instant,
        pub(crate) init_duration: Mutex<Option<std::time::Duration>>,
    }

    impl OnceCellInfo {
        pub(crate) fn snapshot(&self, now: Instant) -> OnceCellSnapshot {
            let state = match self.state.load(Ordering::Relaxed) {
                ONCE_INITIALIZING => OnceCellState::Initializing,
                ONCE_INITIALIZED => OnceCellState::Initialized,
                _ => OnceCellState::Empty,
            };
            OnceCellSnapshot {
                name: self.name.clone(),
                state,
                age_secs: now.duration_since(self.created_at).as_secs_f64(),
                init_duration_secs: self.init_duration.lock().unwrap().map(|d| d.as_secs_f64()),
            }
        }
    }

    pub struct OnceCell<T> {
        inner: tokio::sync::OnceCell<T>,
        info: Arc<OnceCellInfo>,
    }

    impl<T> OnceCell<T> {
        pub fn new(name: impl Into<String>) -> Self {
            let info = Arc::new(OnceCellInfo {
                name: name.into(),
                state: AtomicU8::new(ONCE_EMPTY),
                created_at: Instant::now(),
                init_duration: Mutex::new(None),
            });
            crate::registry::prune_and_register_once_cell(&info);
            Self {
                inner: tokio::sync::OnceCell::new(),
                info,
            }
        }

        pub fn get(&self) -> Option<&T> {
            self.inner.get()
        }

        pub fn initialized(&self) -> bool {
            self.inner.initialized()
        }

        pub async fn get_or_init<F, Fut>(&self, f: F) -> &T
        where
            F: FnOnce() -> Fut,
            Fut: std::future::Future<Output = T>,
        {
            if self.inner.initialized() {
                return self.inner.get().unwrap();
            }

            self.info
                .state
                .compare_exchange(
                    ONCE_EMPTY,
                    ONCE_INITIALIZING,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .ok();
            let start = Instant::now();

            let result = self.inner.get_or_init(f).await;

            if self
                .info
                .state
                .compare_exchange(
                    ONCE_INITIALIZING,
                    ONCE_INITIALIZED,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                *self.info.init_duration.lock().unwrap() = Some(start.elapsed());
            }

            result
        }

        pub async fn get_or_try_init<F, Fut, E>(&self, f: F) -> Result<&T, E>
        where
            F: FnOnce() -> Fut,
            Fut: std::future::Future<Output = Result<T, E>>,
        {
            if self.inner.initialized() {
                return Ok(self.inner.get().unwrap());
            }

            self.info
                .state
                .compare_exchange(
                    ONCE_EMPTY,
                    ONCE_INITIALIZING,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .ok();
            let start = Instant::now();

            let result = self.inner.get_or_try_init(f).await;

            if result.is_ok() {
                if self
                    .info
                    .state
                    .compare_exchange(
                        ONCE_INITIALIZING,
                        ONCE_INITIALIZED,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    *self.info.init_duration.lock().unwrap() = Some(start.elapsed());
                }
            } else {
                // Failed init — revert to empty
                self.info
                    .state
                    .compare_exchange(
                        ONCE_INITIALIZING,
                        ONCE_EMPTY,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .ok();
            }

            result
        }

        pub fn set(&self, value: T) -> Result<(), T> {
            let start = Instant::now();
            self.info
                .state
                .compare_exchange(
                    ONCE_EMPTY,
                    ONCE_INITIALIZING,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .ok();
            match self.inner.set(value) {
                Ok(()) => {
                    self.info
                        .state
                        .store(ONCE_INITIALIZED, Ordering::Relaxed);
                    *self.info.init_duration.lock().unwrap() = Some(start.elapsed());
                    Ok(())
                }
                Err(e) => {
                    // Already initialized, revert our state change
                    self.info
                        .state
                        .compare_exchange(
                            ONCE_INITIALIZING,
                            ONCE_INITIALIZED,
                            Ordering::Relaxed,
                            Ordering::Relaxed,
                        )
                        .ok();
                    match e {
                        tokio::sync::SetError::AlreadyInitializedError(v) => Err(v),
                        tokio::sync::SetError::InitializingError(v) => Err(v),
                    }
                }
            }
        }
    }
}

// ── Zero-cost stub (no diagnostics) ─────────────────────

#[cfg(not(feature = "diagnostics"))]
mod stub {
    pub struct OnceCell<T>(tokio::sync::OnceCell<T>);

    impl<T> OnceCell<T> {
        #[inline]
        pub fn new(_name: impl Into<String>) -> Self {
            Self(tokio::sync::OnceCell::new())
        }

        #[inline]
        pub fn get(&self) -> Option<&T> {
            self.0.get()
        }

        #[inline]
        pub fn initialized(&self) -> bool {
            self.0.initialized()
        }

        #[inline]
        pub async fn get_or_init<F, Fut>(&self, f: F) -> &T
        where
            F: FnOnce() -> Fut,
            Fut: std::future::Future<Output = T>,
        {
            self.0.get_or_init(f).await
        }

        #[inline]
        pub async fn get_or_try_init<F, Fut, E>(&self, f: F) -> Result<&T, E>
        where
            F: FnOnce() -> Fut,
            Fut: std::future::Future<Output = Result<T, E>>,
        {
            self.0.get_or_try_init(f).await
        }

        #[inline]
        pub fn set(&self, value: T) -> Result<(), T> {
            self.0.set(value).map_err(|e| match e {
                tokio::sync::SetError::AlreadyInitializedError(v) => v,
                tokio::sync::SetError::InitializingError(v) => v,
            })
        }
    }
}

// ── Re-exports ──────────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub(crate) use diag::OnceCellInfo;

#[cfg(feature = "diagnostics")]
pub use diag::OnceCell;

#[cfg(not(feature = "diagnostics"))]
pub use stub::OnceCell;
