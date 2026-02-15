//! Diagnostic wrappers for tokio channels, semaphores, and `OnceCell`.
//!
//! When the `diagnostics` feature is enabled, wraps tokio sync primitives
//! to track message counts, channel state, semaphore contention, and OnceCell
//! initialization timing. When disabled, all wrappers are zero-cost.

pub use peeps_types::{
    MpscChannelSnapshot, OnceCellSnapshot, OnceCellState, OneshotChannelSnapshot, OneshotState,
    SemaphoreSnapshot, SyncSnapshot, WatchChannelSnapshot,
};

// ── Diagnostics-enabled implementation ──────────────────────────

#[cfg(feature = "diagnostics")]
mod diag {
    use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
    use std::sync::{Arc, LazyLock, Mutex, Weak};
    use std::time::Instant;

    use super::*;

    // ── Registry ────────────────────────────────────────────────

    static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

    #[derive(Default)]
    struct Registry {
        mpsc: Vec<Weak<MpscInfo>>,
        oneshot: Vec<Weak<OneshotInfo>>,
        watch: Vec<Weak<WatchInfo>>,
        semaphore: Vec<Weak<SemaphoreInfo>>,
        once_cell: Vec<Weak<OnceCellInfo>>,
    }

    pub fn snapshot_all() -> SyncSnapshot {
        let reg = REGISTRY.lock().unwrap();
        let now = Instant::now();

        SyncSnapshot {
            mpsc_channels: reg
                .mpsc
                .iter()
                .filter_map(|w| w.upgrade())
                .map(|info| info.snapshot(now))
                .collect(),
            oneshot_channels: reg
                .oneshot
                .iter()
                .filter_map(|w| w.upgrade())
                .map(|info| info.snapshot(now))
                .collect(),
            watch_channels: reg
                .watch
                .iter()
                .filter_map(|w| w.upgrade())
                .map(|info| info.snapshot(now))
                .collect(),
            semaphores: reg
                .semaphore
                .iter()
                .filter_map(|w| w.upgrade())
                .map(|info| info.snapshot(now))
                .collect(),
            once_cells: reg
                .once_cell
                .iter()
                .filter_map(|w| w.upgrade())
                .map(|info| info.snapshot(now))
                .collect(),
        }
    }

    fn prune_and_register_mpsc(info: &Arc<MpscInfo>) {
        let mut reg = REGISTRY.lock().unwrap();
        reg.mpsc.retain(|w| w.strong_count() > 0);
        reg.mpsc.push(Arc::downgrade(info));
    }

    fn prune_and_register_oneshot(info: &Arc<OneshotInfo>) {
        let mut reg = REGISTRY.lock().unwrap();
        reg.oneshot.retain(|w| w.strong_count() > 0);
        reg.oneshot.push(Arc::downgrade(info));
    }

    fn prune_and_register_watch(info: &Arc<WatchInfo>) {
        let mut reg = REGISTRY.lock().unwrap();
        reg.watch.retain(|w| w.strong_count() > 0);
        reg.watch.push(Arc::downgrade(info));
    }

    fn prune_and_register_semaphore(info: &Arc<SemaphoreInfo>) {
        let mut reg = REGISTRY.lock().unwrap();
        reg.semaphore.retain(|w| w.strong_count() > 0);
        reg.semaphore.push(Arc::downgrade(info));
    }

    fn prune_and_register_once_cell(info: &Arc<OnceCellInfo>) {
        let mut reg = REGISTRY.lock().unwrap();
        reg.once_cell.retain(|w| w.strong_count() > 0);
        reg.once_cell.push(Arc::downgrade(info));
    }

    // ── mpsc ────────────────────────────────────────────────────

    struct MpscInfo {
        name: String,
        bounded: bool,
        capacity: Option<u64>,
        sent: AtomicU64,
        received: AtomicU64,
        send_waiters: AtomicU64,
        sender_count: AtomicU64,
        sender_closed: AtomicU8,
        receiver_closed: AtomicU8,
        created_at: Instant,
        creator_task_id: Option<u64>,
    }

    impl MpscInfo {
        fn snapshot(&self, now: Instant) -> MpscChannelSnapshot {
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
                creator_task_name: self.creator_task_id.and_then(peeps_tasks::task_name),
            }
        }
    }

    // ── mpsc bounded ────────────────────────────────────────────

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
        pub async fn send(
            &self,
            value: T,
        ) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
            self.info.send_waiters.fetch_add(1, Ordering::Relaxed);
            let result = self.inner.send(value).await;
            self.info.send_waiters.fetch_sub(1, Ordering::Relaxed);
            if result.is_ok() {
                self.info.sent.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn try_send(
            &self,
            value: T,
        ) -> Result<(), tokio::sync::mpsc::error::TrySendError<T>> {
            let result = self.inner.try_send(value);
            if result.is_ok() {
                self.info.sent.fetch_add(1, Ordering::Relaxed);
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
            let result = self.inner.recv().await;
            if result.is_some() {
                self.info.received.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn try_recv(
            &mut self,
        ) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
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
            sender_count: AtomicU64::new(1),
            sender_closed: AtomicU8::new(0),
            receiver_closed: AtomicU8::new(0),
            created_at: Instant::now(),
            creator_task_id: peeps_tasks::current_task_id(),
        });
        prune_and_register_mpsc(&info);
        (
            Sender { inner: tx, info: Arc::clone(&info) },
            Receiver { inner: rx, info },
        )
    }

    // ── mpsc unbounded ──────────────────────────────────────────

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
        pub fn send(
            &self,
            value: T,
        ) -> Result<(), tokio::sync::mpsc::error::SendError<T>> {
            let result = self.inner.send(value);
            if result.is_ok() {
                self.info.sent.fetch_add(1, Ordering::Relaxed);
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
            let result = self.inner.recv().await;
            if result.is_some() {
                self.info.received.fetch_add(1, Ordering::Relaxed);
            }
            result
        }

        pub fn try_recv(
            &mut self,
        ) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
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
            sender_count: AtomicU64::new(1),
            sender_closed: AtomicU8::new(0),
            receiver_closed: AtomicU8::new(0),
            created_at: Instant::now(),
            creator_task_id: peeps_tasks::current_task_id(),
        });
        prune_and_register_mpsc(&info);
        (
            UnboundedSender { inner: tx, info: Arc::clone(&info) },
            UnboundedReceiver { inner: rx, info },
        )
    }

    // ── oneshot ─────────────────────────────────────────────────

    const ONESHOT_PENDING: u8 = 0;
    const ONESHOT_SENT: u8 = 1;
    const ONESHOT_RECEIVED: u8 = 2;
    const ONESHOT_SENDER_DROPPED: u8 = 3;
    const ONESHOT_RECEIVER_DROPPED: u8 = 4;

    struct OneshotInfo {
        name: String,
        state: AtomicU8,
        created_at: Instant,
        creator_task_id: Option<u64>,
    }

    impl OneshotInfo {
        fn snapshot(&self, now: Instant) -> OneshotChannelSnapshot {
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
                creator_task_name: self.creator_task_id.and_then(peeps_tasks::task_name),
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
                // Sender dropped without sending
                self.info.state.compare_exchange(
                    ONESHOT_PENDING,
                    ONESHOT_SENDER_DROPPED,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ).ok();
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
            // If still pending, receiver dropped without receiving
            self.info.state.compare_exchange(
                ONESHOT_PENDING,
                ONESHOT_RECEIVER_DROPPED,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).ok();
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

        pub fn try_recv(
            &mut self,
        ) -> Result<T, tokio::sync::oneshot::error::TryRecvError> {
            let result = self.inner.try_recv();
            if result.is_ok() {
                self.info.state.store(ONESHOT_RECEIVED, Ordering::Relaxed);
            }
            result
        }
    }

    pub fn oneshot_channel<T>(
        name: impl Into<String>,
    ) -> (OneshotSender<T>, OneshotReceiver<T>) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let info = Arc::new(OneshotInfo {
            name: name.into(),
            state: AtomicU8::new(ONESHOT_PENDING),
            created_at: Instant::now(),
            creator_task_id: peeps_tasks::current_task_id(),
        });
        prune_and_register_oneshot(&info);
        (
            OneshotSender { inner: Some(tx), info: Arc::clone(&info) },
            OneshotReceiver { inner: rx, info },
        )
    }

    // ── watch ───────────────────────────────────────────────────

    struct WatchInfo {
        name: String,
        changes: AtomicU64,
        created_at: Instant,
        creator_task_id: Option<u64>,
        // We need the sender to query receiver_count, so we store a weak
        // reference to the sender wrapper's inner sender. But the sender
        // is generic, so instead we just snapshot receiver_count at
        // registration time through a callback.
        receiver_count: Box<dyn Fn() -> usize + Send + Sync>,
    }

    impl WatchInfo {
        fn snapshot(&self, now: Instant) -> WatchChannelSnapshot {
            WatchChannelSnapshot {
                name: self.name.clone(),
                changes: self.changes.load(Ordering::Relaxed),
                receiver_count: (self.receiver_count)() as u64,
                age_secs: now.duration_since(self.created_at).as_secs_f64(),
                creator_task_id: self.creator_task_id,
                creator_task_name: self.creator_task_id.and_then(peeps_tasks::task_name),
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
        pub async fn changed(
            &mut self,
        ) -> Result<(), tokio::sync::watch::error::RecvError> {
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
            creator_task_id: peeps_tasks::current_task_id(),
            receiver_count: Box::new(move || tx_clone.receiver_count()),
        });
        prune_and_register_watch(&info);
        (
            WatchSender { inner: tx, info: Arc::clone(&info) },
            WatchReceiver { inner: rx, _info: info },
        )
    }

    // ── Semaphore ───────────────────────────────────────────────

    struct SemaphoreInfo {
        name: String,
        permits_total: u64,
        waiters: AtomicU64,
        acquires: AtomicU64,
        total_wait_nanos: AtomicU64,
        max_wait_nanos: AtomicU64,
        created_at: Instant,
        creator_task_id: Option<u64>,
        available_permits: Box<dyn Fn() -> usize + Send + Sync>,
    }

    impl SemaphoreInfo {
        fn snapshot(&self, now: Instant) -> SemaphoreSnapshot {
            let acquires = self.acquires.load(Ordering::Relaxed);
            let total_wait_nanos = self.total_wait_nanos.load(Ordering::Relaxed);
            let avg_wait_secs = if acquires == 0 {
                0.0
            } else {
                (total_wait_nanos as f64 / acquires as f64) / 1_000_000_000.0
            };
            SemaphoreSnapshot {
                name: self.name.clone(),
                permits_total: self.permits_total,
                permits_available: (self.available_permits)() as u64,
                waiters: self.waiters.load(Ordering::Relaxed),
                acquires,
                avg_wait_secs,
                max_wait_secs: self.max_wait_nanos.load(Ordering::Relaxed) as f64 / 1_000_000_000.0,
                age_secs: now.duration_since(self.created_at).as_secs_f64(),
                creator_task_id: self.creator_task_id,
                creator_task_name: self.creator_task_id.and_then(peeps_tasks::task_name),
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
                created_at: Instant::now(),
                creator_task_id: peeps_tasks::current_task_id(),
                available_permits: Box::new(move || inner_for_snapshot.available_permits()),
            });
            prune_and_register_semaphore(&info);
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
            self.info.waiters.fetch_add(1, Ordering::Relaxed);
            let start = Instant::now();
            let result = self.inner.acquire().await;
            self.info.waiters.fetch_sub(1, Ordering::Relaxed);
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
            self.info.waiters.fetch_add(1, Ordering::Relaxed);
            let start = Instant::now();
            let result = self.inner.acquire_many(n).await;
            self.info.waiters.fetch_sub(1, Ordering::Relaxed);
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
            self.info.waiters.fetch_add(1, Ordering::Relaxed);
            let start = Instant::now();
            let result = Arc::clone(&self.inner).acquire_owned().await;
            self.info.waiters.fetch_sub(1, Ordering::Relaxed);
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
            self.info.waiters.fetch_add(1, Ordering::Relaxed);
            let start = Instant::now();
            let result = Arc::clone(&self.inner).acquire_many_owned(n).await;
            self.info.waiters.fetch_sub(1, Ordering::Relaxed);
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

    // ── OnceCell ────────────────────────────────────────────────

    const ONCE_EMPTY: u8 = 0;
    const ONCE_INITIALIZING: u8 = 1;
    const ONCE_INITIALIZED: u8 = 2;

    struct OnceCellInfo {
        name: String,
        state: AtomicU8,
        created_at: Instant,
        init_duration: Mutex<Option<std::time::Duration>>,
    }

    impl OnceCellInfo {
        fn snapshot(&self, now: Instant) -> OnceCellSnapshot {
            let state = match self.state.load(Ordering::Relaxed) {
                ONCE_INITIALIZING => OnceCellState::Initializing,
                ONCE_INITIALIZED => OnceCellState::Initialized,
                _ => OnceCellState::Empty,
            };
            OnceCellSnapshot {
                name: self.name.clone(),
                state,
                age_secs: now.duration_since(self.created_at).as_secs_f64(),
                init_duration_secs: self
                    .init_duration
                    .lock()
                    .unwrap()
                    .map(|d| d.as_secs_f64()),
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
            prune_and_register_once_cell(&info);
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
                .compare_exchange(ONCE_EMPTY, ONCE_INITIALIZING, Ordering::Relaxed, Ordering::Relaxed)
                .ok();
            let start = Instant::now();

            let result = self.inner.get_or_init(f).await;

            if self
                .info
                .state
                .compare_exchange(ONCE_INITIALIZING, ONCE_INITIALIZED, Ordering::Relaxed, Ordering::Relaxed)
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
                .compare_exchange(ONCE_EMPTY, ONCE_INITIALIZING, Ordering::Relaxed, Ordering::Relaxed)
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
                .compare_exchange(ONCE_EMPTY, ONCE_INITIALIZING, Ordering::Relaxed, Ordering::Relaxed)
                .ok();
            match self.inner.set(value) {
                Ok(()) => {
                    self.info.state.store(ONCE_INITIALIZED, Ordering::Relaxed);
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

// ── Zero-cost stubs (no diagnostics) ────────────────────────────

#[cfg(not(feature = "diagnostics"))]
mod diag {
    use super::*;

    pub fn snapshot_all() -> SyncSnapshot {
        SyncSnapshot {
            mpsc_channels: Vec::new(),
            oneshot_channels: Vec::new(),
            watch_channels: Vec::new(),
            semaphores: Vec::new(),
            once_cells: Vec::new(),
        }
    }

    // ── mpsc bounded ────────────────────────────────────────────

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
        pub fn try_recv(
            &mut self,
        ) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
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

    // ── mpsc unbounded ──────────────────────────────────────────

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
        pub fn try_recv(
            &mut self,
        ) -> Result<T, tokio::sync::mpsc::error::TryRecvError> {
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

    // ── oneshot ─────────────────────────────────────────────────

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
        pub fn try_recv(
            &mut self,
        ) -> Result<T, tokio::sync::oneshot::error::TryRecvError> {
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

    // ── watch ───────────────────────────────────────────────────

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
        pub fn has_changed(
            &self,
        ) -> Result<bool, tokio::sync::watch::error::RecvError> {
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

    // ── Semaphore ───────────────────────────────────────────────

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

    // ── OnceCell ────────────────────────────────────────────────

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

// ── Public API ──────────────────────────────────────────────────

pub use diag::{
    // mpsc bounded
    channel, Receiver, Sender,
    // mpsc unbounded
    unbounded_channel, UnboundedReceiver, UnboundedSender,
    // oneshot
    oneshot_channel, OneshotReceiver, OneshotSender,
    // watch
    watch_channel, WatchReceiver, WatchSender,
    // semaphore
    DiagnosticSemaphore,
    // OnceCell
    OnceCell,
    // snapshot
    snapshot_all,
};
