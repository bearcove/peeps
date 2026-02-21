// r[impl api.oneshot]

pub use tokio::sync::oneshot::error;

use moire_runtime::{
    EntityHandle, WeakEntityHandle, instrument_operation_on, new_event, record_event,
};
use moire_types::{EdgeKind, EventKind, EventTarget, OneshotRxEntity, OneshotTxEntity};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::oneshot;

/// Instrumented version of [`tokio::sync::oneshot::Sender`].
///
/// Tracks send outcome for diagnostics.
pub struct Sender<T> {
    inner: Option<tokio::sync::oneshot::Sender<T>>,
    handle: EntityHandle<moire_types::OneshotTx>,
}

/// Instrumented version of [`tokio::sync::oneshot::Receiver`].
///
/// Tracks receive events for diagnostics.
pub struct Receiver<T> {
    inner: tokio::sync::oneshot::Receiver<T>,
    handle: EntityHandle<moire_types::OneshotRx>,
    _tx_handle: WeakEntityHandle<moire_types::OneshotTx>,
}

/// Future returned by awaiting a [`Receiver`].
pub struct ReceiverFuture<T> {
    inner: moire_runtime::OperationFuture<tokio::sync::oneshot::Receiver<T>>,
    handle: EntityHandle<moire_types::OneshotRx>,
    _tx_handle: WeakEntityHandle<moire_types::OneshotTx>,
}

impl<T> Future for ReceiverFuture<T> {
    type Output = Result<T, oneshot::error::RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        match unsafe { Pin::new_unchecked(&mut this.inner) }.poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                let event = new_event(
                    EventTarget::Entity(this.handle.id().clone()),
                    EventKind::ChannelReceived,
                );
                record_event(event);
                Poll::Ready(result)
            }
        }
    }
}

impl<T> IntoFuture for Receiver<T> {
    type Output = Result<T, oneshot::error::RecvError>;
    type IntoFuture = ReceiverFuture<T>;

    fn into_future(self) -> Self::IntoFuture {
        ReceiverFuture {
            inner: instrument_operation_on(&self.handle, self.inner),
            handle: self.handle,
            _tx_handle: self._tx_handle,
        }
    }
}

impl<T> Sender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::OneshotTx> {
        &self.handle
    }
    /// Sends a single value, equivalent to [`tokio::sync::oneshot::Sender::send`].
    /// Records a one-shot send event and consumption status.
    pub fn send(mut self, value: T) -> Result<(), T> {
        let Some(inner) = self.inner.take() else {
            return Err(value);
        };
        match inner.send(value) {
            Ok(()) => {
                let _ = self.handle.mutate(|body| body.sent = true);
                let event = new_event(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                );
                record_event(event);
                Ok(())
            }
            Err(value) => {
                let event = new_event(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                );
                record_event(event);
                Err(value)
            }
        }
    }
}

impl<T> Receiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::OneshotRx> {
        &self.handle
    }
}

/// Creates an instrumented oneshot channel, equivalent to [`tokio::sync::oneshot::channel`].
pub fn channel<T>(name: impl Into<String>) -> (Sender<T>, Receiver<T>) {
    let name: String = name.into();
    let (tx, rx) = oneshot::channel();

    let tx_handle = EntityHandle::new(format!("{name}:tx"), OneshotTxEntity { sent: false });

    let rx_handle = EntityHandle::new(format!("{name}:rx"), OneshotRxEntity {});

    tx_handle.link_to_handle(&rx_handle, EdgeKind::PairedWith);

    (
        Sender {
            inner: Some(tx),
            handle: tx_handle.clone(),
        },
        Receiver {
            inner: rx,
            handle: rx_handle,
            _tx_handle: tx_handle.downgrade(),
        },
    )
}
