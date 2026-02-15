use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::task::{Context, Poll};
use std::time::Instant;

use peeps_types::{Edge, Node, NodeKind};

// ── WaitEdge ───────────────────────────────────────────
//
// A future wrapper that only emits a registry edge after the inner
// future returns `Poll::Pending` at least once. This ensures edges
// represent actual blockage, not just "about to await".

struct WaitEdge<'a, F> {
    inner: F,
    dst: &'a str,
    edge_src: Option<String>,
    pending: bool,
}

impl<'a, F> WaitEdge<'a, F> {
    fn new(dst: &'a str, inner: F) -> Self {
        Self {
            inner,
            dst,
            edge_src: None,
            pending: false,
        }
    }
}

impl<F: Future> Future for WaitEdge<'_, F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: `inner` is structurally pinned. Other fields (`dst`, `edge_src`,
        // `pending`) are `Unpin` and are not moved through the pin.
        let this = unsafe { self.get_unchecked_mut() };
        let inner = unsafe { Pin::new_unchecked(&mut this.inner) };

        match inner.poll(cx) {
            Poll::Ready(val) => {
                if let Some(src) = this.edge_src.take() {
                    crate::registry::remove_edge(&src, this.dst);
                }
                Poll::Ready(val)
            }
            Poll::Pending => {
                if !this.pending {
                    this.pending = true;
                    crate::stack::with_top(|top| {
                        crate::registry::edge(top, this.dst);
                        this.edge_src = Some(top.to_string());
                    });
                }
                Poll::Pending
            }
        }
    }
}

impl<F> Drop for WaitEdge<'_, F> {
    fn drop(&mut self) {
        if let Some(ref src) = self.edge_src {
            crate::registry::remove_edge(src, self.dst);
        }
    }
}

// ── Info types ──────────────────────────────────────────

pub(super) struct MpscInfo {
    pub(super) name: String,
    pub(super) tx_node_id: String,
    pub(super) rx_node_id: String,
    pub(super) bounded: bool,
    pub(super) capacity: Option<u64>,
    pub(super) sent: AtomicU64,
    pub(super) received: AtomicU64,
    pub(super) send_waiters: AtomicU64,
    pub(super) recv_waiters: AtomicU64,
    pub(super) sender_count: AtomicU64,
    pub(super) sender_closed: AtomicU8,
    pub(super) receiver_closed: AtomicU8,
    pub(super) high_watermark: AtomicU64,
    pub(super) created_at: Instant,
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
    fn track_send_watermark(&self) {
        let sent = self.sent.load(Ordering::Relaxed);
        let received = self.received.load(Ordering::Relaxed);
        let queue_len = sent.saturating_sub(received);
        update_atomic_max(&self.high_watermark, queue_len);
    }
}

pub(super) struct OneshotInfo {
    pub(super) name: String,
    pub(super) tx_node_id: String,
    pub(super) rx_node_id: String,
    pub(super) state: AtomicU8,
    pub(super) created_at: Instant,
}

const ONESHOT_PENDING: u8 = 0;
const ONESHOT_SENT: u8 = 1;
const ONESHOT_RECEIVED: u8 = 2;
const ONESHOT_SENDER_DROPPED: u8 = 3;
const ONESHOT_RECEIVER_DROPPED: u8 = 4;

pub(super) struct WatchInfo {
    pub(super) name: String,
    pub(super) tx_node_id: String,
    pub(super) rx_node_id: String,
    pub(super) changes: AtomicU64,
    pub(super) created_at: Instant,
    pub(super) receiver_count: Box<dyn Fn() -> usize + Send + Sync>,
}

// ── Storage ─────────────────────────────────────────────

static MPSC_REGISTRY: LazyLock<Mutex<Vec<Weak<MpscInfo>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static ONESHOT_REGISTRY: LazyLock<Mutex<Vec<Weak<OneshotInfo>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

static WATCH_REGISTRY: LazyLock<Mutex<Vec<Weak<WatchInfo>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

fn prune_and_register_mpsc(info: &Arc<MpscInfo>) {
    let mut reg = MPSC_REGISTRY.lock().unwrap();
    reg.retain(|w| w.strong_count() > 0);
    reg.push(Arc::downgrade(info));
}

fn prune_and_register_oneshot(info: &Arc<OneshotInfo>) {
    let mut reg = ONESHOT_REGISTRY.lock().unwrap();
    reg.retain(|w| w.strong_count() > 0);
    reg.push(Arc::downgrade(info));
}

fn prune_and_register_watch(info: &Arc<WatchInfo>) {
    let mut reg = WATCH_REGISTRY.lock().unwrap();
    reg.retain(|w| w.strong_count() > 0);
    reg.push(Arc::downgrade(info));
}

// ── mpsc bounded ────────────────────────────────────────

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
        let result = WaitEdge::new(&self.info.tx_node_id, self.inner.send(value)).await;
        self.info.send_waiters.fetch_sub(1, Ordering::Relaxed);
        if result.is_ok() {
            self.info.sent.fetch_add(1, Ordering::Relaxed);
            self.info.track_send_watermark();
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
        let result = WaitEdge::new(&self.info.rx_node_id, self.inner.recv()).await;
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
    let name = name.into();
    let tx_node_id = peeps_types::new_node_id("mpsc_tx");
    let rx_node_id = peeps_types::new_node_id("mpsc_rx");
    let info = Arc::new(MpscInfo {
        name,
        tx_node_id,
        rx_node_id,
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
    });
    prune_and_register_mpsc(&info);
    (
        Sender {
            inner: tx,
            info: Arc::clone(&info),
        },
        Receiver { inner: rx, info },
    )
}

// ── mpsc unbounded ──────────────────────────────────────

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
        let result = WaitEdge::new(&self.info.rx_node_id, self.inner.recv()).await;
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
    let name = name.into();
    let tx_node_id = peeps_types::new_node_id("mpsc_tx");
    let rx_node_id = peeps_types::new_node_id("mpsc_rx");
    let info = Arc::new(MpscInfo {
        name,
        tx_node_id,
        rx_node_id,
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
    });
    prune_and_register_mpsc(&info);
    (
        UnboundedSender {
            inner: tx,
            info: Arc::clone(&info),
        },
        UnboundedReceiver { inner: rx, info },
    )
}

// ── oneshot ─────────────────────────────────────────────

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
    pub async fn recv(
        mut self,
    ) -> Result<T, tokio::sync::oneshot::error::RecvError> {
        let result = WaitEdge::new(&self.info.rx_node_id, &mut self.inner).await;
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
    let name = name.into();
    let tx_node_id = peeps_types::new_node_id("oneshot_tx");
    let rx_node_id = peeps_types::new_node_id("oneshot_rx");
    let info = Arc::new(OneshotInfo {
        name,
        tx_node_id,
        rx_node_id,
        state: AtomicU8::new(ONESHOT_PENDING),
        created_at: Instant::now(),
    });
    prune_and_register_oneshot(&info);
    (
        OneshotSender {
            inner: Some(tx),
            info: Arc::clone(&info),
        },
        OneshotReceiver { inner: rx, info },
    )
}

// ── watch ───────────────────────────────────────────────

pub struct WatchSender<T> {
    inner: tokio::sync::watch::Sender<T>,
    info: Arc<WatchInfo>,
}

impl<T> WatchSender<T> {
    pub fn send(
        &self,
        value: T,
    ) -> Result<(), tokio::sync::watch::error::SendError<T>> {
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
        WaitEdge::new(&self._info.rx_node_id, self.inner.changed()).await
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
    let name = name.into();
    let tx_node_id = peeps_types::new_node_id("watch_tx");
    let rx_node_id = peeps_types::new_node_id("watch_rx");
    let info = Arc::new(WatchInfo {
        name,
        tx_node_id,
        rx_node_id,
        changes: AtomicU64::new(0),
        created_at: Instant::now(),
        receiver_count: Box::new(move || tx_clone.receiver_count()),
    });
    prune_and_register_watch(&info);
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

// ── Graph emission ──────────────────────────────────────

pub(super) fn emit_channel_nodes(graph: &mut peeps_types::GraphSnapshot) {
    let now = Instant::now();

    // ── mpsc channels ────────────────────────────────────
    {
        let reg = MPSC_REGISTRY.lock().unwrap();
        for info in reg.iter().filter_map(|w| w.upgrade()) {
            let name = &info.name;
            let created_at_ns = now.duration_since(info.created_at).as_nanos() as u64;
            let sent = info.sent.load(Ordering::Relaxed);
            let received = info.received.load(Ordering::Relaxed);
            let queue_len = sent.saturating_sub(received);
            let high_watermark = info.high_watermark.load(Ordering::Relaxed);
            let send_waiters = info.send_waiters.load(Ordering::Relaxed);
            let recv_waiters = info.recv_waiters.load(Ordering::Relaxed);
            let sender_count = info.sender_count.load(Ordering::Relaxed);
            let sender_closed = info.sender_closed.load(Ordering::Relaxed) != 0;
            let receiver_closed = info.receiver_closed.load(Ordering::Relaxed) != 0;

            // TX node
            {
                let mut attrs = String::with_capacity(384);
                attrs.push('{');
                json_kv_str(&mut attrs, "name", name, true);
                json_kv_u64(&mut attrs, "created_at_ns", created_at_ns, false);
                json_kv_bool(&mut attrs, "closed", sender_closed, false);
                json_kv_bool(&mut attrs, "bounded", info.bounded, false);
                if let Some(cap) = info.capacity {
                    json_kv_u64(&mut attrs, "capacity", cap, false);
                }
                json_kv_u64(&mut attrs, "sender_count", sender_count, false);
                json_kv_u64(&mut attrs, "send_waiters", send_waiters, false);
                json_kv_u64(&mut attrs, "sent_total", sent, false);
                json_kv_u64(&mut attrs, "queue_len", queue_len, false);
                json_kv_u64(&mut attrs, "high_watermark", high_watermark, false);
                if info.bounded {
                    if let Some(cap) = info.capacity {
                        if cap > 0 {
                            let utilization =
                                (queue_len as f64 / cap as f64 * 1000.0).round() / 1000.0;
                            json_kv_f64(&mut attrs, "utilization", utilization, false);
                        }
                    }
                }
                attrs.push_str(",\"meta\":{}");
                attrs.push('}');

                graph.nodes.push(Node {
                    id: info.tx_node_id.clone(),
                    kind: NodeKind::Tx,
                    label: Some(format!("{name}:tx")),
                    attrs_json: attrs,
                });
            }

            // RX node
            {
                let mut attrs = String::with_capacity(384);
                attrs.push('{');
                json_kv_str(&mut attrs, "name", name, true);
                json_kv_u64(&mut attrs, "created_at_ns", created_at_ns, false);
                json_kv_bool(&mut attrs, "closed", receiver_closed, false);
                json_kv_u64(&mut attrs, "recv_waiters", recv_waiters, false);
                json_kv_u64(&mut attrs, "recv_total", received, false);
                json_kv_u64(&mut attrs, "queue_len", queue_len, false);
                attrs.push_str(",\"meta\":{}");
                attrs.push('}');

                graph.nodes.push(Node {
                    id: info.rx_node_id.clone(),
                    kind: NodeKind::Rx,
                    label: Some(format!("{name}:rx")),
                    attrs_json: attrs,
                });
            }

            // tx → rx gateway edge
            graph.edges.push(Edge {
                src: info.tx_node_id.clone(),
                dst: info.rx_node_id.clone(),
                attrs_json: "{}".to_string(),
            });
        }
    }

    // ── oneshot channels ─────────────────────────────────
    {
        let reg = ONESHOT_REGISTRY.lock().unwrap();
        for info in reg.iter().filter_map(|w| w.upgrade()) {
            let name = &info.name;
            let age_ns = now.duration_since(info.created_at).as_nanos() as u64;
            let state_val = info.state.load(Ordering::Relaxed);
            let state_str = match state_val {
                ONESHOT_SENT => "sent",
                ONESHOT_RECEIVED => "received",
                ONESHOT_SENDER_DROPPED => "sender_dropped",
                ONESHOT_RECEIVER_DROPPED => "receiver_dropped",
                _ => "pending",
            };
            let sender_closed = state_val == ONESHOT_SENDER_DROPPED || state_val == ONESHOT_RECEIVED;
            let receiver_closed =
                state_val == ONESHOT_RECEIVER_DROPPED || state_val == ONESHOT_RECEIVED;

            // TX node
            {
                let mut attrs = String::with_capacity(256);
                attrs.push('{');
                json_kv_str(&mut attrs, "name", name, true);
                json_kv_u64(&mut attrs, "created_at_ns", age_ns, false);
                json_kv_bool(&mut attrs, "closed", sender_closed, false);
                json_kv_str(&mut attrs, "state", state_str, false);
                json_kv_u64(&mut attrs, "age_ns", age_ns, false);
                attrs.push_str(",\"meta\":{}");
                attrs.push('}');

                graph.nodes.push(Node {
                    id: info.tx_node_id.clone(),
                    kind: NodeKind::Tx,
                    label: Some(format!("{name}:tx")),
                    attrs_json: attrs,
                });
            }

            // RX node
            {
                let mut attrs = String::with_capacity(256);
                attrs.push('{');
                json_kv_str(&mut attrs, "name", name, true);
                json_kv_u64(&mut attrs, "created_at_ns", age_ns, false);
                json_kv_bool(&mut attrs, "closed", receiver_closed, false);
                json_kv_str(&mut attrs, "state", state_str, false);
                json_kv_u64(&mut attrs, "age_ns", age_ns, false);
                attrs.push_str(",\"meta\":{}");
                attrs.push('}');

                graph.nodes.push(Node {
                    id: info.rx_node_id.clone(),
                    kind: NodeKind::Rx,
                    label: Some(format!("{name}:rx")),
                    attrs_json: attrs,
                });
            }

            // tx → rx gateway edge
            graph.edges.push(Edge {
                src: info.tx_node_id.clone(),
                dst: info.rx_node_id.clone(),
                attrs_json: "{}".to_string(),
            });
        }
    }

    // ── watch channels ───────────────────────────────────
    {
        let reg = WATCH_REGISTRY.lock().unwrap();
        for info in reg.iter().filter_map(|w| w.upgrade()) {
            let name = &info.name;
            let age_ns = now.duration_since(info.created_at).as_nanos() as u64;
            let changes = info.changes.load(Ordering::Relaxed);
            let receiver_count = (info.receiver_count)() as u64;

            // TX node
            {
                let mut attrs = String::with_capacity(256);
                attrs.push('{');
                json_kv_str(&mut attrs, "name", name, true);
                json_kv_u64(&mut attrs, "created_at_ns", age_ns, false);
                json_kv_u64(&mut attrs, "changes", changes, false);
                json_kv_u64(&mut attrs, "receiver_count", receiver_count, false);
                json_kv_u64(&mut attrs, "age_ns", age_ns, false);
                attrs.push_str(",\"meta\":{}");
                attrs.push('}');

                graph.nodes.push(Node {
                    id: info.tx_node_id.clone(),
                    kind: NodeKind::Tx,
                    label: Some(format!("{name}:tx")),
                    attrs_json: attrs,
                });
            }

            // RX node
            {
                let mut attrs = String::with_capacity(256);
                attrs.push('{');
                json_kv_str(&mut attrs, "name", name, true);
                json_kv_u64(&mut attrs, "created_at_ns", age_ns, false);
                json_kv_u64(&mut attrs, "changes", changes, false);
                json_kv_u64(&mut attrs, "receiver_count", receiver_count, false);
                json_kv_u64(&mut attrs, "age_ns", age_ns, false);
                attrs.push_str(",\"meta\":{}");
                attrs.push('}');

                graph.nodes.push(Node {
                    id: info.rx_node_id.clone(),
                    kind: NodeKind::Rx,
                    label: Some(format!("{name}:rx")),
                    attrs_json: attrs,
                });
            }

            // tx → rx gateway edge
            graph.edges.push(Edge {
                src: info.tx_node_id.clone(),
                dst: info.rx_node_id.clone(),
                attrs_json: "{}".to_string(),
            });
        }
    }
}

// ── JSON helpers ────────────────────────────────────────

pub(super) fn json_kv_str(out: &mut String, key: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":\"");
    peeps_types::json_escape_into(out, value);
    out.push('"');
}

pub(super) fn json_kv_u64(out: &mut String, key: &str, value: u64, first: bool) {
    use std::io::Write;
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    let mut buf = [0u8; 20];
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = write!(cursor, "{value}");
    let len = cursor.position() as usize;
    out.push_str(std::str::from_utf8(&buf[..len]).unwrap_or("0"));
}

pub(super) fn json_kv_bool(out: &mut String, key: &str, value: bool, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(if value { "true" } else { "false" });
}

pub(super) fn json_kv_f64(out: &mut String, key: &str, value: f64, first: bool) {
    use std::io::Write;
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    let mut buf = [0u8; 32];
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = write!(cursor, "{value}");
    let len = cursor.position() as usize;
    out.push_str(std::str::from_utf8(&buf[..len]).unwrap_or("0"));
}
