use super::{local_source, Source, SourceRight};

use peeps_runtime::{
    instrument_operation_on_with_source, record_event_with_source, EntityHandle, WeakEntityHandle,
};
use peeps_types::{
    EdgeKind, EntityBody, Event, EventKind, EventTarget, OneshotRxEntity, OneshotTxEntity,
};
use tokio::sync::oneshot;

pub struct OneshotSender<T> {
    inner: Option<tokio::sync::oneshot::Sender<T>>,
    handle: EntityHandle<peeps_types::OneshotTx>,
}

pub struct OneshotReceiver<T> {
    inner: Option<tokio::sync::oneshot::Receiver<T>>,
    handle: EntityHandle<peeps_types::OneshotRx>,
    _tx_handle: WeakEntityHandle<peeps_types::OneshotTx>,
}

impl<T> OneshotSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::OneshotTx> {
        &self.handle
    }

    #[doc(hidden)]
    pub fn send_with_source(mut self, value: T, source: Source) -> Result<(), T> {
        let Some(inner) = self.inner.take() else {
            return Err(value);
        };
        match inner.send(value) {
            Ok(()) => {
                let _ = self.handle.mutate(|body| body.sent = true);
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                    source.clone(),
                );
                record_event_with_source(event, &source);
                Ok(())
            }
            Err(value) => {
                let event = Event::new_with_source(
                    EventTarget::Entity(self.handle.id().clone()),
                    EventKind::ChannelSent,
                    source.clone(),
                );
                record_event_with_source(event, &source);
                Err(value)
            }
        }
    }
}

impl<T> OneshotReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::OneshotRx> {
        &self.handle
    }

    #[doc(hidden)]
    pub async fn recv_with_source(
        mut self,
        source: Source,
    ) -> Result<T, oneshot::error::RecvError> {
        let inner = self.inner.take().expect("oneshot receiver consumed");
        let result = instrument_operation_on_with_source(&self.handle, inner, &source).await;
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelReceived,
            source.clone(),
        );
        record_event_with_source(event, &source);
        result
    }
}

pub fn oneshot<T>(
    name: impl Into<String>,
    source: SourceRight,
) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let name: String = name.into();
    let (tx, rx) = oneshot::channel();

    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::OneshotTx(OneshotTxEntity { sent: false }),
        local_source(source),
    )
    .into_typed::<peeps_types::OneshotTx>();

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::OneshotRx(OneshotRxEntity {}),
        local_source(source),
    )
    .into_typed::<peeps_types::OneshotRx>();

    tx_handle.link_to_handle_with_source(
        &rx_handle,
        EdgeKind::PairedWith,
        local_source(source),
    );

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

pub fn oneshot_channel<T>(
    name: impl Into<String>,
    source: SourceRight,
) -> (OneshotSender<T>, OneshotReceiver<T>) {
    #[allow(deprecated)]
    oneshot(name, source)
}
