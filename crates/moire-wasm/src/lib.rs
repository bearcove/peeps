//! WASM runtime surface for moire.
//!
//! Instrumentation is intentionally no-op on wasm.

use std::future::Future;

#[cfg(feature = "diagnostics")]
#[doc(hidden)]
pub mod __internal {
    use std::future::{Future, IntoFuture};
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub struct InstrumentedFuture<F>(F);

    impl<F: Future> Future for InstrumentedFuture<F> {
        type Output = F::Output;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().0) }.poll(cx)
        }
    }

    impl<F> InstrumentedFuture<F> {
        pub fn skip_entry_frames(self, _n: u8) -> Self {
            self
        }
    }

    pub fn instrument_future<F, O, M>(
        _name: impl Into<String>,
        fut: F,
        _on: Option<O>,
        _meta: Option<M>,
    ) -> InstrumentedFuture<F::IntoFuture>
    where
        F: IntoFuture,
    {
        InstrumentedFuture(fut.into_future())
    }
}

/// Task utilities matching `moire::task` on native.
pub mod task {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// No-op extension trait matching the native `FutureExt`.
    pub trait FutureExt: Future + Sized {
        fn named(self, _name: impl Into<String>) -> Self {
            self
        }
    }

    impl<F: Future + Sized> FutureExt for F {}

    /// Wasm equivalent of `tokio::task::JoinHandle`.
    ///
    /// On wasm, `spawn_local` returns `()` so we wrap a oneshot receiver.
    /// The task sends `()` when it completes; awaiting the handle waits for that.
    pub struct JoinHandle<T> {
        rx: futures_channel::oneshot::Receiver<T>,
    }

    impl<T> JoinHandle<T> {
        /// No-op rename for API parity.
        pub fn named(self, _name: impl Into<String>) -> Self {
            self
        }

        pub fn abort(&self) {
            // No abort support on wasm; tasks run to completion.
        }
    }

    impl<T> Future for JoinHandle<T> {
        type Output = Result<T, futures_channel::oneshot::Canceled>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let rx = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().rx) };
            rx.poll(cx)
        }
    }

    impl<T> std::fmt::Debug for JoinHandle<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("JoinHandle").finish_non_exhaustive()
        }
    }

    /// Spawn a task and return a [`JoinHandle`].
    pub fn spawn<F, T>(future: F) -> JoinHandle<T>
    where
        F: Future<Output = T> + 'static,
        T: 'static,
    {
        let (tx, rx) = futures_channel::oneshot::channel();
        wasm_bindgen_futures::spawn_local(async move {
            let result = future.await;
            let _ = tx.send(result);
        });
        JoinHandle { rx }
    }
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

    /// Synchronous mutex — on wasm, identical to [`Mutex`].
    ///
    /// On native this uses `parking_lot::Mutex` for lock-free fast path; on wasm
    /// `std::sync::Mutex` is fine since there is no thread contention.
    pub type SyncMutex<T> = Mutex<T>;
    pub type SyncMutexGuard<'a, T> = std::sync::MutexGuard<'a, T>;

    /// Async notify primitive (wasm backend via `event-listener`).
    #[derive(Clone)]
    pub struct Notify(std::sync::Arc<event_listener::Event>);

    impl Notify {
        pub fn new(_name: impl Into<String>) -> Self {
            Self(std::sync::Arc::new(event_listener::Event::new()))
        }

        pub async fn notified(&self) {
            self.0.listen().await;
        }

        pub fn notify_one(&self) {
            self.0.notify(1);
        }

        pub fn notify_waiters(&self) {
            self.0.notify(usize::MAX);
        }
    }

    impl std::fmt::Debug for Notify {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Notify").finish_non_exhaustive()
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

    /// Mpsc sender — either bounded (slot-reserving) or unbounded.
    pub enum Sender<T> {
        Bounded(mpsc::bounded::Sender<T>),
        Unbounded(async_channel::Sender<T>),
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            match self {
                Self::Bounded(s) => Self::Bounded(s.clone()),
                Self::Unbounded(s) => Self::Unbounded(s.clone()),
            }
        }
    }

    impl<T> std::fmt::Debug for Sender<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Sender").finish_non_exhaustive()
        }
    }

    impl<T> Sender<T> {
        pub async fn send(&self, value: T) -> Result<(), SendError<T>> {
            match self {
                Self::Bounded(s) => s.send(value).await,
                Self::Unbounded(s) => s.send(value).await.map_err(|e| SendError(e.0)),
            }
        }

        pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
            match self {
                Self::Bounded(s) => s.try_send(value),
                Self::Unbounded(s) => s.try_send(value).map_err(|e| match e {
                    async_channel::TrySendError::Full(v) => TrySendError::Full(v),
                    async_channel::TrySendError::Closed(v) => TrySendError::Closed(v),
                }),
            }
        }

        pub fn is_closed(&self) -> bool {
            match self {
                Self::Bounded(s) => s.is_closed(),
                Self::Unbounded(s) => s.is_closed(),
            }
        }

        pub async fn reserve_owned(self) -> Result<mpsc::OwnedPermit<T>, SendError<()>> {
            match self {
                Self::Bounded(s) => s
                    .reserve_owned()
                    .await
                    .map(|p| mpsc::OwnedPermit(p))
                    .map_err(|_| SendError(())),
                Self::Unbounded(_) => panic!("reserve_owned called on unbounded channel"),
            }
        }
    }

    /// Mpsc receiver — either bounded (slot-reserving) or unbounded.
    pub enum Receiver<T> {
        Bounded(mpsc::bounded::Receiver<T>),
        Unbounded(async_channel::Receiver<T>),
    }

    impl<T> Receiver<T> {
        pub async fn recv(&mut self) -> Option<T> {
            match self {
                Self::Bounded(r) => r.recv().await,
                Self::Unbounded(r) => r.recv().await.ok(),
            }
        }
    }

    /// Instrumented mpsc channel primitives (wasm backend).
    ///
    /// Bounded channels use a hand-rolled slot-reserving implementation so that
    /// `OwnedPermit` has real semantics: `reserve_owned()` claims a slot in the
    /// queue and the permit holder is guaranteed that `send()` will not block.
    /// This works because wasm is single-threaded — `Rc<RefCell<...>>` suffices.
    pub mod mpsc {
        pub use super::{Receiver, SendError, Sender};

        /// Unbounded sender — alias for [`Sender`].
        pub type UnboundedSender<T> = Sender<T>;
        /// Unbounded receiver — alias for [`Receiver`].
        pub type UnboundedReceiver<T> = Receiver<T>;

        /// Create a bounded mpsc channel with slot-reserving semantics.
        pub fn channel<T>(_name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
            bounded::channel(buffer)
        }

        /// Create an unbounded mpsc channel.
        pub fn unbounded_channel<T>(
            _name: impl Into<String>,
        ) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
            let (tx, rx) = async_channel::unbounded();
            (Sender::Unbounded(tx), Receiver::Unbounded(rx))
        }

        /// An owned send permit — holds a reserved slot in the channel.
        ///
        /// Created by [`Sender::reserve_owned`]. Sending consumes the permit and
        /// releases the reserved slot.
        pub struct OwnedPermit<T>(pub(super) bounded::OwnedPermit<T>);

        impl<T> OwnedPermit<T> {
            /// Send a value using this permit, consuming it.
            pub fn send(self, value: T) -> Result<(), SendError<T>> {
                self.0.send(value)
            }
        }

        pub(super) mod bounded {
            use std::cell::RefCell;
            use std::collections::VecDeque;
            use std::future::Future;
            use std::pin::Pin;
            use std::rc::Rc;
            use std::task::{Context, Poll, Waker};

            struct Inner<T> {
                buf: VecDeque<T>,
                /// Slots claimed by outstanding OwnedPermits but not yet sent.
                reserved: usize,
                capacity: usize,
                /// Set when all Senders are dropped.
                closed: bool,
                recv_waker: Option<Waker>,
                send_wakers: VecDeque<Waker>,
            }

            impl<T> Inner<T> {
                fn available(&self) -> usize {
                    self.capacity.saturating_sub(self.buf.len() + self.reserved)
                }
            }

            pub struct Sender<T> {
                inner: Rc<RefCell<Inner<T>>>,
            }

            impl<T> Clone for Sender<T> {
                fn clone(&self) -> Self {
                    Self {
                        inner: Rc::clone(&self.inner),
                    }
                }
            }

            impl<T> Drop for Sender<T> {
                fn drop(&mut self) {
                    // Only close when the last Sender drops (Rc count hits 1 = just Receiver).
                    if Rc::strong_count(&self.inner) == 2 {
                        let mut inner = self.inner.borrow_mut();
                        inner.closed = true;
                        if let Some(w) = inner.recv_waker.take() {
                            w.wake();
                        }
                    }
                }
            }

            impl<T> Sender<T> {
                pub async fn send(&self, value: T) -> Result<(), super::SendError<T>> {
                    let reserve = ReserveFuture {
                        inner: Rc::clone(&self.inner),
                    };
                    if reserve.await.is_err() {
                        return Err(super::SendError(value));
                    }
                    let mut inner = self.inner.borrow_mut();
                    // reserved was already bumped by ReserveFuture; undo it and push directly.
                    inner.reserved -= 1;
                    inner.buf.push_back(value);
                    if let Some(w) = inner.recv_waker.take() {
                        w.wake();
                    }
                    Ok(())
                }

                pub fn try_send(&self, value: T) -> Result<(), super::super::TrySendError<T>> {
                    let mut inner = self.inner.borrow_mut();
                    if inner.closed {
                        return Err(super::super::TrySendError::Closed(value));
                    }
                    if inner.available() == 0 {
                        return Err(super::super::TrySendError::Full(value));
                    }
                    inner.buf.push_back(value);
                    if let Some(w) = inner.recv_waker.take() {
                        w.wake();
                    }
                    Ok(())
                }

                pub fn is_closed(&self) -> bool {
                    self.inner.borrow().closed
                }

                pub async fn reserve_owned(self) -> Result<OwnedPermit<T>, ChannelClosed> {
                    let reserve = ReserveFuture {
                        inner: Rc::clone(&self.inner),
                    };
                    reserve.await?;
                    // Clone inner for the permit; self drops here (may trigger close check, but
                    // only if this was the last sender, which callers should not do).
                    Ok(OwnedPermit {
                        inner: Rc::clone(&self.inner),
                    })
                }
            }

            /// Future that waits until a slot is available and claims it.
            struct ReserveFuture<T> {
                inner: Rc<RefCell<Inner<T>>>,
            }

            /// Error from [`ReserveFuture`] — channel closed before a slot was available.
            pub struct ChannelClosed;

            impl<T> Future for ReserveFuture<T> {
                type Output = Result<(), ChannelClosed>;

                fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                    let mut inner = self.inner.borrow_mut();
                    if inner.closed {
                        return Poll::Ready(Err(ChannelClosed));
                    }
                    if inner.available() > 0 {
                        inner.reserved += 1;
                        Poll::Ready(Ok(()))
                    } else {
                        inner.send_wakers.push_back(cx.waker().clone());
                        Poll::Pending
                    }
                }
            }

            pub struct OwnedPermit<T> {
                inner: Rc<RefCell<Inner<T>>>,
            }

            impl<T> OwnedPermit<T> {
                pub fn send(self, value: T) -> Result<(), super::SendError<T>> {
                    let mut inner = self.inner.borrow_mut();
                    // We hold a reserved slot — push the value and release the reservation.
                    inner.reserved -= 1;
                    inner.buf.push_back(value);
                    if let Some(w) = inner.recv_waker.take() {
                        w.wake();
                    }
                    Ok(())
                }
            }

            impl<T> Drop for OwnedPermit<T> {
                fn drop(&mut self) {
                    // If dropped without sending, release the reserved slot and wake a sender.
                    let mut inner = self.inner.borrow_mut();
                    if inner.reserved > 0 {
                        inner.reserved -= 1;
                        if let Some(w) = inner.send_wakers.pop_front() {
                            w.wake();
                        }
                    }
                }
            }

            pub struct Receiver<T> {
                inner: Rc<RefCell<Inner<T>>>,
            }

            impl<T> Receiver<T> {
                pub async fn recv(&mut self) -> Option<T> {
                    RecvFuture {
                        inner: Rc::clone(&self.inner),
                    }
                    .await
                }
            }

            struct RecvFuture<T> {
                inner: Rc<RefCell<Inner<T>>>,
            }

            impl<T> Future for RecvFuture<T> {
                type Output = Option<T>;

                fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                    let mut inner = self.inner.borrow_mut();
                    if let Some(value) = inner.buf.pop_front() {
                        // A slot opened up — wake a waiting sender.
                        if let Some(w) = inner.send_wakers.pop_front() {
                            w.wake();
                        }
                        Poll::Ready(Some(value))
                    } else if inner.closed {
                        Poll::Ready(None)
                    } else {
                        inner.recv_waker = Some(cx.waker().clone());
                        Poll::Pending
                    }
                }
            }

            pub fn channel<T>(
                capacity: usize,
            ) -> (super::super::Sender<T>, super::super::Receiver<T>) {
                let inner = Rc::new(RefCell::new(Inner {
                    buf: VecDeque::new(),
                    reserved: 0,
                    capacity,
                    closed: false,
                    recv_waker: None,
                    send_wakers: VecDeque::new(),
                }));
                (
                    super::super::Sender::Bounded(Sender {
                        inner: Rc::clone(&inner),
                    }),
                    super::super::Receiver::Bounded(Receiver { inner }),
                )
            }
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

        impl<T> std::future::Future for Receiver<T> {
            type Output = Result<T, futures_channel::oneshot::Canceled>;

            fn poll(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<Self::Output> {
                let inner =
                    unsafe { std::pin::Pin::new_unchecked(&mut self.get_unchecked_mut().0) };
                inner.poll(cx)
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
