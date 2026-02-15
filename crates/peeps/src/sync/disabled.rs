// ── Zero-cost stubs (no diagnostics) ────────────────────

// ── mpsc bounded ────────────────────────────────────────

pub struct Sender<T>(tokio::sync::mpsc::Sender<T>);

impl<T> Clone for Sender<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Sender<T> {
    #[inline]
    pub async fn send(
        &self,
        value: T,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
        self.0.send(value).await
    }

    #[inline]
    pub fn try_send(
        &self,
        value: T,
    ) -> Result<(), tokio::sync::mpsc::error::TrySendError<T>> {
        self.0.try_send(value)
    }

    #[inline]
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    #[inline]
    pub fn max_capacity(&self) -> usize {
        self.0.max_capacity()
    }
}

pub struct Receiver<T>(tokio::sync::mpsc::Receiver<T>);

impl<T> Receiver<T> {
    #[inline]
    pub async fn recv(&mut self) -> Option<T> {
        self.0.recv().await
    }

    #[inline]
    pub fn try_recv(&mut self) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
        self.0.try_recv()
    }

    #[inline]
    pub fn close(&mut self) {
        self.0.close()
    }
}

#[inline]
pub fn channel<T>(_name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = tokio::sync::mpsc::channel(buffer);
    (Sender(tx), Receiver(rx))
}

// ── mpsc unbounded ──────────────────────────────────────

pub struct UnboundedSender<T>(tokio::sync::mpsc::UnboundedSender<T>);

impl<T> Clone for UnboundedSender<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> UnboundedSender<T> {
    #[inline]
    pub fn send(
        &self,
        value: T,
    ) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
        self.0.send(value)
    }

    #[inline]
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

pub struct UnboundedReceiver<T>(tokio::sync::mpsc::UnboundedReceiver<T>);

impl<T> UnboundedReceiver<T> {
    #[inline]
    pub async fn recv(&mut self) -> Option<T> {
        self.0.recv().await
    }

    #[inline]
    pub fn try_recv(&mut self) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
        self.0.try_recv()
    }

    #[inline]
    pub fn close(&mut self) {
        self.0.close()
    }
}

#[inline]
pub fn unbounded_channel<T>(
    _name: impl Into<String>,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (UnboundedSender(tx), UnboundedReceiver(rx))
}

// ── oneshot ─────────────────────────────────────────────

pub struct OneshotSender<T>(tokio::sync::oneshot::Sender<T>);

impl<T> OneshotSender<T> {
    #[inline]
    pub fn send(self, value: T) -> Result<(), T> {
        self.0.send(value)
    }

    #[inline]
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

pub struct OneshotReceiver<T>(tokio::sync::oneshot::Receiver<T>);

impl<T> OneshotReceiver<T> {
    #[inline]
    pub async fn recv(self) -> Result<T, tokio::sync::oneshot::error::RecvError> {
        self.0.await
    }

    #[inline]
    pub fn try_recv(&mut self) -> Result<T, tokio::sync::oneshot::error::TryRecvError> {
        self.0.try_recv()
    }
}

#[inline]
pub fn oneshot_channel<T>(
    _name: impl Into<String>,
) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    (OneshotSender(tx), OneshotReceiver(rx))
}

// ── watch ───────────────────────────────────────────────

pub struct WatchSender<T>(tokio::sync::watch::Sender<T>);

impl<T> WatchSender<T> {
    #[inline]
    pub fn send(
        &self,
        value: T,
    ) -> Result<(), tokio::sync::watch::error::SendError<T>> {
        self.0.send(value)
    }

    #[inline]
    pub fn send_modify<F: FnOnce(&mut T)>(&self, modify: F) {
        self.0.send_modify(modify)
    }

    #[inline]
    pub fn send_if_modified<F: FnOnce(&mut T) -> bool>(&self, modify: F) -> bool {
        self.0.send_if_modified(modify)
    }

    #[inline]
    pub fn borrow(&self) -> tokio::sync::watch::Ref<'_, T> {
        self.0.borrow()
    }

    #[inline]
    pub fn receiver_count(&self) -> usize {
        self.0.receiver_count()
    }

    #[inline]
    pub fn subscribe(&self) -> WatchReceiver<T> {
        WatchReceiver(self.0.subscribe())
    }

    #[inline]
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
}

pub struct WatchReceiver<T>(tokio::sync::watch::Receiver<T>);

impl<T> Clone for WatchReceiver<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> WatchReceiver<T> {
    #[inline]
    pub async fn changed(
        &mut self,
    ) -> Result<(), tokio::sync::watch::error::RecvError> {
        self.0.changed().await
    }

    #[inline]
    pub fn borrow(&self) -> tokio::sync::watch::Ref<'_, T> {
        self.0.borrow()
    }

    #[inline]
    pub fn borrow_and_update(&mut self) -> tokio::sync::watch::Ref<'_, T> {
        self.0.borrow_and_update()
    }

    #[inline]
    pub fn has_changed(&self) -> Result<bool, tokio::sync::watch::error::RecvError> {
        self.0.has_changed()
    }
}

#[inline]
pub fn watch_channel<T>(
    _name: impl Into<String>,
    init: T,
) -> (WatchSender<T>, WatchReceiver<T>) {
    let (tx, rx) = tokio::sync::watch::channel(init);
    (WatchSender(tx), WatchReceiver(rx))
}

// ── Semaphore ───────────────────────────────────────────

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

// ── OnceCell ────────────────────────────────────────────

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

// ── Graph emission (no-op) ──────────────────────────────

#[inline(always)]
pub fn emit_into_graph(_graph: &mut peeps_types::GraphSnapshot) {}
