use super::*;

use peeps_types::{
    ChannelCloseCause, ChannelDetails, ChannelEndpointEntity, ChannelEndpointLifecycle,
    ChannelReceiveEvent, ChannelReceiveOutcome, ChannelSendEvent, ChannelSendOutcome, EdgeKind,
    EntityBody, EntityId, Event, EventTarget, OperationKind, WatchChannelDetails,
};
use std::future::Future;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::watch;

impl<T> Drop for WatchSender<T> {
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
        apply_watch_state(&self.channel);
        if let Some(rx_id) = emit_for_rx {
            emit_channel_closed(&rx_id, ChannelCloseCause::SenderDropped);
        }
    }
}

impl<T> Drop for WatchReceiver<T> {
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
        apply_watch_state(&self.channel);
        if let Some(tx_id) = emit_for_tx {
            emit_channel_closed(&tx_id, ChannelCloseCause::ReceiverDropped);
        }
    }
}

impl<T: Clone> WatchSender<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn send_with_cx(
        &self,
        value: T,
        cx: CrateContext,
    ) -> Result<(), watch::error::SendError<T>> {
        self.send_with_source(value, cx.join(UnqualSource::caller()))
    }

    pub fn send_with_source(
        &self,
        value: T,
        source: Source,
    ) -> Result<(), watch::error::SendError<T>> {
        match self.inner.send(value) {
            Ok(()) => {
                let now = peeps_types::PTime::now();
                if let Ok(mut state) = self.channel.lock() {
                    state.last_update_at = Some(now);
                }
                apply_watch_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Ok,
                        queue_len: None,
                    },
                ) {
                    record_event_with_source(event, &source);
                }
                Ok(())
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
                apply_watch_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Closed,
                        queue_len: None,
                    },
                ) {
                    record_event_with_source(event, &source);
                }
                Err(err)
            }
        }
    }

    #[track_caller]
    pub fn send_replace_with_cx(&self, value: T, cx: CrateContext) -> T {
        self.send_replace_with_source(value, cx.join(UnqualSource::caller()))
    }

    pub fn send_replace_with_source(
        &self,
        value: T,
        _source: Source,
    ) -> T {
        let old = self.inner.send_replace(value);
        let now = peeps_types::PTime::now();
        if let Ok(mut state) = self.channel.lock() {
            state.last_update_at = Some(now);
        }
        apply_watch_state(&self.channel);
        old
    }

    #[track_caller]
    pub fn subscribe(&self) -> WatchReceiver<T> {
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_add(1);
        }
        WatchReceiver {
            inner: self.inner.subscribe(),
            handle: self.receiver_handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T: Clone> WatchReceiver<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn changed_with_cx(
        &mut self,
        cx: CrateContext,
    ) -> impl Future<Output = Result<(), watch::error::RecvError>> + '_ {
        self.changed_with_source(cx.join(UnqualSource::caller()))
    }

    #[allow(clippy::manual_async_fn)]
    pub fn changed_with_source(
        &mut self,
        source: Source,
    ) -> impl Future<Output = Result<(), watch::error::RecvError>> + '_ {
        async move {
            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Recv,
                self.inner.changed(),
                &source,
            )
            .await;
            match result {
                Ok(()) => {
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Ok,
                            queue_len: None,
                        },
                    ) {
                        record_event_with_source(event, &source);
                    }
                    Ok(())
                }
                Err(err) => {
                    if let Ok(mut state) = self.channel.lock() {
                        if state.tx_close_cause.is_none() {
                            state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                        }
                        if state.rx_close_cause.is_none() {
                            state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                        }
                    }
                    apply_watch_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Closed,
                            queue_len: None,
                        },
                    ) {
                        record_event_with_source(event, &source);
                    }
                    Err(err)
                }
            }
        }
    }

    #[track_caller]
    pub fn borrow(&self) -> watch::Ref<'_, T> {
        self.inner.borrow()
    }

    #[track_caller]
    pub fn borrow_and_update(&mut self) -> watch::Ref<'_, T> {
        self.inner.borrow_and_update()
    }

    #[track_caller]
    pub fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.inner.has_changed()
    }
}

pub fn watch<T: Clone>(
    name: impl Into<CompactString>,
    initial: T,
    source: UnqualSource,
) -> (WatchSender<T>, WatchReceiver<T>) {
    let name = name.into();
    let (tx, rx) = watch::channel(initial);
    let details = ChannelDetails::Watch(WatchChannelDetails {
        last_update_at: None,
    });
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    let details = ChannelDetails::Watch(WatchChannelDetails {
        last_update_at: None,
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
    let channel = Arc::new(StdMutex::new(WatchRuntimeState {
        tx_id: tx_handle.id().clone(),
        rx_id: rx_handle.id().clone(),
        tx_ref_count: 1,
        rx_ref_count: 1,
        tx_close_cause: None,
        rx_close_cause: None,
        last_update_at: None,
    }));
    (
        WatchSender {
            inner: tx,
            handle: tx_handle,
            receiver_handle: rx_handle.clone(),
            channel: channel.clone(),
            name: name.clone(),
        },
        WatchReceiver {
            inner: rx,
            handle: rx_handle,
            channel,
            name,
        },
    )
}

pub fn watch_channel<T: Clone>(
    name: impl Into<CompactString>,
    initial: T,
    source: UnqualSource,
) -> (WatchSender<T>, WatchReceiver<T>) {
    #[allow(deprecated)]
    watch(name, initial, source)
}

impl<T: Clone> AsEntityRef for WatchSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T: Clone> AsEntityRef for WatchReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
