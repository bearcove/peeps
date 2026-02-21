//! WASM runtime surface for moire.
//!
//! Instrumentation is intentionally no-op on wasm.

use std::future::Future;

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
    use super::{Receiver, Sender};

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

/// Instrumented oneshot channel primitives (wasm no-op backend).
pub mod oneshot {
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
