// r[impl api.broadcast]

use moire_runtime::{
    new_event, record_event, AsEntityRef, EntityHandle, EntityRef, WeakEntityHandle,
};
use moire_types::{
    BroadcastRxEntity, BroadcastTxEntity, EdgeKind, EventKind, EventTarget,
};
use tokio::sync::broadcast;

/// Instrumented version of [`tokio::sync::broadcast::Sender`].
///
/// This wraps the Tokio broadcast sender and records send/subscribe lifecycle.
pub struct BroadcastSender<T> {
    inner: tokio::sync::broadcast::Sender<T>,
    handle: EntityHandle<moire_types::BroadcastTx>,
}

/// Instrumented version of [`tokio::sync::broadcast::Receiver`].
///
/// This wraps the Tokio broadcast receiver and records message receive events.
pub struct BroadcastReceiver<T> {
    inner: tokio::sync::broadcast::Receiver<T>,
    handle: EntityHandle<moire_types::BroadcastRx>,
    tx_handle: WeakEntityHandle<moire_types::BroadcastTx>,
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
        Self {
            inner: self.inner.resubscribe(),
            handle: self.handle.clone(),
            tx_handle: self.tx_handle.clone(),
        }
    }
}

impl<T: Clone> BroadcastSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::BroadcastTx> {
        &self.handle
    }
    /// Subscribes a receiver, equivalent to [`tokio::sync::broadcast::Sender::subscribe`].
    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        let handle = EntityHandle::new("broadcast:rx.subscribe", BroadcastRxEntity { lag: 0 });
        self.handle
            .link_to_handle(&handle, EdgeKind::PairedWith);
        BroadcastReceiver {
            inner: self.inner.subscribe(),
            handle,
            tx_handle: self.handle.downgrade(),
        }
    }
    /// Sends a value through the channel, mirroring [`tokio::sync::broadcast::Sender::send`].
    pub fn send(&self, value: T) -> Result<usize, broadcast::error::SendError<T>> {
                let result = self.inner.send(value);
        let event = new_event(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelSent, 
        );
        record_event(event);
        result
    }
}

impl<T: Clone> BroadcastReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::BroadcastRx> {
        &self.handle
    }
    /// Receives the next broadcast value, equivalent to [`tokio::sync::broadcast::Receiver::recv`].
    pub async fn recv(&mut self) -> Result<T, broadcast::error::RecvError> {
                match self.inner.recv().await {
            Ok(value) => {
                let lag = self.inner.len().min(u32::MAX as usize) as u32;
                let _ = self.handle.mutate(|body| body.lag = lag);
                let event = new_event(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelReceived, 
                );
                record_event(event);
                Ok(value)
            }
            Err(err) => {
                if let broadcast::error::RecvError::Lagged(n) = err {
                    let lag = n.min(u32::MAX as u64) as u32;
                    let _ = self.handle.mutate(|body| body.lag = lag);
                }
                let event = new_event(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelReceived, 
                );
                record_event(event);
                Err(err)
            }
        }
    }
}

/// Creates an instrumented broadcast channel, matching [`tokio::sync::broadcast::channel`].
pub fn broadcast<T: Clone>(
    name: impl Into<String>,
    capacity: usize,
) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
        let name = name.into();
    let (tx, rx) = broadcast::channel(capacity);
    let capacity_u32 = capacity.min(u32::MAX as usize) as u32;

    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        BroadcastTxEntity {
            capacity: capacity_u32,
        },
    );

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        BroadcastRxEntity { lag: 0 },
    );

    tx_handle.link_to_handle(&rx_handle, EdgeKind::PairedWith);

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

impl<T: Clone> AsEntityRef for BroadcastSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
