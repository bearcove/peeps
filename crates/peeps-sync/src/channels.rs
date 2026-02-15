// ══════════════════════════════════════════════════════════
// Diagnostics-enabled implementation
// ══════════════════════════════════════════════════════════

#[cfg(feature = "diagnostics")]
mod diag {
    use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    use peeps_types::{
        MpscChannelSnapshot, OneshotChannelSnapshot, OneshotState, WatchChannelSnapshot,
    };

    // ── mpsc info ───────────────────────────────────────

    pub(crate) struct MpscInfo {
        pub(crate) name: String,
        pub(crate) bounded: bool,
        pub(crate) capacity: Option<u64>,
        pub(crate) sent: AtomicU64,
        pub(crate) received: AtomicU64,
        pub(crate) send_waiters: AtomicU64,
        pub(crate) recv_waiters: AtomicU64,
        pub(crate) sender_count: AtomicU64,
        pub(crate) sender_closed: AtomicU8,
        pub(crate) receiver_closed: AtomicU8,
        pub(crate) high_watermark: AtomicU64,
        pub(crate) created_at: Instant,
        pub(crate) creator_task_id: Option<u64>,
    }

    fn update_atomic_max(target: &AtomicU64, observed: u64) {
        let mut current = target.load(Ordering::Relaxed);
        while observed > current {
            match target.compare_exchange_weak(
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

    impl MpscInfo {
        pub(crate) fn snapshot(&self, now: Instant) -> MpscChannelSnapshot {
            MpscChannelSnapshot {
                name: self.name.clone(),
                bounded: self.bounded,
                capacity: self.capacity,
                sent: self.sent.load(Ordering::Relaxed),
                received: self.received.load(Ordering::Relaxed),
                send_waiters: self.send_waiters.load(Ordering::Relaxed),
                sender_count: self.sender_count.load(Ordering::Relaxed),
                sender_closed: self.sender_closed.load(Ordering::Relaxed) != 0,
                receiver_closed: self.receiver_closed.load(Ordering::Relaxed) != 0,
                age_secs: now.duration_since(self.created_at).as_secs_f64(),
                creator_task_id: self.creator_task_id,
                creator_task_name: self.creator_task_id.and_then(peeps_futures::task_name),
            }
        }

        fn track_send_watermark(&self) {
            let sent = self.sent.load(Ordering::Relaxed);
            let received = self.received.load(Ordering::Relaxed);
            let queue_len = sent.saturating_sub(received);
            update_atomic_max(&self.high_watermark, queue_len);
        }
    }

    // ── mpsc bounded ────────────────────────────────────

    pub struct Sender<T> {
        inner: tokio::sync::mpsc::Sender<T>,
        info: Arc<MpscInfo>,
    }

    impl<T> Clone for Sender<T> {
        fn clone(&self) -> Self {
            self.info.sender_count.fetch_add(1, Ordering::Relaxed);
            Self {
                inner: self.inner.clone(),
                info: Arc::clone(&self.info),
            }
        }
    }

    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            let prev = self.info.sender_count.fetch_sub(1, Ordering::Relaxed);
            if prev == 1 {
                self.info.sender_closed.store(1, Ordering::Relaxed);
            }
        }
    }

    impl<T> Sender<T> {
        pub async fn send(&self, value: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
            self.info.send_waiters.fetch_add(1, Ordering::Relaxed);
            let result = self.inner.send(value).await;
            self.info.send_waiters.fetch_sub(1, Ordering::Relaxed);
            if result.is_ok() {
                self.info.sent.fetch_add(1, Ordering::Relaxed);
                self.info.track_send_watermark();
            }
            result
        }

        pub fn try_send(&self, value: T) -> Result<(), tokio::sync::mpsc::error::TrySendError<T>> {
            let result = self.inner.try_send(value);
            if result.is_ok() {
                self.info.sent.fetch_add(1, Ordering::Relaxed);
                self.info.track_send_watermark();
            }
            result
        }

        pub fn is_closed(&self) -> bool {
            self.inner.is_closed()
        }

        pub fn capacity(&self) -> usize {
            self.inner.capacity()
        }

        pub fn max_capacity(&self) -> usize {
            self.inner.max_capacity()
        }
    }

    pub struct Receiver<T> {
        inner: tokio::sync::mpsc::Receiver<T>,
        info: Arc<MpscInfo>,
    }

    impl<T> Drop for Receiver<T> {
        fn drop(&mut self) {
            self.info.receiver_closed.store(1, Ordering::Relaxed);
        }
    }

    impl<T> Receiver<T> {
        pub async fn recv(&mut self) -> Option<T> {
            self.info.recv_waiters.fetch_add(1, Ordering::Relaxed);
            let result = self.inner.recv().await;
            self.info.recv_waiters.fetch_sub(1, Ordering::Relaxed);
            if result.is_some() {
                self.info.received.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn try_recv(&mut self) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
            let result = self.inner.try_recv();
            if result.is_ok() {
                self.info.received.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn close(&mut self) {
            self.inner.close();
            self.info.receiver_closed.store(1, Ordering::Relaxed);
        }
    }

    pub fn channel<T>(name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = tokio::sync::mpsc::channel(buffer);
        let info = Arc::new(MpscInfo {
            name: name.into(),
            bounded: true,
            capacity: Some(buffer as u64),
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
            send_waiters: AtomicU64::new(0),
            recv_waiters: AtomicU64::new(0),
            sender_count: AtomicU64::new(1),
            sender_closed: AtomicU8::new(0),
            receiver_closed: AtomicU8::new(0),
            high_watermark: AtomicU64::new(0),
            created_at: Instant::now(),
            creator_task_id: peeps_futures::current_task_id(),
        });
        crate::registry::prune_and_register_mpsc(&info);
        (
            Sender {
                inner: tx,
                info: Arc::clone(&info),
            },
            Receiver { inner: rx, info },
        )
    }

    // ── mpsc unbounded ──────────────────────────────────

    pub struct UnboundedSender<T> {
        inner: tokio::sync::mpsc::UnboundedSender<T>,
        info: Arc<MpscInfo>,
    }

    impl<T> Clone for UnboundedSender<T> {
        fn clone(&self) -> Self {
            self.info.sender_count.fetch_add(1, Ordering::Relaxed);
            Self {
                inner: self.inner.clone(),
                info: Arc::clone(&self.info),
            }
        }
    }

    impl<T> Drop for UnboundedSender<T> {
        fn drop(&mut self) {
            let prev = self.info.sender_count.fetch_sub(1, Ordering::Relaxed);
            if prev == 1 {
                self.info.sender_closed.store(1, Ordering::Relaxed);
            }
        }
    }

    impl<T> UnboundedSender<T> {
        pub fn send(&self, value: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
            let result = self.inner.send(value);
            if result.is_ok() {
                self.info.sent.fetch_add(1, Ordering::Relaxed);
                self.info.track_send_watermark();
            }
            result
        }

        pub fn is_closed(&self) -> bool {
            self.inner.is_closed()
        }
    }

    pub struct UnboundedReceiver<T> {
        inner: tokio::sync::mpsc::UnboundedReceiver<T>,
        info: Arc<MpscInfo>,
    }

    impl<T> Drop for UnboundedReceiver<T> {
        fn drop(&mut self) {
            self.info.receiver_closed.store(1, Ordering::Relaxed);
        }
    }

    impl<T> UnboundedReceiver<T> {
        pub async fn recv(&mut self) -> Option<T> {
            self.info.recv_waiters.fetch_add(1, Ordering::Relaxed);
            let result = self.inner.recv().await;
            self.info.recv_waiters.fetch_sub(1, Ordering::Relaxed);
            if result.is_some() {
                self.info.received.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn try_recv(&mut self) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
            let result = self.inner.try_recv();
            if result.is_ok() {
                self.info.received.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn close(&mut self) {
            self.inner.close();
            self.info.receiver_closed.store(1, Ordering::Relaxed);
        }
    }

    pub fn unbounded_channel<T>(
        name: impl Into<String>,
    ) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let info = Arc::new(MpscInfo {
            name: name.into(),
            bounded: false,
            capacity: None,
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
            send_waiters: AtomicU64::new(0),
            recv_waiters: AtomicU64::new(0),
            sender_count: AtomicU64::new(1),
            sender_closed: AtomicU8::new(0),
            receiver_closed: AtomicU8::new(0),
            high_watermark: AtomicU64::new(0),
            created_at: Instant::now(),
            creator_task_id: peeps_futures::current_task_id(),
        });
        crate::registry::prune_and_register_mpsc(&info);
        (
            UnboundedSender {
                inner: tx,
                info: Arc::clone(&info),
            },
            UnboundedReceiver { inner: rx, info },
        )
    }

    // ── oneshot ─────────────────────────────────────────

    const ONESHOT_PENDING: u8 = 0;
    const ONESHOT_SENT: u8 = 1;
    const ONESHOT_RECEIVED: u8 = 2;
    const ONESHOT_SENDER_DROPPED: u8 = 3;
    const ONESHOT_RECEIVER_DROPPED: u8 = 4;

    pub(crate) struct OneshotInfo {
        pub(crate) name: String,
        pub(crate) state: AtomicU8,
        pub(crate) created_at: Instant,
        pub(crate) creator_task_id: Option<u64>,
    }

    impl OneshotInfo {
        pub(crate) fn snapshot(&self, now: Instant) -> OneshotChannelSnapshot {
            let state = match self.state.load(Ordering::Relaxed) {
                ONESHOT_SENT => OneshotState::Sent,
                ONESHOT_RECEIVED => OneshotState::Received,
                ONESHOT_SENDER_DROPPED => OneshotState::SenderDropped,
                ONESHOT_RECEIVER_DROPPED => OneshotState::ReceiverDropped,
                _ => OneshotState::Pending,
            };
            OneshotChannelSnapshot {
                name: self.name.clone(),
                state,
                age_secs: now.duration_since(self.created_at).as_secs_f64(),
                creator_task_id: self.creator_task_id,
                creator_task_name: self.creator_task_id.and_then(peeps_futures::task_name),
            }
        }
    }

    pub struct OneshotSender<T> {
        inner: Option<tokio::sync::oneshot::Sender<T>>,
        info: Arc<OneshotInfo>,
    }

    impl<T> Drop for OneshotSender<T> {
        fn drop(&mut self) {
            if self.inner.is_some() {
                self.info
                    .state
                    .compare_exchange(
                        ONESHOT_PENDING,
                        ONESHOT_SENDER_DROPPED,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .ok();
            }
        }
    }

    impl<T> OneshotSender<T> {
        pub fn send(mut self, value: T) -> Result<(), T> {
            let inner = self.inner.take().unwrap();
            let result = inner.send(value);
            if result.is_ok() {
                self.info.state.store(ONESHOT_SENT, Ordering::Relaxed);
            }
            result
        }

        pub fn is_closed(&self) -> bool {
            self.inner.as_ref().unwrap().is_closed()
        }
    }

    pub struct OneshotReceiver<T> {
        inner: tokio::sync::oneshot::Receiver<T>,
        info: Arc<OneshotInfo>,
    }

    impl<T> Drop for OneshotReceiver<T> {
        fn drop(&mut self) {
            self.info
                .state
                .compare_exchange(
                    ONESHOT_PENDING,
                    ONESHOT_RECEIVER_DROPPED,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .ok();
        }
    }

    impl<T> OneshotReceiver<T> {
        pub async fn recv(mut self) -> Result<T, tokio::sync::oneshot::error::RecvError> {
            let result = (&mut self.inner).await;
            if result.is_ok() {
                self.info.state.store(ONESHOT_RECEIVED, Ordering::Relaxed);
            }
            result
        }

        pub fn try_recv(&mut self) -> Result<T, tokio::sync::oneshot::error::TryRecvError> {
            let result = self.inner.try_recv();
            if result.is_ok() {
                self.info.state.store(ONESHOT_RECEIVED, Ordering::Relaxed);
            }
            result
        }
    }

    pub fn oneshot_channel<T>(name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let info = Arc::new(OneshotInfo {
            name: name.into(),
            state: AtomicU8::new(ONESHOT_PENDING),
            created_at: Instant::now(),
            creator_task_id: peeps_futures::current_task_id(),
        });
        crate::registry::prune_and_register_oneshot(&info);
        (
            OneshotSender {
                inner: Some(tx),
                info: Arc::clone(&info),
            },
            OneshotReceiver { inner: rx, info },
        )
    }

    // ── watch ───────────────────────────────────────────

    pub(crate) struct WatchInfo {
        pub(crate) name: String,
        pub(crate) changes: AtomicU64,
        pub(crate) created_at: Instant,
        pub(crate) creator_task_id: Option<u64>,
        pub(crate) receiver_count: Box<dyn Fn() -> usize + Send + Sync>,
    }

    impl WatchInfo {
        pub(crate) fn snapshot(&self, now: Instant) -> WatchChannelSnapshot {
            WatchChannelSnapshot {
                name: self.name.clone(),
                changes: self.changes.load(Ordering::Relaxed),
                receiver_count: (self.receiver_count)() as u64,
                age_secs: now.duration_since(self.created_at).as_secs_f64(),
                creator_task_id: self.creator_task_id,
                creator_task_name: self.creator_task_id.and_then(peeps_futures::task_name),
            }
        }
    }

    pub struct WatchSender<T> {
        inner: tokio::sync::watch::Sender<T>,
        info: Arc<WatchInfo>,
    }

    impl<T> WatchSender<T> {
        pub fn send(&self, value: T) -> Result<(), tokio::sync::watch::error::SendError<T>> {
            let result = self.inner.send(value);
            if result.is_ok() {
                self.info.changes.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn send_modify<F: FnOnce(&mut T)>(&self, modify: F) {
            self.inner.send_modify(modify);
            self.info.changes.fetch_add(1, Ordering::Relaxed);
        }

        pub fn send_if_modified<F: FnOnce(&mut T) -> bool>(&self, modify: F) -> bool {
            let modified = self.inner.send_if_modified(modify);
            if modified {
                self.info.changes.fetch_add(1, Ordering::Relaxed);
            }
            modified
        }

        pub fn borrow(&self) -> tokio::sync::watch::Ref<'_, T> {
            self.inner.borrow()
        }

        pub fn receiver_count(&self) -> usize {
            self.inner.receiver_count()
        }

        pub fn subscribe(&self) -> WatchReceiver<T> {
            WatchReceiver {
                inner: self.inner.subscribe(),
                _info: Arc::clone(&self.info),
            }
        }

        pub fn is_closed(&self) -> bool {
            self.inner.is_closed()
        }
    }

    pub struct WatchReceiver<T> {
        inner: tokio::sync::watch::Receiver<T>,
        _info: Arc<WatchInfo>,
    }

    impl<T> Clone for WatchReceiver<T> {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
                _info: Arc::clone(&self._info),
            }
        }
    }

    impl<T> WatchReceiver<T> {
        pub async fn changed(&mut self) -> Result<(), tokio::sync::watch::error::RecvError> {
            self.inner.changed().await
        }

        pub fn borrow(&self) -> tokio::sync::watch::Ref<'_, T> {
            self.inner.borrow()
        }

        pub fn borrow_and_update(&mut self) -> tokio::sync::watch::Ref<'_, T> {
            self.inner.borrow_and_update()
        }

        pub fn has_changed(&self) -> Result<bool, tokio::sync::watch::error::RecvError> {
            self.inner.has_changed()
        }
    }

    pub fn watch_channel<T: Send + Sync + 'static>(
        name: impl Into<String>,
        init: T,
    ) -> (WatchSender<T>, WatchReceiver<T>) {
        let (tx, rx) = tokio::sync::watch::channel(init);
        let tx_clone = tx.clone();
        let info = Arc::new(WatchInfo {
            name: name.into(),
            changes: AtomicU64::new(0),
            created_at: Instant::now(),
            creator_task_id: peeps_futures::current_task_id(),
            receiver_count: Box::new(move || tx_clone.receiver_count()),
        });
        crate::registry::prune_and_register_watch(&info);
        (
            WatchSender {
                inner: tx,
                info: Arc::clone(&info),
            },
            WatchReceiver {
                inner: rx,
                _info: info,
            },
        )
    }
}

// ══════════════════════════════════════════════════════════
// Zero-cost stubs (no diagnostics)
// ══════════════════════════════════════════════════════════

#[cfg(not(feature = "diagnostics"))]
mod stub {
    pub struct Sender<T>(tokio::sync::mpsc::Sender<T>);

    impl<T> Clone for Sender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> Sender<T> {
        #[inline]
        pub async fn send(&self, value: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
            self.0.send(value).await
        }

        #[inline]
        pub fn try_send(&self, value: T) -> Result<(), tokio::sync::mpsc::error::TrySendError<T>> {
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

    // ── mpsc unbounded ──────────────────────────────────

    pub struct UnboundedSender<T>(tokio::sync::mpsc::UnboundedSender<T>);

    impl<T> Clone for UnboundedSender<T> {
        #[inline]
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }

    impl<T> UnboundedSender<T> {
        #[inline]
        pub fn send(&self, value: T) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
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

    // ── oneshot ─────────────────────────────────────────

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
    pub fn oneshot_channel<T>(_name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (OneshotSender(tx), OneshotReceiver(rx))
    }

    // ── watch ───────────────────────────────────────────

    pub struct WatchSender<T>(tokio::sync::watch::Sender<T>);

    impl<T> WatchSender<T> {
        #[inline]
        pub fn send(&self, value: T) -> Result<(), tokio::sync::watch::error::SendError<T>> {
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
        pub async fn changed(&mut self) -> Result<(), tokio::sync::watch::error::RecvError> {
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
}

// ── Re-exports ──────────────────────────────────────────

// Re-export the info types for registry access
#[cfg(feature = "diagnostics")]
pub(crate) use diag::{MpscInfo, OneshotInfo, WatchInfo};

#[cfg(feature = "diagnostics")]
pub use diag::{
    channel, oneshot_channel, unbounded_channel, watch_channel, OneshotReceiver, OneshotSender,
    Receiver, Sender, UnboundedReceiver, UnboundedSender, WatchReceiver, WatchSender,
};

#[cfg(not(feature = "diagnostics"))]
pub use stub::{
    channel, oneshot_channel, unbounded_channel, watch_channel, OneshotReceiver, OneshotSender,
    Receiver, Sender, UnboundedReceiver, UnboundedSender, WatchReceiver, WatchSender,
};
