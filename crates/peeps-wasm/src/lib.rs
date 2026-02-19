//! WASM runtime surface for peeps.
//!
//! Instrumentation is intentionally no-op on wasm.

use std::future::Future;
use std::time::Duration;

#[doc(hidden)]
pub use tokio;

#[doc(hidden)]
pub fn __init_from_macro() {}

#[macro_export]
macro_rules! facade {
    () => {
        pub mod peeps {
            pub mod prelude {}
        }
    };
}

/// Wrapper around `std::sync::Mutex` with the same constructor shape as native peeps.
pub struct Mutex<T>(std::sync::Mutex<T>);

impl<T> Mutex<T> {
    #[inline]
    pub fn new(_name: &'static str, value: T) -> Self {
        Self(std::sync::Mutex::new(value))
    }

    #[inline]
    pub fn lock(&self) -> std::sync::MutexGuard<'_, T> {
        self.0
            .lock()
            .expect("wasm mutex poisoned; cannot continue")
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
            Self::Closed(_) => f.debug_struct("TrySendError::Closed").finish_non_exhaustive(),
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

/// Unbounded sender.
pub type UnboundedSender<T> = Sender<T>;
/// Unbounded receiver.
pub type UnboundedReceiver<T> = Receiver<T>;

/// Oneshot sender.
pub use futures_channel::oneshot::Sender as OneshotSender;

/// Wrapper around `futures-channel` oneshot receiver for API parity.
pub struct OneshotReceiver<T>(futures_channel::oneshot::Receiver<T>);

impl<T> OneshotReceiver<T> {
    pub async fn recv(self) -> Result<T, futures_channel::oneshot::Canceled> {
        self.0.await
    }

    pub fn try_recv(&mut self) -> Result<Option<T>, futures_channel::oneshot::Canceled> {
        self.0.try_recv()
    }
}

/// Create a bounded mpsc channel.
pub fn channel<T>(_name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = async_channel::bounded(buffer);
    (Sender(tx), Receiver(rx))
}

/// Alias for `channel`.
pub fn bounded<T>(name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
    channel(name, buffer)
}

/// Create an unbounded mpsc channel.
pub fn unbounded_channel<T>(_name: impl Into<String>) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (tx, rx) = async_channel::unbounded();
    (Sender(tx), Receiver(rx))
}

/// Alias for `unbounded_channel`.
pub fn unbounded<T>(name: impl Into<String>) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    unbounded_channel(name)
}

/// Create a oneshot channel.
pub fn oneshot<T>(_name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let (tx, rx) = futures_channel::oneshot::channel();
    (tx, OneshotReceiver(rx))
}

/// Alias for `oneshot`.
pub fn oneshot_channel<T>(name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    oneshot(name)
}

/// Handle that can be used to abort a spawned task.
///
/// On wasm this is a no-op because fire-and-forget tasks cannot be cancelled.
#[derive(Debug)]
pub struct AbortHandle;

impl AbortHandle {
    pub fn abort(&self) -> bool {
        false
    }
}

/// Spawn a task concurrently on the browser executor.
pub fn spawn<F>(_name: &'static str, future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

/// Spawn a task and return an abort handle.
pub fn spawn_with_abort<F>(_name: &'static str, future: F) -> AbortHandle
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
    AbortHandle
}

/// Sleep for a duration.
pub async fn sleep(duration: Duration, _label: impl Into<String>) {
    gloo_timers::future::sleep(duration).await;
}

/// Run a future with a timeout.
pub async fn timeout<F, T>(duration: Duration, future: F, _label: impl Into<String>) -> Option<T>
where
    F: Future<Output = T>,
{
    use futures_util::future::{Either, select};
    use std::pin::pin;

    let sleep_fut = pin!(gloo_timers::future::sleep(duration));
    let work_fut = pin!(future);

    match select(work_fut, sleep_fut).await {
        Either::Left((result, _)) => Some(result),
        Either::Right((_, _)) => None,
    }
}
