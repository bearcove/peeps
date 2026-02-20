// r[impl api.mpsc]
use super::capture_backtrace_id;

use moire_runtime::{
    instrument_operation_on_with_source, record_event_with_source, AsEntityRef, EntityHandle,
    EntityRef, WeakEntityHandle,
};
use moire_types::{
    EdgeKind, EntityBody, Event, EventKind, EventTarget, MpscRxEntity, MpscTxEntity,
};
use tokio::sync::mpsc;

/// Instrumented version of [`tokio::sync::mpsc::Sender`].
///
/// Tracks queue length and send activity for diagnostics.
pub struct Sender<T> {
    inner: tokio::sync::mpsc::Sender<T>,
    handle: EntityHandle<moire_types::MpscTx>,
}

/// Instrumented version of [`tokio::sync::mpsc::Receiver`].
///
/// Tracks receive activity and queue length for diagnostics.
pub struct Receiver<T> {
    inner: tokio::sync::mpsc::Receiver<T>,
    handle: EntityHandle<moire_types::MpscRx>,
    tx_handle: WeakEntityHandle<moire_types::MpscTx>,
}

/// Instrumented version of [`tokio::sync::mpsc::UnboundedSender`].
/// Tracks unbounded send activity for diagnostics.
pub struct UnboundedSender<T> {
    inner: tokio::sync::mpsc::UnboundedSender<T>,
    handle: EntityHandle<moire_types::MpscTx>,
}

/// Instrumented version of [`tokio::sync::mpsc::UnboundedReceiver`].
/// Tracks unbounded receive activity for diagnostics.
pub struct UnboundedReceiver<T> {
    inner: tokio::sync::mpsc::UnboundedReceiver<T>,
    handle: EntityHandle<moire_types::MpscRx>,
    tx_handle: WeakEntityHandle<moire_types::MpscTx>,
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
    pub fn handle(&self) -> &EntityHandle<moire_types::MpscTx> {
        &self.handle
    }

    /// Attempts to enqueue a value without waiting, equivalent to [`tokio::sync::mpsc::Sender::try_send`].
    pub fn try_send(&self, value: T) -> Result<(), mpsc::error::TrySendError<T>> {
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

    /// Returns true if the sender is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Sends a value and awaits slot availability, matching [`tokio::sync::mpsc::Sender::send`].
    pub async fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
        let source = capture_backtrace_id();
        let result =
            instrument_operation_on_with_source(&self.handle, self.inner.send(value), source).await;
        if result.is_ok() {
            let _ = self
                .handle
                .mutate(|body| body.queue_len = body.queue_len.saturating_add(1));
        }
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelSent,
            source,
        );
        record_event_with_source(event, source);
        result
    }
}

impl<T> Receiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::MpscRx> {
        &self.handle
    }
    /// Receives the next message, matching [`tokio::sync::mpsc::Receiver::recv`].
    pub async fn recv(&mut self) -> Option<T> {
        let source = capture_backtrace_id();
        let result =
            instrument_operation_on_with_source(&self.handle, self.inner.recv(), source).await;
        if result.is_some() {
            let _ = self
                .tx_handle
                .mutate(|body| body.queue_len = body.queue_len.saturating_sub(1));
        }
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelReceived,
            source,
        );
        record_event_with_source(event, source);
        result
    }

    /// Closes the receive half, equivalent to [`tokio::sync::mpsc::Receiver::close`].
    pub fn close(&mut self) {
        self.inner.close();
    }
}

impl<T> UnboundedSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::MpscTx> {
        &self.handle
    }
    /// Sends a value on an unbounded channel, matching [`tokio::sync::mpsc::UnboundedSender::send`].
    pub fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
        let source = capture_backtrace_id();
        match self.inner.send(value) {
            Ok(()) => {
                let _ = self
                    .handle
                    .mutate(|body| body.queue_len = body.queue_len.saturating_add(1));
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                    source,
                );
                record_event_with_source(event, source);
                Ok(())
            }
            Err(err) => {
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                    source,
                );
                record_event_with_source(event, source);
                Err(err)
            }
        }
    }

    /// Returns true if the unbounded sender is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl<T> UnboundedReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::MpscRx> {
        &self.handle
    }
    /// Receives the next unbounded message, matching [`tokio::sync::mpsc::UnboundedReceiver::recv`].
    pub async fn recv(&mut self) -> Option<T> {
        let source = capture_backtrace_id();
        let result =
            instrument_operation_on_with_source(&self.handle, self.inner.recv(), source).await;
        if result.is_some() {
            let _ = self
                .tx_handle
                .mutate(|body| body.queue_len = body.queue_len.saturating_sub(1));
        }
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelReceived,
            source,
        );
        record_event_with_source(event, source);
        result
    }

    /// Closes the unbounded receive half.
    pub fn close(&mut self) {
        self.inner.close();
    }
}

/// Creates a bounded channel, equivalent to [`tokio::sync::mpsc::channel`].
pub fn channel<T>(name: impl Into<String>, capacity: usize) -> (Sender<T>, Receiver<T>) {
    let source = capture_backtrace_id();
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
    .into_typed::<moire_types::MpscTx>();

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::MpscRx(MpscRxEntity {}),
        source,
    )
    .into_typed::<moire_types::MpscRx>();

    tx_handle.link_to_handle_with_source(&rx_handle, EdgeKind::PairedWith, source);

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

/// Creates an unbounded channel, equivalent to [`tokio::sync::mpsc::unbounded_channel`].
pub fn unbounded_channel<T>(name: impl Into<String>) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let source = capture_backtrace_id();
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
    .into_typed::<moire_types::MpscTx>();

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::MpscRx(MpscRxEntity {}),
        source,
    )
    .into_typed::<moire_types::MpscRx>();

    tx_handle.link_to_handle_with_source(&rx_handle, EdgeKind::PairedWith, source);

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
