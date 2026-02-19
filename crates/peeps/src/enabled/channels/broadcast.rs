use super::*;

use peeps_types::{
    BufferState, ChannelCloseCause, ChannelClosedEvent, ChannelDetails, ChannelEndpointEntity,
    ChannelEndpointLifecycle, ChannelReceiveEvent, ChannelReceiveOutcome, ChannelSendEvent,
    ChannelSendOutcome, EdgeKind, EntityBody, EntityId, Event, EventTarget, OperationKind,
};
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::broadcast;

impl<T> Drop for BroadcastSender<T> {
    fn drop(&mut self) {
        let mut emit_for_rx = None;
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_sub(1);
            if state.tx_ref_count == 0 {
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                    emit_for_rx = Some(EntityId::new(state.rx_id.as_str()));
                }
            }
        }
        apply_broadcast_state(&self.channel);
        if let Some(rx_id) = emit_for_rx {
            emit_channel_closed(&rx_id, ChannelCloseCause::SenderDropped);
        }
    }
}

impl<T> Drop for BroadcastReceiver<T> {
    fn drop(&mut self) {
        let mut emit_for_tx = None;
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_sub(1);
            if state.rx_ref_count == 0 {
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    emit_for_tx = Some(EntityId::new(state.tx_id.as_str()));
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                }
            }
        }
        apply_broadcast_state(&self.channel);
        if let Some(tx_id) = emit_for_tx {
            emit_channel_closed(&tx_id, ChannelCloseCause::ReceiverDropped);
        }
    }
}

impl<T: Clone> BroadcastSender<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_add(1);
        }
        BroadcastReceiver {
            inner: self.inner.subscribe(),
            handle: self.receiver_handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }

    #[track_caller]
    pub fn send_with_cx(
        &self,
        value: T,
        cx: CrateContext,
    ) -> Result<usize, broadcast::error::SendError<T>> {
        self.send_with_source(value, UnqualSource::caller(), cx)
    }

    pub fn send_with_source(
        &self,
        value: T,
        source: UnqualSource,
        cx: CrateContext,
    ) -> Result<usize, broadcast::error::SendError<T>> {
        match self.inner.send(value) {
            Ok(receivers) => {
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Ok,
                        queue_len: None,
                    },
                ) {
                    record_event_with_source(event, source, cx);
                }
                Ok(receivers)
            }
            Err(err) => {
                if let Ok(mut state) = self.channel.lock() {
                    if state.tx_close_cause.is_none() {
                        state.tx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    }
                    if state.rx_close_cause.is_none() {
                        state.rx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    }
                }
                apply_broadcast_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Closed,
                        queue_len: None,
                    },
                ) {
                    record_event_with_source(event, source, cx);
                }
                if let Ok(event) = Event::channel_closed(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelClosedEvent {
                        cause: ChannelCloseCause::ReceiverDropped,
                    },
                ) {
                    record_event_with_source(event, source, cx);
                }
                Err(err)
            }
        }
    }
}

impl<T: Clone> BroadcastReceiver<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn recv_with_cx(
        &mut self,
        cx: CrateContext,
    ) -> impl Future<Output = Result<T, broadcast::error::RecvError>> + '_ {
        self.recv_with_source(UnqualSource::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn recv_with_source(
        &mut self,
        source: UnqualSource,
        cx: CrateContext,
    ) -> impl Future<Output = Result<T, broadcast::error::RecvError>> + '_ {
        async move {
            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Recv,
                self.inner.recv(),
                source,
                cx,
            )
            .await;
            match result {
                Ok(value) => {
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Ok,
                            queue_len: None,
                        },
                    ) {
                        record_event_with_source(event, source, cx);
                    }
                    Ok(value)
                }
                Err(err) => {
                    if let broadcast::error::RecvError::Closed = err {
                        if let Ok(mut state) = self.channel.lock() {
                            if state.tx_close_cause.is_none() {
                                state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                            }
                            if state.rx_close_cause.is_none() {
                                state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                            }
                        }
                        apply_broadcast_state(&self.channel);
                        if let Ok(event) = Event::channel_closed(
                            EventTarget::Entity(self.handle.id().clone()),
                            &ChannelClosedEvent {
                                cause: ChannelCloseCause::SenderDropped,
                            },
                        ) {
                            record_event_with_source(event, source, cx);
                        }
                    }
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Empty,
                            queue_len: None,
                        },
                    ) {
                        record_event_with_source(event, source, cx);
                    }
                    Err(err)
                }
            }
        }
    }
}

#[track_caller]
pub fn broadcast<T: Clone>(
    name: impl Into<CompactString>,
    capacity: usize,
    source: UnqualSource,
) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
    let name = name.into();
    let (tx, rx) = broadcast::channel(capacity);
    let capacity_u32 = capacity.min(u32::MAX as usize) as u32;
    let details = ChannelDetails::Broadcast(peeps_types::BroadcastChannelDetails {
        buffer: Some(BufferState {
            occupancy: 0,
            capacity: Some(capacity_u32),
        }),
    });
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    let details = ChannelDetails::Broadcast(peeps_types::BroadcastChannelDetails {
        buffer: Some(BufferState {
            occupancy: 0,
            capacity: Some(capacity_u32),
        }),
    });
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    let channel = Arc::new(StdMutex::new(BroadcastRuntimeState {
        tx_id: tx_handle.id().clone(),
        rx_id: rx_handle.id().clone(),
        tx_ref_count: 1,
        rx_ref_count: 1,
        capacity: capacity_u32,
        tx_close_cause: None,
        rx_close_cause: None,
    }));
    (
        BroadcastSender {
            inner: tx,
            handle: tx_handle,
            receiver_handle: rx_handle.clone(),
            channel: channel.clone(),
            name: name.clone(),
        },
        BroadcastReceiver {
            inner: rx,
            handle: rx_handle,
            channel,
            name,
        },
    )
}

impl<T: Clone> AsEntityRef for BroadcastSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T: Clone> AsEntityRef for BroadcastReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
