// r[impl api.watch]

use moire_runtime::{
    instrument_operation_on, new_event, record_event, AsEntityRef, EntityHandle,
    EntityRef, WeakEntityHandle,
};
use moire_types::{
    EdgeKind, EventKind, EventTarget, WatchRxEntity, WatchTxEntity,
};
use tokio::sync::watch;

/// Instrumented version of [`tokio::sync::watch::Sender`].
///
/// Records watch state transitions and notifications for diagnostics.
pub struct WatchSender<T> {
    inner: tokio::sync::watch::Sender<T>,
    handle: EntityHandle<moire_types::WatchTx>,
}

/// Instrumented version of [`tokio::sync::watch::Receiver`].
///
/// Tracks observed values and change notifications for diagnostics.
pub struct WatchReceiver<T> {
    inner: tokio::sync::watch::Receiver<T>,
    handle: EntityHandle<moire_types::WatchRx>,
    tx_handle: WeakEntityHandle<moire_types::WatchTx>,
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
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            tx_handle: self.tx_handle.clone(),
        }
    }
}

impl<T: Clone> WatchSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::WatchTx> {
        &self.handle
    }
    /// Sends a new value, matching [`tokio::sync::watch::Sender::send`].
    ///
    /// Updates receiver metadata and records a channel-sent event.
    pub fn send(&self, value: T) -> Result<(), watch::error::SendError<T>> {
                let result = self.inner.send(value);
        if result.is_ok() {
            let _ = self
                .handle
                .mutate(|body| body.last_update_at = Some(moire_types::PTime::now()));
        }
        let event = new_event(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelSent, 
        );
        record_event(event);
        result
    }
    /// Replaces the current value and returns the previous value.
    ///
    /// Mirrors [`tokio::sync::watch::Sender::send_replace`].
    pub fn send_replace(&self, value: T) -> T {
                let old = self.inner.send_replace(value);
        let _ = self
            .handle
            .mutate(|body| body.last_update_at = Some(moire_types::PTime::now()));
        let event = new_event(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelSent, 
        );
        record_event(event);
        old
    }
    /// Subscribes a receiver, equivalent to [`tokio::sync::watch::Sender::subscribe`].
    ///
    /// Returns a linked sender/receiver pair with diagnostic metadata.
    pub fn subscribe(&self) -> WatchReceiver<T> {
        let handle = EntityHandle::new("watch:rx.subscribe", WatchRxEntity {});
        self.handle
            .link_to_handle(&handle, EdgeKind::PairedWith);
        WatchReceiver {
            inner: self.inner.subscribe(),
            handle,
            tx_handle: self.handle.downgrade(),
        }
    }
}

impl<T: Clone> WatchReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::WatchRx> {
        &self.handle
    }
    /// Waits for a value change, matching [`tokio::sync::watch::Receiver::changed`].
    ///
    /// Records notification wait timing for diagnostics.
    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
                let result =
            instrument_operation_on(&self.handle, self.inner.changed()).await;
        let event = new_event(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::ChannelReceived, 
        );
        record_event(event);
        result
    }

    /// Returns a borrowed reference to the current value.
    ///
    /// Equivalent to [`tokio::sync::watch::Receiver::borrow`].
    pub fn borrow(&self) -> watch::Ref<'_, T> {
        self.inner.borrow()
    }

    /// Updates and then borrows the current value.
    ///
    /// Same semantics as [`tokio::sync::watch::Receiver::borrow_and_update`].
    pub fn borrow_and_update(&mut self) -> watch::Ref<'_, T> {
        self.inner.borrow_and_update()
    }

    /// Checks whether the value has changed since the last borrow.
    ///
    /// Mirrors [`tokio::sync::watch::Receiver::has_changed`].
    pub fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.inner.has_changed()
    }
}

/// Creates an instrumented watch channel, equivalent to [`tokio::sync::watch::channel`].
pub fn watch<T: Clone>(name: impl Into<String>, initial: T) -> (WatchSender<T>, WatchReceiver<T>) {
        let name = name.into();
    let (tx, rx) = watch::channel(initial);

    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        WatchTxEntity {
            last_update_at: None,
        },
    );

    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        WatchRxEntity {},
    );

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

impl<T: Clone> AsEntityRef for WatchSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}
