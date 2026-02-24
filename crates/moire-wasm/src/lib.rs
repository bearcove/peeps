//! WASM runtime surface for moire.
//!
//! Instrumentation is intentionally no-op on wasm.

use std::future::Future;

/// Task utilities matching `moire::task` on native.
pub mod task {
    use std::future::Future;

    /// No-op extension trait matching the native `FutureExt`.
    pub trait FutureExt: Future + Sized {
        fn named(self, _name: impl Into<String>) -> Self {
            self
        }
    }

    impl<F: Future + Sized> FutureExt for F {}
}

/// Sync primitives matching `moire::sync` on native.
pub mod sync {
    /// Wrapper around `std::sync::Mutex` with the same constructor shape as native moire.
    pub struct Mutex<T>(std::sync::Mutex<T>);

    impl<T> Mutex<T> {
        #[inline]
        pub fn new(_name: &'static str, value: T) -> Self {
            Self(std::sync::Mutex::new(value))
        }

        #[inline]
        pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
            self.0.lock().expect("wasm mutex poisoned; cannot continue")
        }
    }

    /// Error returned when sending fails because the receiver was dropped.
    pub struct SendError<T>(pub T);

    impl<T> std::fmt::Debug for SendError<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("SendError").finish_non_exhaustive()
        }
    }

    impl<T> std::fmt::Display for SendError<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "channel closed")
        }
    }

    impl<T> std::error::Error for SendError<T> {}

    /// Error returned when `try_send` fails.
    pub enum TrySendError<T> {
        /// Channel is full.
        Full(T),
        /// Channel is closed.
        Closed(T),
    }

    impl<T> std::fmt::Debug for TrySendError<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Full(_) => f.debug_struct("TrySendError::Full").finish_non_exhaustive(),
                Self::Closed(_) => f
                    .debug_struct("TrySendError::Closed")
                    .finish_non_exhaustive(),
            }
        }
    }

    /// Wrapper around `async-channel::Sender`.
    pub struct Sender<T>(async_channel::Sender<T>);

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> Sender<T> {
        pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
            self.0.send(value).await.map_err(|e| SendError(e.0))
        }

        pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
            self.0.try_send(value).map_err(|e| match e {
                async_channel::TrySendError::Full(v) => TrySendError::Full(v),
                async_channel::TrySendError::Closed(v) => TrySendError::Closed(v),
            })
        }

        pub fn is_closed(&self) -> bool {
            self.0.is_closed()
        }
    }

    /// Wrapper around `async-channel::Receiver`.
    pub struct Receiver<T>(async_channel::Receiver<T>);

    impl<T> Receiver<T> {
        pub async fn recv(&mut self) -> Option<T> {
            self.0.recv().await.ok()
        }
    }

    /// Instrumented mpsc channel primitives (wasm no-op backend).
    pub mod mpsc {
        pub use super::{Receiver, Sender};

        /// Unbounded sender — alias for [`Sender`].
        pub type UnboundedSender<T> = Sender<T>;
        /// Unbounded receiver — alias for [`Receiver`].
        pub type UnboundedReceiver<T> = Receiver<T>;

        /// Create a bounded mpsc channel.
        pub fn channel<T>(_name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
            let (tx, rx) = async_channel::bounded(buffer);
            (Sender(tx), Receiver(rx))
        }

        /// Create an unbounded mpsc channel.
        pub fn unbounded_channel<T>(
            _name: impl Into<String>,
        ) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
            let (tx, rx) = async_channel::unbounded();
            (Sender(tx), Receiver(rx))
        }
    }

    /// Error returned when acquiring a permit from a closed semaphore.
    #[derive(Debug)]
    pub struct AcquireError(());

    impl std::fmt::Display for AcquireError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("semaphore closed")
        }
    }

    impl std::error::Error for AcquireError {}

    /// Error returned when `try_acquire` fails.
    #[derive(Debug, PartialEq, Eq)]
    pub enum TryAcquireError {
        /// Semaphore is closed.
        Closed,
        /// No permits are available.
        NoPermits,
    }

    impl std::fmt::Display for TryAcquireError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Closed => f.write_str("semaphore closed"),
                Self::NoPermits => f.write_str("no permits available"),
            }
        }
    }

    impl std::error::Error for TryAcquireError {}

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct SemaphoreInner {
        sem: Arc<async_lock::Semaphore>,
        closed: AtomicBool,
        available: AtomicUsize,
    }

    /// Owned permit returned by [`Semaphore::acquire_owned`].
    pub struct OwnedSemaphorePermit {
        _guard: async_lock::SemaphoreGuardArc,
        inner: Arc<SemaphoreInner>,
    }

    impl Drop for OwnedSemaphorePermit {
        fn drop(&mut self) {
            self.inner.available.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Borrowed permit returned by [`Semaphore::acquire`].
    pub struct SemaphorePermit<'a> {
        _guard: async_lock::SemaphoreGuard<'a>,
        inner: Arc<SemaphoreInner>,
    }

    impl Drop for SemaphorePermit<'_> {
        fn drop(&mut self) {
            self.inner.available.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Async semaphore (wasm backend via async-lock).
    #[derive(Clone)]
    pub struct Semaphore(Arc<SemaphoreInner>);

    impl Semaphore {
        pub fn new(_name: impl Into<String>, permits: usize) -> Self {
            Self(Arc::new(SemaphoreInner {
                sem: Arc::new(async_lock::Semaphore::new(permits)),
                closed: AtomicBool::new(false),
                available: AtomicUsize::new(permits),
            }))
        }

        pub fn available_permits(&self) -> usize {
            self.0.available.load(Ordering::Relaxed)
        }

        pub fn close(&self) {
            self.0.closed.store(true, Ordering::Release);
        }

        pub fn is_closed(&self) -> bool {
            self.0.closed.load(Ordering::Acquire)
        }

        pub fn add_permits(&self, n: usize) {
            self.0.sem.add_permits(n);
            self.0.available.fetch_add(n, Ordering::Relaxed);
        }

        pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, AcquireError> {
            if self.0.closed.load(Ordering::Acquire) {
                return Err(AcquireError(()));
            }
            self.0.available.fetch_sub(1, Ordering::Relaxed);
            Ok(SemaphorePermit {
                _guard: self.0.sem.acquire().await,
                inner: Arc::clone(&self.0),
            })
        }

        pub async fn acquire_owned(&self) -> Result<OwnedSemaphorePermit, AcquireError> {
            if self.0.closed.load(Ordering::Acquire) {
                return Err(AcquireError(()));
            }
            self.0.available.fetch_sub(1, Ordering::Relaxed);
            Ok(OwnedSemaphorePermit {
                _guard: Arc::clone(&self.0.sem).acquire_arc().await,
                inner: Arc::clone(&self.0),
            })
        }

        pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, TryAcquireError> {
            if self.0.closed.load(Ordering::Acquire) {
                return Err(TryAcquireError::Closed);
            }
            match self.0.sem.try_acquire() {
                Some(guard) => {
                    self.0.available.fetch_sub(1, Ordering::Relaxed);
                    Ok(SemaphorePermit {
                        _guard: guard,
                        inner: Arc::clone(&self.0),
                    })
                }
                None => Err(TryAcquireError::NoPermits),
            }
        }

        pub fn try_acquire_owned(&self) -> Result<OwnedSemaphorePermit, TryAcquireError> {
            if self.0.closed.load(Ordering::Acquire) {
                return Err(TryAcquireError::Closed);
            }
            match Arc::clone(&self.0.sem).try_acquire_arc() {
                Some(guard) => {
                    self.0.available.fetch_sub(1, Ordering::Relaxed);
                    Ok(OwnedSemaphorePermit {
                        _guard: guard,
                        inner: Arc::clone(&self.0),
                    })
                }
                None => Err(TryAcquireError::NoPermits),
            }
        }
    }

    /// Instrumented oneshot channel primitives (wasm no-op backend).
    pub mod oneshot {
        pub mod error {
            pub use futures_channel::oneshot::Canceled as RecvError;
        }

        /// Oneshot sender.
        pub use futures_channel::oneshot::Sender;

        /// Wrapper around `futures-channel` oneshot receiver for API parity.
        pub struct Receiver<T>(futures_channel::oneshot::Receiver<T>);

        impl<T> std::future::IntoFuture for Receiver<T> {
            type Output = Result<T, futures_channel::oneshot::Canceled>;
            type IntoFuture = futures_channel::oneshot::Receiver<T>;

            fn into_future(self) -> Self::IntoFuture {
                self.0
            }
        }

        impl<T> Receiver<T> {
            pub fn try_recv(&mut self) -> Result<Option<T>, futures_channel::oneshot::Canceled> {
                self.0.try_recv()
            }
        }

        /// Create a oneshot channel.
        pub fn channel<T>(_name: impl Into<String>) -> (Sender<T>, Receiver<T>) {
            let (tx, rx) = futures_channel::oneshot::channel();
            (tx, Receiver(rx))
        }
    }
}

/// Custom entity and event support (wasm no-op backend).
pub mod custom {
    pub use moire_types::{CustomEntity, CustomEventKind, EntityBody, EventTarget, Json};

    /// No-op entity handle for custom entities on wasm.
    #[derive(Clone)]
    pub struct CustomEntityHandle;

    impl CustomEntityHandle {
        pub fn new(_name: impl Into<String>, _body: CustomEntity) -> Self {
            Self
        }

        pub fn mutate(&self, _f: impl FnOnce(&mut CustomEntity)) -> bool {
            false
        }

        pub fn emit_event(
            &self,
            _kind: impl Into<String>,
            _display_name: impl Into<String>,
            _payload: Json,
        ) {
        }
    }

    pub fn record_custom_event(
        _target: EventTarget,
        _kind: impl Into<String>,
        _display_name: impl Into<String>,
        _payload: Json,
    ) {
    }
}

/// Time utilities matching `moire::time` on native.
pub mod time {
    use std::future::Future;
    use std::time::Duration;

    /// Timeout error, equivalent to `tokio::time::error::Elapsed`.
    pub mod error {
        /// Error returned when a timeout has elapsed.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Elapsed;

        impl std::fmt::Display for Elapsed {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("deadline has elapsed")
            }
        }

        impl std::error::Error for Elapsed {}
    }

    /// Sleep for a duration, equivalent to `tokio::time::sleep`.
    pub async fn sleep(duration: Duration) {
        gloo_timers::future::sleep(duration).await;
    }

    /// Run a future with a timeout.
    pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, error::Elapsed>
    where
        F: Future<Output = T>,
    {
        use futures_util::future::{Either, select};
        use std::pin::pin;

        let sleep_fut = pin!(gloo_timers::future::sleep(duration));
        let work_fut = pin!(future);

        match select(work_fut, sleep_fut).await {
            Either::Left((result, _)) => Ok(result),
            Either::Right((_, _)) => Err(error::Elapsed),
        }
    }
}

/// Spawn a task concurrently on the browser executor, equivalent to `moire::spawn`.
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}
