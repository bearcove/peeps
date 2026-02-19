use compact_str::CompactString;
use peeps_types::{
    BufferState, ChannelCloseCause, ChannelClosedEvent, ChannelEndpointLifecycle,
    ChannelWaitEndedEvent, ChannelWaitKind, ChannelWaitStartedEvent, EntityId, Event, EventTarget,
    OneshotState,
};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;

pub(super) use super::db::runtime_db;
pub(super) use super::futures::instrument_operation_on_with_source;
pub(super) use super::handles::{AsEntityRef, EntityHandle, EntityRef};
pub(super) use super::{
    record_event_with_entity_source, record_event_with_source, CrateContext, UnqualSource,
};

pub mod broadcast;
pub mod mpsc;
pub mod oneshot;
pub mod watch;

pub struct Sender<T> {
    inner: tokio::sync::mpsc::Sender<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct Receiver<T> {
    inner: tokio::sync::mpsc::Receiver<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct UnboundedSender<T> {
    inner: tokio::sync::mpsc::UnboundedSender<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct UnboundedReceiver<T> {
    inner: tokio::sync::mpsc::UnboundedReceiver<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct OneshotSender<T> {
    inner: Option<tokio::sync::oneshot::Sender<T>>,
    handle: EntityHandle,
    channel: Arc<StdMutex<OneshotRuntimeState>>,
}

pub struct OneshotReceiver<T> {
    inner: Option<tokio::sync::oneshot::Receiver<T>>,
    handle: EntityHandle,
    channel: Arc<StdMutex<OneshotRuntimeState>>,
    name: CompactString,
}

pub struct BroadcastSender<T> {
    inner: tokio::sync::broadcast::Sender<T>,
    handle: EntityHandle,
    receiver_handle: EntityHandle,
    channel: Arc<StdMutex<BroadcastRuntimeState>>,
    name: CompactString,
}

pub struct BroadcastReceiver<T> {
    inner: tokio::sync::broadcast::Receiver<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<BroadcastRuntimeState>>,
    name: CompactString,
}

pub struct WatchSender<T> {
    inner: tokio::sync::watch::Sender<T>,
    handle: EntityHandle,
    receiver_handle: EntityHandle,
    channel: Arc<StdMutex<WatchRuntimeState>>,
    name: CompactString,
}

pub struct WatchReceiver<T> {
    inner: tokio::sync::watch::Receiver<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<WatchRuntimeState>>,
    name: CompactString,
}

pub(super) struct ChannelRuntimeState {
    tx_id: EntityId,
    rx_id: EntityId,
    tx_ref_count: u32,
    rx_state: ReceiverState,
    queue_len: u32,
    capacity: Option<u32>,
    tx_close_cause: Option<ChannelCloseCause>,
    rx_close_cause: Option<ChannelCloseCause>,
}

pub(super) struct OneshotRuntimeState {
    tx_id: EntityId,
    rx_id: EntityId,
    tx_lifecycle: ChannelEndpointLifecycle,
    rx_lifecycle: ChannelEndpointLifecycle,
    state: OneshotState,
}

pub(super) struct BroadcastRuntimeState {
    tx_id: EntityId,
    rx_id: EntityId,
    tx_ref_count: u32,
    rx_ref_count: u32,
    capacity: u32,
    tx_close_cause: Option<ChannelCloseCause>,
    rx_close_cause: Option<ChannelCloseCause>,
}

pub(super) struct WatchRuntimeState {
    tx_id: EntityId,
    rx_id: EntityId,
    tx_ref_count: u32,
    rx_ref_count: u32,
    tx_close_cause: Option<ChannelCloseCause>,
    rx_close_cause: Option<ChannelCloseCause>,
    last_update_at: Option<peeps_types::PTime>,
}

pub(super) enum ReceiverState {
    Alive,
    Dropped,
}

impl ChannelRuntimeState {
    pub(super) fn tx_lifecycle(&self) -> ChannelEndpointLifecycle {
        match self.tx_close_cause {
            Some(cause) => ChannelEndpointLifecycle::Closed(cause),
            None => ChannelEndpointLifecycle::Open,
        }
    }

    pub(super) fn rx_lifecycle(&self) -> ChannelEndpointLifecycle {
        match self.rx_close_cause {
            Some(cause) => ChannelEndpointLifecycle::Closed(cause),
            None => ChannelEndpointLifecycle::Open,
        }
    }

    pub(super) fn is_send_full(&self) -> bool {
        self.capacity
            .map(|capacity| self.queue_len >= capacity)
            .unwrap_or(false)
    }

    pub(super) fn is_receive_empty(&self) -> bool {
        self.queue_len == 0
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T> Clone for BroadcastSender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            receiver_handle: self.receiver_handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T: Clone> Clone for BroadcastReceiver<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.resubscribe(),
            handle: self.handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T> Clone for WatchSender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            receiver_handle: self.receiver_handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T> Clone for WatchReceiver<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

pub(super) fn sync_channel_state(
    channel: &Arc<StdMutex<ChannelRuntimeState>>,
) -> Option<(
    EntityId,
    EntityId,
    Option<BufferState>,
    ChannelEndpointLifecycle,
    ChannelEndpointLifecycle,
)> {
    let state = channel.lock().ok()?;
    Some((
        EntityId::new(state.tx_id.as_str()),
        EntityId::new(state.rx_id.as_str()),
        Some(BufferState {
            occupancy: state.queue_len,
            capacity: state.capacity,
        }),
        state.tx_lifecycle(),
        state.rx_lifecycle(),
    ))
}

pub(super) fn apply_channel_state(channel: &Arc<StdMutex<ChannelRuntimeState>>) {
    let Some((tx_id, rx_id, buffer, tx_lifecycle, rx_lifecycle)) = sync_channel_state(channel)
    else {
        return;
    };
    if let Ok(mut db) = runtime_db().lock() {
        db.update_channel_endpoint_state(&tx_id, tx_lifecycle, buffer);
        db.update_channel_endpoint_state(&rx_id, rx_lifecycle, buffer);
    }
}

pub(super) fn sync_oneshot_state(
    channel: &Arc<StdMutex<OneshotRuntimeState>>,
) -> Option<(
    EntityId,
    EntityId,
    OneshotState,
    ChannelEndpointLifecycle,
    ChannelEndpointLifecycle,
)> {
    let state = channel.lock().ok()?;
    Some((
        EntityId::new(state.tx_id.as_str()),
        EntityId::new(state.rx_id.as_str()),
        state.state,
        state.tx_lifecycle,
        state.rx_lifecycle,
    ))
}

pub(super) fn apply_oneshot_state(channel: &Arc<StdMutex<OneshotRuntimeState>>) {
    let Some((tx_id, rx_id, state, tx_lifecycle, rx_lifecycle)) = sync_oneshot_state(channel)
    else {
        return;
    };
    if let Ok(mut db) = runtime_db().lock() {
        db.update_oneshot_endpoint_state(&tx_id, tx_lifecycle, state);
        db.update_oneshot_endpoint_state(&rx_id, rx_lifecycle, state);
    }
}

pub(super) fn sync_broadcast_state(
    channel: &Arc<StdMutex<BroadcastRuntimeState>>,
) -> Option<(
    EntityId,
    EntityId,
    Option<BufferState>,
    ChannelEndpointLifecycle,
    ChannelEndpointLifecycle,
)> {
    let state = channel.lock().ok()?;
    let tx_lifecycle = match state.tx_close_cause {
        Some(cause) => ChannelEndpointLifecycle::Closed(cause),
        None => ChannelEndpointLifecycle::Open,
    };
    let rx_lifecycle = match state.rx_close_cause {
        Some(cause) => ChannelEndpointLifecycle::Closed(cause),
        None => ChannelEndpointLifecycle::Open,
    };
    Some((
        EntityId::new(state.tx_id.as_str()),
        EntityId::new(state.rx_id.as_str()),
        Some(BufferState {
            occupancy: 0,
            capacity: Some(state.capacity),
        }),
        tx_lifecycle,
        rx_lifecycle,
    ))
}

pub(super) fn apply_broadcast_state(channel: &Arc<StdMutex<BroadcastRuntimeState>>) {
    let Some((tx_id, rx_id, buffer, tx_lifecycle, rx_lifecycle)) = sync_broadcast_state(channel)
    else {
        return;
    };
    if let Ok(mut db) = runtime_db().lock() {
        db.update_channel_endpoint_state(&tx_id, tx_lifecycle, buffer);
        db.update_channel_endpoint_state(&rx_id, rx_lifecycle, buffer);
    }
}

pub(super) fn sync_watch_state(
    channel: &Arc<StdMutex<WatchRuntimeState>>,
) -> Option<(
    EntityId,
    EntityId,
    ChannelEndpointLifecycle,
    ChannelEndpointLifecycle,
    Option<peeps_types::PTime>,
)> {
    let state = channel.lock().ok()?;
    let tx_lifecycle = match state.tx_close_cause {
        Some(cause) => ChannelEndpointLifecycle::Closed(cause),
        None => ChannelEndpointLifecycle::Open,
    };
    let rx_lifecycle = match state.rx_close_cause {
        Some(cause) => ChannelEndpointLifecycle::Closed(cause),
        None => ChannelEndpointLifecycle::Open,
    };
    Some((
        EntityId::new(state.tx_id.as_str()),
        EntityId::new(state.rx_id.as_str()),
        tx_lifecycle,
        rx_lifecycle,
        state.last_update_at,
    ))
}

pub(super) fn apply_watch_state(channel: &Arc<StdMutex<WatchRuntimeState>>) {
    let Some((tx_id, rx_id, tx_lifecycle, rx_lifecycle, last_update_at)) =
        sync_watch_state(channel)
    else {
        return;
    };
    if let Ok(mut db) = runtime_db().lock() {
        db.update_channel_endpoint_state(&tx_id, tx_lifecycle, None);
        db.update_channel_endpoint_state(&rx_id, rx_lifecycle, None);
        db.update_watch_last_update(&tx_id, last_update_at);
        db.update_watch_last_update(&rx_id, last_update_at);
    }
}

pub(super) fn emit_channel_wait_started(
    target: &EntityId,
    kind: ChannelWaitKind,
    source: UnqualSource,
    cx: CrateContext,
) {
    if let Ok(event) = Event::channel_wait_started_with_source(
        EventTarget::Entity(target.clone()),
        &ChannelWaitStartedEvent { kind },
        source.into_compact_string(),
        Some(cx.manifest_dir()),
    ) {
        if let Ok(mut db) = runtime_db().lock() {
            db.record_event(event);
        }
    }
}

pub(super) fn emit_channel_wait_ended(
    target: &EntityId,
    kind: ChannelWaitKind,
    started: Instant,
    source: UnqualSource,
    cx: CrateContext,
) {
    let wait_ns = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    if let Ok(event) = Event::channel_wait_ended_with_source(
        EventTarget::Entity(target.clone()),
        &ChannelWaitEndedEvent { kind, wait_ns },
        source.into_compact_string(),
        Some(cx.manifest_dir()),
    ) {
        if let Ok(mut db) = runtime_db().lock() {
            db.record_event(event);
        }
    }
}

pub(super) fn emit_channel_closed(target: &EntityId, cause: ChannelCloseCause) {
    if let Ok(event) = Event::channel_closed(
        EventTarget::Entity(target.clone()),
        &ChannelClosedEvent { cause },
    ) {
        record_event_with_entity_source(event, target);
    }
}
