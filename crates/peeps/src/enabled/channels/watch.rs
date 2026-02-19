use super::{Source, SourceRight};

use peeps_runtime::{
    instrument_operation_on_with_source, record_event_with_source, AsEntityRef, EntityHandle,
    EntityRef, WeakEntityHandle,
};
use peeps_types::{
    EdgeKind, EntityBody, Event, EventKind, EventTarget, WatchRxEntity, WatchTxEntity,
};
use tokio::sync::watch;

pub struct WatchSender<T> {
    inner: tokio::sync::watch::Sender<T>,
    handle: EntityHandle<peeps_types::WatchTx>,
}

pub struct WatchReceiver<T> {
    inner: tokio::sync::watch::Receiver<T>,
    handle: EntityHandle<peeps_types::WatchRx>,
    tx_handle: WeakEntityHandle<peeps_types::WatchTx>,
}

impl<T> Clone for WatchSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
        }
    }
}

impl<T> Clone for WatchReceiver<T> {
    fn clone(&self) -> Self {
        let handle = EntityHandle::new(
            "watch:rx.clone",
            EntityBody::WatchRx(WatchRxEntity {}),
            SourceRight::caller(),
        )
        .into_typed::<peeps_types::WatchRx>();
        Self {
            inner: self.inner.clone(),
            handle,
            tx_handle: self.tx_handle.clone(),
        }
    }
}

impl<T: Clone> WatchSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::WatchTx> {
        &self.handle
    }

    #[doc(hidden)]
    pub fn send_with_source(&self, value: T, source: Source) -> Result<(), watch::error::SendError<T>> {
        let result = self.inner.send(value);
        if result.is_ok() {
            let _ = self
                .handle
                .mutate(|body| body.last_update_at = Some(peeps_types::PTime::now()));
        }
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelSent,
            source.clone(),
        );
        record_event_with_source(event, &source);
        result
    }

    #[doc(hidden)]
    pub fn send_replace_with_source(&self, value: T, source: Source) -> T {
        let old = self.inner.send_replace(value);
        let _ = self
            .handle
            .mutate(|body| body.last_update_at = Some(peeps_types::PTime::now()));
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelSent,
            source.clone(),
        );
        record_event_with_source(event, &source);
        old
    }

    pub fn subscribe(&self) -> WatchReceiver<T> {
        let handle = EntityHandle::new(
            "watch:rx.subscribe",
            EntityBody::WatchRx(WatchRxEntity {}),
            SourceRight::caller(),
        )
        .into_typed::<peeps_types::WatchRx>();
        self.handle.link_to_handle(&handle, EdgeKind::PairedWith);
        WatchReceiver {
            inner: self.inner.subscribe(),
            handle,
            tx_handle: self.handle.downgrade(),
        }
    }
}

impl<T: Clone> WatchReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<peeps_types::WatchRx> {
        &self.handle
    }

    #[doc(hidden)]
    pub async fn changed_with_source(
        &mut self,
        source: Source,
    ) -> Result<(), watch::error::RecvError> {
        let result =
            instrument_operation_on_with_source(&self.handle, self.inner.changed(), &source).await;
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelReceived,
            source.clone(),
        );
        record_event_with_source(event, &source);
        result
    }

    pub fn borrow(&self) -> watch::Ref<'_, T> {
        self.inner.borrow()
    }

    pub fn borrow_and_update(&mut self) -> watch::Ref<'_, T> {
        self.inner.borrow_and_update()
    }

    pub fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.inner.has_changed()
    }
}

pub fn watch<T: Clone>(
    name: impl Into<String>,
    initial: T,
    source: SourceRight,
) -> (WatchSender<T>, WatchReceiver<T>) {
    let name = name.into();
    let (tx, rx) = watch::channel(initial);

    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::WatchTx(WatchTxEntity {
            last_update_at: None,
        }),
        source,
    )
    .into_typed::<peeps_types::WatchTx>();

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::WatchRx(WatchRxEntity {}),
        source,
    )
    .into_typed::<peeps_types::WatchRx>();

    tx_handle.link_to_handle(&rx_handle, EdgeKind::PairedWith);

    (
        WatchSender {
            inner: tx,
            handle: tx_handle.clone(),
        },
        WatchReceiver {
            inner: rx,
            handle: rx_handle,
            tx_handle: tx_handle.downgrade(),
        },
    )
}

pub fn watch_channel<T: Clone>(
    name: impl Into<String>,
    initial: T,
    source: SourceRight,
) -> (WatchSender<T>, WatchReceiver<T>) {
    #[allow(deprecated)]
    watch(name, initial, source)
}

impl<T: Clone> AsEntityRef for WatchSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
