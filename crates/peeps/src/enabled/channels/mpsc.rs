use super::{Source, SourceRight};

use peeps_runtime::{
    current_causal_target, instrument_operation_on_with_source, record_event_with_source,
    AsEntityRef, EntityHandle, EntityRef, WeakEntityHandle,
};
use peeps_types::{
    EdgeKind, EntityBody, Event, EventKind, EventTarget, MpscRxEntity, MpscTxEntity,
};
use tokio::sync::mpsc;

pub struct Sender<T> {
    inner: tokio::sync::mpsc::Sender<T>,
    handle: EntityHandle<peeps_types::MpscTx>,
}

pub struct Receiver<T> {
    inner: tokio::sync::mpsc::Receiver<T>,
    handle: EntityHandle<peeps_types::MpscRx>,
    tx_handle: WeakEntityHandle<peeps_types::MpscTx>,
}

pub struct UnboundedSender<T> {
    inner: tokio::sync::mpsc::UnboundedSender<T>,
    handle: EntityHandle<peeps_types::MpscTx>,
}

pub struct UnboundedReceiver<T> {
    inner: tokio::sync::mpsc::UnboundedReceiver<T>,
    handle: EntityHandle<peeps_types::MpscRx>,
    tx_handle: WeakEntityHandle<peeps_types::MpscTx>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
        }
    }
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
        }
    }
}

impl<T> Sender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::MpscTx> {
        &self.handle
    }

    pub fn try_send(&self, value: T) -> Result<(), mpsc::error::TrySendError<T>> {
        if let Some(caller) = current_causal_target() {
            self.handle.link_to(&caller, EdgeKind::Polls);
        }
        match self.inner.try_send(value) {
            Ok(()) => {
                let _ = self
                    .handle
                    .mutate(|body| body.queue_len = body.queue_len.saturating_add(1));
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    #[doc(hidden)]
    pub async fn send_with_source(&self, value: T, source: Source) -> Result<(), mpsc::error::SendError<T>> {
        let result =
            instrument_operation_on_with_source(&self.handle, self.inner.send(value), &source).await;
        if result.is_ok() {
            let _ = self
                .handle
                .mutate(|body| body.queue_len = body.queue_len.saturating_add(1));
        }
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelSent,
            source.clone(),
        );
        record_event_with_source(event, &source);
        result
    }
}

impl<T> Receiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::MpscRx> {
        &self.handle
    }

    #[doc(hidden)]
    pub async fn recv_with_source(&mut self, source: Source) -> Option<T> {
        let result =
            instrument_operation_on_with_source(&self.handle, self.inner.recv(), &source).await;
        if result.is_some() {
            let _ = self
                .tx_handle
                .mutate(|body| body.queue_len = body.queue_len.saturating_sub(1));
        }
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelReceived,
            source.clone(),
        );
        record_event_with_source(event, &source);
        result
    }

    pub fn close(&mut self) {
        self.inner.close();
    }
}

impl<T> UnboundedSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::MpscTx> {
        &self.handle
    }

    #[doc(hidden)]
    pub fn send_with_source(&self, value: T, source: Source) -> Result<(), mpsc::error::SendError<T>> {
        if let Some(caller) = current_causal_target() {
            self.handle.link_to(&caller, EdgeKind::Polls);
        }
        match self.inner.send(value) {
            Ok(()) => {
                let _ = self
                    .handle
                    .mutate(|body| body.queue_len = body.queue_len.saturating_add(1));
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                    source.clone(),
                );
                record_event_with_source(event, &source);
                Ok(())
            }
            Err(err) => {
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                    source.clone(),
                );
                record_event_with_source(event, &source);
                Err(err)
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl<T> UnboundedReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::MpscRx> {
        &self.handle
    }

    #[doc(hidden)]
    pub async fn recv_with_source(&mut self, source: Source) -> Option<T> {
        let result =
            instrument_operation_on_with_source(&self.handle, self.inner.recv(), &source).await;
        if result.is_some() {
            let _ = self
                .tx_handle
                .mutate(|body| body.queue_len = body.queue_len.saturating_sub(1));
        }
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelReceived,
            source.clone(),
        );
        record_event_with_source(event, &source);
        result
    }

    pub fn close(&mut self) {
        self.inner.close();
    }
}

pub fn channel<T>(name: impl Into<String>, capacity: usize, source: SourceRight) -> (Sender<T>, Receiver<T>) {
    let name = name.into();
    let (tx, rx) = mpsc::channel(capacity);
    let capacity_u32 = capacity.min(u32::MAX as usize) as u32;

    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::MpscTx(MpscTxEntity {
            queue_len: 0,
            capacity: Some(capacity_u32),
        }),
        source,
    )
    .into_typed::<peeps_types::MpscTx>();

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::MpscRx(MpscRxEntity {}),
        source,
    )
    .into_typed::<peeps_types::MpscRx>();

    tx_handle.link_to_handle(&rx_handle, EdgeKind::PairedWith);

    (
        Sender {
            inner: tx,
            handle: tx_handle.clone(),
        },
        Receiver {
            inner: rx,
            handle: rx_handle,
            tx_handle: tx_handle.downgrade(),
        },
    )
}

pub fn mpsc_channel<T>(name: impl Into<String>, capacity: usize, source: SourceRight) -> (Sender<T>, Receiver<T>) {
    #[allow(deprecated)]
    channel(name, capacity, source)
}

pub fn unbounded_channel<T>(name: impl Into<String>, source: SourceRight) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let name = name.into();
    let (tx, rx) = mpsc::unbounded_channel();

    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::MpscTx(MpscTxEntity {
            queue_len: 0,
            capacity: None,
        }),
        source,
    )
    .into_typed::<peeps_types::MpscTx>();

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::MpscRx(MpscRxEntity {}),
        source,
    )
    .into_typed::<peeps_types::MpscRx>();

    tx_handle.link_to_handle(&rx_handle, EdgeKind::PairedWith);

    (
        UnboundedSender {
            inner: tx,
            handle: tx_handle.clone(),
        },
        UnboundedReceiver {
            inner: rx,
            handle: rx_handle,
            tx_handle: tx_handle.downgrade(),
        },
    )
}

pub fn mpsc_unbounded_channel<T>(
    name: impl Into<String>,
    source: SourceRight,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    #[allow(deprecated)]
    unbounded_channel(name, source)
}

impl<T> AsEntityRef for Sender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for UnboundedSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
