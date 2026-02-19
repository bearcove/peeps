use super::{local_source, Source, SourceRight};

use peeps_runtime::{
    record_event_with_source, AsEntityRef, EntityHandle, EntityRef, WeakEntityHandle,
};
use peeps_types::{
    BroadcastRxEntity, BroadcastTxEntity, EdgeKind, EntityBody, Event, EventKind, EventTarget,
};
use tokio::sync::broadcast;

pub struct BroadcastSender<T> {
    inner: tokio::sync::broadcast::Sender<T>,
    handle: EntityHandle<peeps_types::BroadcastTx>,
}

pub struct BroadcastReceiver<T> {
    inner: tokio::sync::broadcast::Receiver<T>,
    handle: EntityHandle<peeps_types::BroadcastRx>,
    tx_handle: WeakEntityHandle<peeps_types::BroadcastTx>,
}

impl<T> Clone for BroadcastSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
        }
    }
}

impl<T: Clone> Clone for BroadcastReceiver<T> {
    fn clone(&self) -> Self {
        let handle = EntityHandle::new(
            "broadcast:rx.clone",
            EntityBody::BroadcastRx(BroadcastRxEntity { lag: 0 }),
            local_source(SourceRight::caller()),
        )
        .into_typed::<peeps_types::BroadcastRx>();
        Self {
            inner: self.inner.resubscribe(),
            handle,
            tx_handle: self.tx_handle.clone(),
        }
    }
}

impl<T: Clone> BroadcastSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::BroadcastTx> {
        &self.handle
    }

    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        let handle = EntityHandle::new(
            "broadcast:rx.subscribe",
            EntityBody::BroadcastRx(BroadcastRxEntity { lag: 0 }),
            local_source(SourceRight::caller()),
        )
        .into_typed::<peeps_types::BroadcastRx>();
        self.handle.link_to_handle_with_source(
            &handle,
            EdgeKind::PairedWith,
            local_source(SourceRight::caller()),
        );
        BroadcastReceiver {
            inner: self.inner.subscribe(),
            handle,
            tx_handle: self.handle.downgrade(),
        }
    }

    #[doc(hidden)]
    pub fn send_with_source(
        &self,
        value: T,
        source: Source,
    ) -> Result<usize, broadcast::error::SendError<T>> {
        let result = self.inner.send(value);
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelSent,
            source.clone(),
        );
        record_event_with_source(event, &source);
        result
    }
}

impl<T: Clone> BroadcastReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::BroadcastRx> {
        &self.handle
    }

    #[doc(hidden)]
    pub async fn recv_with_source(
        &mut self,
        source: Source,
    ) -> Result<T, broadcast::error::RecvError> {
        match self.inner.recv().await {
            Ok(value) => {
                let lag = self.inner.len().min(u32::MAX as usize) as u32;
                let _ = self.handle.mutate(|body| body.lag = lag);
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelReceived,
                    source.clone(),
                );
                record_event_with_source(event, &source);
                Ok(value)
            }
            Err(err) => {
                if let broadcast::error::RecvError::Lagged(n) = err {
                    let lag = n.min(u32::MAX as u64) as u32;
                    let _ = self.handle.mutate(|body| body.lag = lag);
                }
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelReceived,
                    source.clone(),
                );
                record_event_with_source(event, &source);
                Err(err)
            }
        }
    }
}

pub fn broadcast<T: Clone>(
    name: impl Into<String>,
    capacity: usize,
    source: SourceRight,
) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
    let name = name.into();
    let (tx, rx) = broadcast::channel(capacity);
    let capacity_u32 = capacity.min(u32::MAX as usize) as u32;

    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::BroadcastTx(BroadcastTxEntity {
            capacity: capacity_u32,
        }),
        local_source(source),
    )
    .into_typed::<peeps_types::BroadcastTx>();

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::BroadcastRx(BroadcastRxEntity { lag: 0 }),
        local_source(source),
    )
    .into_typed::<peeps_types::BroadcastRx>();

    tx_handle.link_to_handle_with_source(
        &rx_handle,
        EdgeKind::PairedWith,
        local_source(source),
    );

    (
        BroadcastSender {
            inner: tx,
            handle: tx_handle.clone(),
        },
        BroadcastReceiver {
            inner: rx,
            handle: rx_handle,
            tx_handle: tx_handle.downgrade(),
        },
    )
}

pub fn broadcast_channel<T: Clone>(
    name: impl Into<String>,
    capacity: usize,
    source: SourceRight,
) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
    #[allow(deprecated)]
    broadcast(name, capacity, source)
}

impl<T: Clone> AsEntityRef for BroadcastSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
