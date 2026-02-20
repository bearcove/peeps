// r[impl api.oneshot]
use super::capture_backtrace_id;

use moire_runtime::{
    instrument_operation_on_with_source, record_event_with_source, EntityHandle, WeakEntityHandle,
};
use moire_types::{
    EdgeKind, EntityBody, Event, EventKind, EventTarget, OneshotRxEntity, OneshotTxEntity,
};
use tokio::sync::oneshot;

/// Instrumented version of [`tokio::sync::oneshot::Sender`].
///
/// Tracks send outcome for diagnostics.
pub struct OneshotSender<T> {
    inner: Option<tokio::sync::oneshot::Sender<T>>,
    handle: EntityHandle<moire_types::OneshotTx>,
}

/// Instrumented version of [`tokio::sync::oneshot::Receiver`].
///
/// Tracks receive events for diagnostics.
pub struct OneshotReceiver<T> {
    inner: Option<tokio::sync::oneshot::Receiver<T>>,
    handle: EntityHandle<moire_types::OneshotRx>,
    _tx_handle: WeakEntityHandle<moire_types::OneshotTx>,
}

impl<T> OneshotSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::OneshotTx> {
        &self.handle
    }
    /// Sends a single value, equivalent to [`tokio::sync::oneshot::Sender::send`].
    /// Records a one-shot send event and consumption status.
    pub fn send(mut self, value: T) -> Result<(), T> {
        let source = capture_backtrace_id();
        let Some(inner) = self.inner.take() else {
            return Err(value);
        };
        match inner.send(value) {
            Ok(()) => {
                let _ = self.handle.mutate(|body| body.sent = true);
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                    source,
                );
                record_event_with_source(event, source);
                Ok(())
            }
            Err(value) => {
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                    source,
                );
                record_event_with_source(event, source);
                Err(value)
            }
        }
    }
}

impl<T> OneshotReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::OneshotRx> {
        &self.handle
    }
    /// Waits for the oneshot message, matching [`tokio::sync::oneshot::Receiver::await`].
    /// Equivalent to receiving the value in Tokio's oneshot receiver API.
    pub async fn recv(mut self) -> Result<T, oneshot::error::RecvError> {
        let source = capture_backtrace_id();
        let inner = self.inner.take().expect("oneshot receiver consumed");
        let result = instrument_operation_on_with_source(&self.handle, inner, source).await;
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelReceived,
            source,
        );
        record_event_with_source(event, source);
        result
    }
}

/// Creates an instrumented oneshot channel, equivalent to [`tokio::sync::oneshot::channel`].
pub fn oneshot<T>(name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let source = capture_backtrace_id();
    let name: String = name.into();
    let (tx, rx) = oneshot::channel();

    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::OneshotTx(OneshotTxEntity { sent: false }),
        source,
    )
    .into_typed::<moire_types::OneshotTx>();

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::OneshotRx(OneshotRxEntity {}),
        source,
    )
    .into_typed::<moire_types::OneshotRx>();

    tx_handle.link_to_handle_with_source(&rx_handle, EdgeKind::PairedWith, source);

    (
        OneshotSender {
            inner: Some(tx),
            handle: tx_handle.clone(),
        },
        OneshotReceiver {
            inner: Some(rx),
            handle: rx_handle,
            _tx_handle: tx_handle.downgrade(),
        },
    )
}

/// Alias for [`oneshot`], kept for Tokio API parity.
pub fn oneshot_channel<T>(name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    oneshot(name)
}
