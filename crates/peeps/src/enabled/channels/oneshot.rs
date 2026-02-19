use super::*;

use peeps_types::{
    ChannelCloseCause, ChannelClosedEvent, ChannelDetails, ChannelEndpointEntity,
    ChannelEndpointLifecycle, ChannelReceiveEvent, ChannelReceiveOutcome, ChannelSendEvent,
    ChannelSendOutcome, EdgeKind, EntityBody, EntityId, Event, EventTarget, OneshotChannelDetails,
    OneshotState, OperationKind,
};
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::oneshot;

impl<T> Drop for OneshotSender<T> {
    fn drop(&mut self) {
        if self.inner.is_none() {
            return;
        }
        let mut emit_for_rx = None;
        if let Ok(mut state) = self.channel.lock() {
            if matches!(state.state, OneshotState::Pending) {
                state.state = OneshotState::SenderDropped;
                state.tx_lifecycle =
                    ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                state.rx_lifecycle =
                    ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                emit_for_rx = Some(EntityId::new(state.rx_id.as_str()));
            }
        }
        apply_oneshot_state(&self.channel);
        if let Some(rx_id) = emit_for_rx {
            emit_channel_closed(&rx_id, ChannelCloseCause::SenderDropped);
        }
    }
}

impl<T> Drop for OneshotReceiver<T> {
    fn drop(&mut self) {
        if self.inner.is_none() {
            return;
        }
        let mut emit_for_tx = None;
        if let Ok(mut state) = self.channel.lock() {
            if matches!(state.state, OneshotState::Pending | OneshotState::Sent) {
                state.state = OneshotState::ReceiverDropped;
                state.tx_lifecycle =
                    ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                state.rx_lifecycle =
                    ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                emit_for_tx = Some(EntityId::new(state.tx_id.as_str()));
            }
        }
        apply_oneshot_state(&self.channel);
        if let Some(tx_id) = emit_for_tx {
            emit_channel_closed(&tx_id, ChannelCloseCause::ReceiverDropped);
        }
    }
}

impl<T> OneshotSender<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn send_with_cx(self, value: T, cx: CrateContext) -> Result<(), T> {
        self.send_with_source(value, UnqualSource::caller(), cx)
    }

    pub fn send_with_source(
        mut self,
        value: T,
        source: UnqualSource,
        cx: CrateContext,
    ) -> Result<(), T> {
        let Some(inner) = self.inner.take() else {
            return Err(value);
        };
        match inner.send(value) {
            Ok(()) => {
                if let Ok(mut state) = self.channel.lock() {
                    state.state = OneshotState::Sent;
                    state.tx_lifecycle =
                        ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                }
                apply_oneshot_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Ok,
                        queue_len: None,
                    },
                ) {
                    record_event_with_source(event, source, cx);
                }
                Ok(())
            }
            Err(value) => {
                if let Ok(mut state) = self.channel.lock() {
                    state.state = OneshotState::ReceiverDropped;
                    state.tx_lifecycle =
                        ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                    state.rx_lifecycle =
                        ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                }
                apply_oneshot_state(&self.channel);
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
                Err(value)
            }
        }
    }
}

impl<T> OneshotReceiver<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn recv_with_cx(
        self,
        cx: CrateContext,
    ) -> impl Future<Output = Result<T, oneshot::error::RecvError>> {
        self.recv_with_source(UnqualSource::caller(), cx)
    }

    #[allow(clippy::manual_async_fn)]
    pub fn recv_with_source(
        mut self,
        source: UnqualSource,
        cx: CrateContext,
    ) -> impl Future<Output = Result<T, oneshot::error::RecvError>> {
        async move {
            let inner = self.inner.take().expect("oneshot receiver consumed");
            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Recv,
                inner,
                source,
                cx,
            )
            .await;
            match result {
                Ok(value) => {
                    if let Ok(mut state) = self.channel.lock() {
                        state.state = OneshotState::Received;
                        state.rx_lifecycle =
                            ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                    }
                    apply_oneshot_state(&self.channel);
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
                    if let Ok(mut state) = self.channel.lock() {
                        state.state = OneshotState::SenderDropped;
                        state.tx_lifecycle =
                            ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                        state.rx_lifecycle =
                            ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                    }
                    apply_oneshot_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Closed,
                            queue_len: None,
                        },
                    ) {
                        record_event_with_source(event, source, cx);
                    }
                    if let Ok(event) = Event::channel_closed(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelClosedEvent {
                            cause: ChannelCloseCause::SenderDropped,
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

pub fn oneshot<T>(
    name: impl Into<String>,
    source: UnqualSource,
) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let name: CompactString = name.into().into();
    let (tx, rx) = oneshot::channel();
    let details = ChannelDetails::Oneshot(OneshotChannelDetails {
        state: OneshotState::Pending,
    });
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    let details = ChannelDetails::Oneshot(OneshotChannelDetails {
        state: OneshotState::Pending,
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
    let channel = Arc::new(StdMutex::new(OneshotRuntimeState {
        tx_id: tx_handle.id().clone(),
        rx_id: rx_handle.id().clone(),
        tx_lifecycle: ChannelEndpointLifecycle::Open,
        rx_lifecycle: ChannelEndpointLifecycle::Open,
        state: OneshotState::Pending,
    }));
    (
        OneshotSender {
            inner: Some(tx),
            handle: tx_handle,
            channel: channel.clone(),
        },
        OneshotReceiver {
            inner: Some(rx),
            handle: rx_handle,
            channel,
            name,
        },
    )
}

pub fn oneshot_channel<T>(
    name: impl Into<String>,
    source: UnqualSource,
) -> (OneshotSender<T>, OneshotReceiver<T>) {
    #[allow(deprecated)]
    oneshot(name, source)
}

impl<T> AsEntityRef for OneshotSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for OneshotReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
