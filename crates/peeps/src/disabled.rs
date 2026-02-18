use compact_str::CompactString;
use peeps_types::{
    set_inference_source_root, EdgeKind, EntityBody, EntityId, ResponseStatus, ScopeBody,
};
use std::ffi::OsStr;
use std::future::{Future, IntoFuture};
use std::io;
use std::ops::{Deref, DerefMut};
#[cfg(not(target_arch = "wasm32"))]
use std::process::{ExitStatus, Output, Stdio};
use std::sync::{LazyLock, Once};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

#[derive(Clone, Debug, Default)]
pub struct EntityRef;

#[derive(Clone, Debug, Default)]
pub struct EntityHandle;

#[derive(Clone, Debug, Default)]
pub struct ScopeRef;

#[derive(Clone, Debug, Default)]
pub struct ScopeHandle;

#[derive(Clone, Copy, Debug, Default)]
pub struct Source;

impl Source {
    #[track_caller]
    pub const fn caller() -> Self {
        Self
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PeepsContext {
    manifest_dir: &'static str,
}

impl PeepsContext {
    pub const fn new(manifest_dir: &'static str) -> Self {
        Self { manifest_dir }
    }

    #[track_caller]
    pub const fn caller(manifest_dir: &'static str) -> Self {
        Self::new(manifest_dir)
    }

    pub const fn manifest_dir(self) -> &'static str {
        self.manifest_dir
    }
}

pub struct Mutex<T> {
    inner: parking_lot::Mutex<T>,
}

pub struct MutexGuard<'a, T> {
    inner: parking_lot::MutexGuard<'a, T>,
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> Mutex<T> {
    pub fn new(_name: &'static str, value: T, _source: Source) -> Self {
        Self {
            inner: parking_lot::Mutex::new(value),
        }
    }

    #[track_caller]
    pub fn lock_with_cx(&self, cx: PeepsContext) -> MutexGuard<'_, T> {
        self.lock_with_source(Source::caller(), cx)
    }

    pub fn lock_with_source(&self, _source: Source, _cx: PeepsContext) -> MutexGuard<'_, T> {
        MutexGuard {
            inner: self.inner.lock(),
        }
    }

    #[track_caller]
    pub fn try_lock_with_cx(&self, cx: PeepsContext) -> Option<MutexGuard<'_, T>> {
        self.try_lock_with_source(Source::caller(), cx)
    }

    pub fn try_lock_with_source(
        &self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Option<MutexGuard<'_, T>> {
        self.inner.try_lock().map(|inner| MutexGuard { inner })
    }
}

pub struct RwLock<T> {
    inner: parking_lot::RwLock<T>,
}

impl<T> RwLock<T> {
    pub fn new(_name: &'static str, value: T, _source: Source) -> Self {
        Self {
            inner: parking_lot::RwLock::new(value),
        }
    }

    #[track_caller]
    pub fn read_with_cx(&self, cx: PeepsContext) -> parking_lot::RwLockReadGuard<'_, T> {
        self.read_with_source(Source::caller(), cx)
    }

    pub fn read_with_source(
        &self,
        _source: Source,
        _cx: PeepsContext,
    ) -> parking_lot::RwLockReadGuard<'_, T> {
        self.inner.read()
    }

    #[track_caller]
    pub fn write_with_cx(&self, cx: PeepsContext) -> parking_lot::RwLockWriteGuard<'_, T> {
        self.write_with_source(Source::caller(), cx)
    }

    pub fn write_with_source(
        &self,
        _source: Source,
        _cx: PeepsContext,
    ) -> parking_lot::RwLockWriteGuard<'_, T> {
        self.inner.write()
    }

    #[track_caller]
    pub fn try_read_with_cx(
        &self,
        cx: PeepsContext,
    ) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        self.try_read_with_source(Source::caller(), cx)
    }

    pub fn try_read_with_source(
        &self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        self.inner.try_read()
    }

    #[track_caller]
    pub fn try_write_with_cx(
        &self,
        cx: PeepsContext,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        self.try_write_with_source(Source::caller(), cx)
    }

    pub fn try_write_with_source(
        &self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        self.inner.try_write()
    }
}

impl EntityHandle {
    pub fn new(_name: impl Into<CompactString>, _body: EntityBody, _source: Source) -> Self {
        Self
    }

    pub fn entity_ref(&self) -> EntityRef {
        EntityRef
    }

    pub fn link_to(&self, _target: &EntityRef, _kind: EdgeKind) {}

    pub fn link_to_handle(&self, _target: &EntityHandle, _kind: EdgeKind) {}

    pub fn link_to_scope(&self, _scope: &ScopeRef) {}

    pub fn link_to_scope_handle(&self, _scope: &ScopeHandle) {}

    pub fn unlink_from_scope(&self, _scope: &ScopeRef) {}

    pub fn unlink_from_scope_handle(&self, _scope: &ScopeHandle) {}
}

/// A type that can be used as the `on =` argument of the `peeps!()` macro.
pub trait AsEntityRef {
    fn as_entity_ref(&self) -> EntityRef;
}

impl AsEntityRef for EntityHandle {
    fn as_entity_ref(&self) -> EntityRef {
        self.entity_ref()
    }
}

impl AsEntityRef for EntityRef {
    fn as_entity_ref(&self) -> EntityRef {
        self.clone()
    }
}

impl<T> AsEntityRef for Sender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for Receiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for UnboundedSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for UnboundedReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for OneshotSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for OneshotReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T: Clone> AsEntityRef for BroadcastSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T: Clone> AsEntityRef for BroadcastReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
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

#[derive(Clone, Debug, Default)]
pub struct RpcRequestHandle;

static DISABLED_ENTITY_HANDLE: EntityHandle = EntityHandle;
static DISABLED_RPC_REQUEST_ID: LazyLock<EntityId> =
    LazyLock::new(|| EntityId::new("disabled-request"));
static DISABLED_RPC_RESPONSE_ID: LazyLock<EntityId> =
    LazyLock::new(|| EntityId::new("disabled-response"));
static DISABLED_RPC_REQUEST_WIRE_ID: CompactString = CompactString::const_new("disabled-request");

impl RpcRequestHandle {
    pub fn id(&self) -> &EntityId {
        LazyLock::force(&DISABLED_RPC_REQUEST_ID)
    }

    pub fn id_for_wire(&self) -> CompactString {
        DISABLED_RPC_REQUEST_WIRE_ID.clone()
    }

    pub fn entity_ref(&self) -> EntityRef {
        EntityRef
    }

    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &DISABLED_ENTITY_HANDLE
    }
}

#[derive(Clone, Debug, Default)]
pub struct RpcResponseHandle;

impl RpcResponseHandle {
    pub fn id(&self) -> &EntityId {
        LazyLock::force(&DISABLED_RPC_RESPONSE_ID)
    }

    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &DISABLED_ENTITY_HANDLE
    }

    pub fn set_status(&self, _status: ResponseStatus) {}

    pub fn mark_ok(&self) {}

    pub fn mark_error(&self) {}

    pub fn mark_cancelled(&self) {}
}

impl ScopeHandle {
    pub fn new(_name: impl Into<CompactString>, _body: ScopeBody, _source: Source) -> Self {
        Self
    }

    pub fn scope_ref(&self) -> ScopeRef {
        ScopeRef
    }
}

pub struct Sender<T> {
    inner: mpsc::Sender<T>,
    handle: EntityHandle,
}

pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
    handle: EntityHandle,
}

pub struct UnboundedSender<T> {
    inner: mpsc::UnboundedSender<T>,
    handle: EntityHandle,
}

pub struct UnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
    handle: EntityHandle,
}

pub struct OneshotSender<T> {
    inner: Option<oneshot::Sender<T>>,
    handle: EntityHandle,
}

pub struct OneshotReceiver<T> {
    inner: Option<oneshot::Receiver<T>>,
    handle: EntityHandle,
}

pub struct BroadcastSender<T> {
    inner: broadcast::Sender<T>,
    handle: EntityHandle,
    receiver_handle: EntityHandle,
}

pub struct BroadcastReceiver<T> {
    inner: broadcast::Receiver<T>,
    handle: EntityHandle,
}

pub struct WatchSender<T> {
    inner: watch::Sender<T>,
    handle: EntityHandle,
    receiver_handle: EntityHandle,
}

pub struct WatchReceiver<T> {
    inner: watch::Receiver<T>,
    handle: EntityHandle,
}

#[derive(Clone)]
pub struct Notify {
    inner: std::sync::Arc<tokio::sync::Notify>,
}

pub struct OnceCell<T>(tokio::sync::OnceCell<T>);

#[derive(Clone)]
pub struct Semaphore(std::sync::Arc<tokio::sync::Semaphore>);

pub struct SemaphorePermit<'a>(tokio::sync::SemaphorePermit<'a>);

pub struct OwnedSemaphorePermit(tokio::sync::OwnedSemaphorePermit);

#[cfg(not(target_arch = "wasm32"))]
pub struct Command(tokio::process::Command);
#[cfg(target_arch = "wasm32")]
pub struct Command;

#[derive(Clone, Debug)]
pub struct CommandDiagnostics {
    pub program: CompactString,
    pub args: Vec<CompactString>,
    pub env: Vec<CompactString>,
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Child(tokio::process::Child);
#[cfg(target_arch = "wasm32")]
pub struct Child;

pub struct JoinSet<T>(tokio::task::JoinSet<T>);

pub type Interval = tokio::time::Interval;

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

impl<T> Clone for BroadcastSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            receiver_handle: self.receiver_handle.clone(),
        }
    }
}

impl<T> Clone for WatchSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            receiver_handle: self.receiver_handle.clone(),
        }
    }
}

impl<T> Clone for WatchReceiver<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
        }
    }
}

impl<T> Sender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn send_with_cx(
        &self,
        value: T,
        cx: PeepsContext,
    ) -> Result<(), mpsc::error::SendError<T>> {
        self.send_with_source(value, Source::caller(), cx).await
    }

    pub async fn send_with_source(
        &self,
        value: T,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<(), mpsc::error::SendError<T>> {
        self.inner.send(value).await
    }

    pub fn try_send(&self, value: T) -> Result<(), mpsc::error::TrySendError<T>> {
        self.inner.try_send(value)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl<T> Receiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv_with_cx(&mut self, cx: PeepsContext) -> Option<T> {
        self.recv_with_source(Source::caller(), cx).await
    }

    pub async fn recv_with_source(&mut self, _source: Source, _cx: PeepsContext) -> Option<T> {
        self.inner.recv().await
    }
}

impl<T> UnboundedSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn send_with_cx(
        &self,
        value: T,
        cx: PeepsContext,
    ) -> Result<(), mpsc::error::SendError<T>> {
        self.send_with_source(value, Source::caller(), cx)
    }

    pub fn send_with_source(
        &self,
        value: T,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<(), mpsc::error::SendError<T>> {
        self.inner.send(value)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl<T> UnboundedReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv_with_cx(&mut self, cx: PeepsContext) -> Option<T> {
        self.recv_with_source(Source::caller(), cx).await
    }

    pub async fn recv_with_source(&mut self, _source: Source, _cx: PeepsContext) -> Option<T> {
        self.inner.recv().await
    }
}

impl<T> OneshotSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn send_with_cx(self, value: T, cx: PeepsContext) -> Result<(), T> {
        self.send_with_source(value, Source::caller(), cx)
    }

    pub fn send_with_source(
        mut self,
        value: T,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<(), T> {
        let Some(inner) = self.inner.take() else {
            return Err(value);
        };
        inner.send(value)
    }
}

impl<T> OneshotReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv_with_cx(self, cx: PeepsContext) -> Result<T, oneshot::error::RecvError> {
        self.recv_with_source(Source::caller(), cx).await
    }

    pub async fn recv_with_source(
        mut self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<T, oneshot::error::RecvError> {
        self.inner.take().expect("oneshot receiver consumed").await
    }
}

impl<T: Clone> BroadcastSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        BroadcastReceiver {
            inner: self.inner.subscribe(),
            handle: self.receiver_handle.clone(),
        }
    }

    #[track_caller]
    pub fn send_with_cx(
        &self,
        value: T,
        cx: PeepsContext,
    ) -> Result<usize, broadcast::error::SendError<T>> {
        self.send_with_source(value, Source::caller(), cx)
    }

    pub fn send_with_source(
        &self,
        value: T,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<usize, broadcast::error::SendError<T>> {
        self.inner.send(value)
    }
}

impl<T: Clone> BroadcastReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv_with_cx(
        &mut self,
        cx: PeepsContext,
    ) -> Result<T, broadcast::error::RecvError> {
        self.recv_with_source(Source::caller(), cx).await
    }

    pub async fn recv_with_source(
        &mut self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<T, broadcast::error::RecvError> {
        self.inner.recv().await
    }
}

impl<T: Clone> WatchSender<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn send_with_cx(
        &self,
        value: T,
        cx: PeepsContext,
    ) -> Result<(), watch::error::SendError<T>> {
        self.send_with_source(value, Source::caller(), cx)
    }

    pub fn send_with_source(
        &self,
        value: T,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<(), watch::error::SendError<T>> {
        self.inner.send(value)
    }

    #[track_caller]
    pub fn send_replace_with_cx(&self, value: T, cx: PeepsContext) -> T {
        self.send_replace_with_source(value, Source::caller(), cx)
    }

    pub fn send_replace_with_source(&self, value: T, _source: Source, _cx: PeepsContext) -> T {
        self.inner.send_replace(value)
    }

    pub fn subscribe(&self) -> WatchReceiver<T> {
        WatchReceiver {
            inner: self.inner.subscribe(),
            handle: self.receiver_handle.clone(),
        }
    }
}

impl<T: Clone> WatchReceiver<T> {
    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn changed_with_cx(
        &mut self,
        cx: PeepsContext,
    ) -> Result<(), watch::error::RecvError> {
        self.changed_with_source(Source::caller(), cx).await
    }

    pub async fn changed_with_source(
        &mut self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<(), watch::error::RecvError> {
        self.inner.changed().await
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

pub fn channel<T>(_name: impl Into<String>, capacity: usize, _source: Source) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel(capacity);
    (
        Sender {
            inner: tx,
            handle: EntityHandle,
        },
        Receiver {
            inner: rx,
            handle: EntityHandle,
        },
    )
}

pub fn unbounded_channel<T>(
    _name: impl Into<String>,
    _source: Source,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        UnboundedSender {
            inner: tx,
            handle: EntityHandle,
        },
        UnboundedReceiver {
            inner: rx,
            handle: EntityHandle,
        },
    )
}

pub fn oneshot<T>(name: impl Into<String>, _source: Source) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let _ = name;
    let (tx, rx) = oneshot::channel();
    (
        OneshotSender {
            inner: Some(tx),
            handle: EntityHandle,
        },
        OneshotReceiver {
            inner: Some(rx),
            handle: EntityHandle,
        },
    )
}

pub fn oneshot_channel<T>(name: impl Into<String>, source: Source) -> (OneshotSender<T>, OneshotReceiver<T>) {
    oneshot(name, source)
}

pub fn broadcast<T: Clone>(
    name: impl Into<CompactString>,
    capacity: usize,
    _source: Source,
) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
    let _ = name;
    let (tx, rx) = broadcast::channel(capacity);
    (
        BroadcastSender {
            inner: tx,
            handle: EntityHandle,
            receiver_handle: EntityHandle,
        },
        BroadcastReceiver {
            inner: rx,
            handle: EntityHandle,
        },
    )
}

pub fn watch<T: Clone>(
    name: impl Into<CompactString>,
    initial: T,
    _source: Source,
) -> (WatchSender<T>, WatchReceiver<T>) {
    let _ = name;
    let (tx, rx) = watch::channel(initial);
    (
        WatchSender {
            inner: tx,
            handle: EntityHandle,
            receiver_handle: EntityHandle,
        },
        WatchReceiver {
            inner: rx,
            handle: EntityHandle,
        },
    )
}

pub fn watch_channel<T: Clone>(
    name: impl Into<CompactString>,
    initial: T,
    source: Source,
) -> (WatchSender<T>, WatchReceiver<T>) {
    watch(name, initial, source)
}

impl Notify {
    pub fn new(_name: impl Into<String>, _source: Source) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub async fn notified_with_cx(&self, cx: PeepsContext) {
        self.notified_with_source(Source::caller(), cx).await;
    }

    pub async fn notified_with_source(&self, _source: Source, _cx: PeepsContext) {
        self.inner.notified().await;
    }

    pub fn notify_one(&self) {
        self.inner.notify_one();
    }

    pub fn notify_waiters(&self) {
        self.inner.notify_waiters();
    }
}

impl<T> OnceCell<T> {
    pub fn new(_name: impl Into<String>, _source: Source) -> Self {
        Self(tokio::sync::OnceCell::new())
    }

    pub fn get(&self) -> Option<&T> {
        self.0.get()
    }

    pub fn initialized(&self) -> bool {
        self.0.initialized()
    }

    pub async fn get_or_init_with_cx<F, Fut>(&self, f: F, cx: PeepsContext) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.get_or_init_with_source(f, Source::caller(), cx).await
    }

    pub async fn get_or_init_with_source<F, Fut>(
        &self,
        f: F,
        _source: Source,
        _cx: PeepsContext,
    ) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.0.get_or_init(f).await
    }

    pub async fn get_or_try_init_with_cx<F, Fut, E>(
        &self,
        f: F,
        cx: PeepsContext,
    ) -> Result<&T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        self.get_or_try_init_with_source(f, Source::caller(), cx).await
    }

    pub async fn get_or_try_init_with_source<F, Fut, E>(
        &self,
        f: F,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<&T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        self.0.get_or_try_init(f).await
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        self.0.set(value).map_err(|e| match e {
            tokio::sync::SetError::AlreadyInitializedError(v) => v,
            tokio::sync::SetError::InitializingError(v) => v,
        })
    }
}

impl Semaphore {
    pub fn new(_name: impl Into<String>, permits: usize, _source: Source) -> Self {
        Self(std::sync::Arc::new(tokio::sync::Semaphore::new(permits)))
    }

    pub fn available_permits(&self) -> usize {
        self.0.available_permits()
    }

    pub fn close(&self) {
        self.0.close()
    }

    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    pub fn add_permits(&self, n: usize) {
        self.0.add_permits(n)
    }

    pub async fn acquire_with_cx(
        &self,
        cx: PeepsContext,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.acquire_with_source(Source::caller(), cx).await
    }

    pub async fn acquire_with_source(
        &self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.0.acquire().await.map(SemaphorePermit)
    }

    pub async fn acquire_many_with_cx(
        &self,
        n: u32,
        cx: PeepsContext,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.acquire_many_with_source(n, Source::caller(), cx).await
    }

    pub async fn acquire_many_with_source(
        &self,
        n: u32,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.0.acquire_many(n).await.map(SemaphorePermit)
    }

    pub async fn acquire_owned_with_cx(
        &self,
        cx: PeepsContext,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        self.acquire_owned_with_source(Source::caller(), cx).await
    }

    pub async fn acquire_owned_with_source(
        &self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        self.0.clone().acquire_owned().await.map(OwnedSemaphorePermit)
    }

    pub async fn acquire_many_owned_with_cx(
        &self,
        n: u32,
        cx: PeepsContext,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        self.acquire_many_owned_with_source(n, Source::caller(), cx)
            .await
    }

    pub async fn acquire_many_owned_with_source(
        &self,
        n: u32,
        _source: Source,
        _cx: PeepsContext,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::AcquireError> {
        self.0.clone().acquire_many_owned(n).await.map(OwnedSemaphorePermit)
    }

    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        self.0.try_acquire().map(SemaphorePermit)
    }

    pub fn try_acquire_many(
        &self,
        n: u32,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        self.0.try_acquire_many(n).map(SemaphorePermit)
    }

    pub fn try_acquire_owned(&self) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        self.0.clone().try_acquire_owned().map(OwnedSemaphorePermit)
    }

    pub fn try_acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        self.0
            .clone()
            .try_acquire_many_owned(n)
            .map(OwnedSemaphorePermit)
    }
}

impl<'a> Deref for SemaphorePermit<'a> {
    type Target = tokio::sync::SemaphorePermit<'a>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for SemaphorePermit<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Deref for OwnedSemaphorePermit {
    type Target = tokio::sync::OwnedSemaphorePermit;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OwnedSemaphorePermit {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Command {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self(tokio::process::Command::new(program))
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.0.arg(arg);
        self
    }

    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        self.0.args(args);
        self
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, val: impl AsRef<OsStr>) -> &mut Self {
        self.0.env(key, val);
        self
    }

    pub fn envs(
        &mut self,
        vars: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> &mut Self {
        self.0.envs(vars);
        self
    }

    pub fn env_clear(&mut self) -> &mut Self {
        self.0.env_clear();
        self
    }

    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.0.env_remove(key);
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<std::path::Path>) -> &mut Self {
        self.0.current_dir(dir);
        self
    }

    pub fn stdin(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stdin(cfg);
        self
    }

    pub fn stdout(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stdout(cfg);
        self
    }

    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stderr(cfg);
        self
    }

    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.0.kill_on_drop(kill_on_drop);
        self
    }

    #[track_caller]
    pub fn spawn_with_cx(&mut self, cx: PeepsContext) -> io::Result<Child> {
        self.spawn_with_source(Source::caller(), cx)
    }

    pub fn spawn_with_source(&mut self, _source: Source, _cx: PeepsContext) -> io::Result<Child> {
        self.0.spawn().map(Child)
    }

    pub async fn status_with_cx(&mut self, cx: PeepsContext) -> io::Result<ExitStatus> {
        self.status_with_source(Source::caller(), cx).await
    }

    pub async fn status_with_source(
        &mut self,
        _source: Source,
        _cx: PeepsContext,
    ) -> io::Result<ExitStatus> {
        self.0.status().await
    }

    pub async fn output_with_cx(&mut self, cx: PeepsContext) -> io::Result<Output> {
        self.output_with_source(Source::caller(), cx).await
    }

    pub async fn output_with_source(
        &mut self,
        _source: Source,
        _cx: PeepsContext,
    ) -> io::Result<Output> {
        self.0.output().await
    }

    pub fn as_std(&self) -> &std::process::Command {
        self.0.as_std()
    }

    #[cfg(unix)]
    pub unsafe fn pre_exec<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut() -> io::Result<()> + Send + Sync + 'static,
    {
        self.0.pre_exec(f);
        self
    }

    pub fn into_inner(self) -> tokio::process::Command {
        self.0
    }

    pub fn into_inner_with_diagnostics(self) -> (tokio::process::Command, CommandDiagnostics) {
        (
            self.0,
            CommandDiagnostics {
                program: CompactString::default(),
                args: Vec::new(),
                env: Vec::new(),
            },
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Child {
    pub fn from_tokio_with_diagnostics(
        child: tokio::process::Child,
        _diag: CommandDiagnostics,
    ) -> Self {
        Self(child)
    }

    pub fn id(&self) -> Option<u32> {
        self.0.id()
    }

    pub async fn wait_with_cx(&mut self, cx: PeepsContext) -> io::Result<ExitStatus> {
        self.wait_with_source(Source::caller(), cx).await
    }

    pub async fn wait_with_source(
        &mut self,
        _source: Source,
        _cx: PeepsContext,
    ) -> io::Result<ExitStatus> {
        self.0.wait().await
    }

    pub async fn wait_with_output_with_cx(self, cx: PeepsContext) -> io::Result<Output> {
        self.wait_with_output_with_source(Source::caller(), cx).await
    }

    pub async fn wait_with_output_with_source(
        self,
        _source: Source,
        _cx: PeepsContext,
    ) -> io::Result<Output> {
        self.0.wait_with_output().await
    }

    pub fn start_kill(&mut self) -> io::Result<()> {
        self.0.start_kill()
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.start_kill()
    }

    pub fn stdin(&mut self) -> &mut Option<tokio::process::ChildStdin> {
        &mut self.0.stdin
    }

    pub fn stdout(&mut self) -> &mut Option<tokio::process::ChildStdout> {
        &mut self.0.stdout
    }

    pub fn stderr(&mut self) -> &mut Option<tokio::process::ChildStderr> {
        &mut self.0.stderr
    }

    pub fn take_stdin(&mut self) -> Option<tokio::process::ChildStdin> {
        self.0.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.0.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.0.stderr.take()
    }
}

#[cfg(target_arch = "wasm32")]
impl Command {
    pub fn new(_program: impl AsRef<OsStr>) -> Self {
        Self
    }

    pub fn arg(&mut self, _arg: impl AsRef<OsStr>) -> &mut Self {
        self
    }

    pub fn args(&mut self, _args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        self
    }

    pub fn env(&mut self, _key: impl AsRef<OsStr>, _val: impl AsRef<OsStr>) -> &mut Self {
        self
    }

    pub fn envs(
        &mut self,
        _vars: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> &mut Self {
        self
    }

    pub fn env_clear(&mut self) -> &mut Self {
        self
    }

    pub fn env_remove(&mut self, _key: impl AsRef<OsStr>) -> &mut Self {
        self
    }

    pub fn current_dir(&mut self, _dir: impl AsRef<std::path::Path>) -> &mut Self {
        self
    }

    pub fn kill_on_drop(&mut self, _kill_on_drop: bool) -> &mut Self {
        self
    }

    pub fn spawn(&mut self) -> io::Result<Child> {
        Err(io::Error::other("tokio::process is unavailable on wasm32"))
    }
}

#[cfg(target_arch = "wasm32")]
impl Child {
    pub fn id(&self) -> Option<u32> {
        None
    }

    pub async fn wait(&mut self) -> io::Result<()> {
        Err(io::Error::other("tokio::process is unavailable on wasm32"))
    }

    pub async fn wait_with_output(self) -> io::Result<Vec<u8>> {
        Err(io::Error::other("tokio::process is unavailable on wasm32"))
    }

    pub fn start_kill(&mut self) -> io::Result<()> {
        Err(io::Error::other("tokio::process is unavailable on wasm32"))
    }

    pub fn kill(&mut self) -> io::Result<()> {
        self.start_kill()
    }
}

impl<T> JoinSet<T>
where
    T: Send + 'static,
{
    pub fn named(_name: impl Into<String>, _source: Source) -> Self {
        Self(tokio::task::JoinSet::new())
    }

    pub fn with_name(name: impl Into<String>, source: Source) -> Self {
        Self::named(name, source)
    }

    #[track_caller]
    pub fn spawn_with_cx<F>(&mut self, label: &'static str, future: F, cx: PeepsContext)
    where
        F: Future<Output = T> + Send + 'static,
    {
        self.spawn_with_source(label, future, Source::caller(), cx);
    }

    pub fn spawn_with_source<F>(
        &mut self,
        _label: &'static str,
        future: F,
        _source: Source,
        _cx: PeepsContext,
    ) where
        F: Future<Output = T> + Send + 'static,
    {
        self.0.spawn(future);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn abort_all(&mut self) {
        self.0.abort_all();
    }

    pub async fn join_next_with_cx(
        &mut self,
        cx: PeepsContext,
    ) -> Option<Result<T, tokio::task::JoinError>> {
        self.join_next_with_source(Source::caller(), cx).await
    }

    pub async fn join_next_with_source(
        &mut self,
        _source: Source,
        _cx: PeepsContext,
    ) -> Option<Result<T, tokio::task::JoinError>> {
        self.0.join_next().await
    }
}

pub fn interval(period: std::time::Duration, _source: Source) -> tokio::time::Interval {
    tokio::time::interval(period)
}

pub fn interval_at(
    start: tokio::time::Instant,
    period: std::time::Duration,
    _source: Source,
) -> tokio::time::Interval {
    tokio::time::interval_at(start, period)
}

static DASHBOARD_DISABLED_WARNING_ONCE: Once = Once::new();

#[doc(hidden)]
pub fn __init_from_macro(manifest_dir: &str) {
    set_inference_source_root(std::path::PathBuf::from(manifest_dir));
    maybe_warn_dashboard_ignored();
}

fn maybe_warn_dashboard_ignored() {
    let Some(value) = std::env::var_os("PEEPS_DASHBOARD") else {
        return;
    };
    if value.to_string_lossy().trim().is_empty() {
        return;
    }

    DASHBOARD_DISABLED_WARNING_ONCE.call_once(|| {
        eprintln!(
            "\n\x1b[1;31m\
======================================================================\n\
 PEEPS WARNING: PEEPS_DASHBOARD is set, but peeps diagnostics is disabled.\n\
 This process will NOT connect to peeps-web in this build.\n\
 Enable the `diagnostics` cargo feature of `peeps` to use dashboard push.\n\
======================================================================\x1b[0m\n"
        );
    });
}

pub fn spawn_tracked<F>(
    _: impl Into<CompactString>,
    fut: F,
    _source: Source,
) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(fut)
}

pub fn spawn_blocking_tracked<F, T>(
    _: impl Into<CompactString>,
    f: F,
    _source: Source,
) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
}

pub fn sleep(duration: std::time::Duration, _label: impl Into<String>) -> impl Future<Output = ()> {
    tokio::time::sleep(duration)
}

pub async fn timeout<F>(
    duration: std::time::Duration,
    future: F,
    _label: impl Into<String>,
) -> Result<F::Output, tokio::time::error::Elapsed>
where
    F: Future,
{
    tokio::time::timeout(duration, future).await
}

pub fn entity_ref_from_wire(_id: impl Into<CompactString>) -> EntityRef {
    EntityRef
}

pub fn rpc_request(
    _method: impl Into<CompactString>,
    _args_preview: impl Into<CompactString>,
    _source: Source,
) -> RpcRequestHandle {
    RpcRequestHandle
}

pub fn rpc_response(_method: impl Into<CompactString>, _source: Source) -> RpcResponseHandle {
    RpcResponseHandle
}

pub fn rpc_response_for(
    method: impl Into<CompactString>,
    _request: &EntityRef,
    source: Source,
) -> RpcResponseHandle {
    rpc_response(method, source)
}

pub fn instrument_future_named<F>(
    _name: impl Into<CompactString>,
    fut: F,
    _source: Source,
) -> F::IntoFuture
where
    F: IntoFuture,
{
    instrument_future_named_with_source(_name, fut, _source)
}

pub fn instrument_future_named_with_source<F>(
    _name: impl Into<CompactString>,
    fut: F,
    _source: Source,
) -> F::IntoFuture
where
    F: IntoFuture,
{
    fut.into_future()
}

pub fn instrument_future_on<F>(
    _name: impl Into<CompactString>,
    _on: &impl AsEntityRef,
    fut: F,
    _source: Source,
) -> F::IntoFuture
where
    F: IntoFuture,
{
    instrument_future_on_with_source(_name, _on, fut, _source)
}

pub fn instrument_future_on_with_source<F>(
    _name: impl Into<CompactString>,
    _on: &impl AsEntityRef,
    fut: F,
    _source: Source,
) -> F::IntoFuture
where
    F: IntoFuture,
{
    fut.into_future()
}

#[macro_export]
macro_rules! peeps {
    (name = $name:expr, fut = $fut:expr $(,)?) => {{
        $crate::instrument_future_named($name, $fut, $crate::Source::caller())
    }};
    (name = $name:expr, on = $on:expr, fut = $fut:expr $(,)?) => {{
        $crate::instrument_future_on($name, &$on, $fut, $crate::Source::caller())
    }};
}

#[macro_export]
macro_rules! peep {
    ($fut:expr, $name:expr $(,)?) => {{
        $crate::instrument_future_named($name, $fut, $crate::Source::caller())
    }};
    ($fut:expr, $name:expr, {$($k:literal => $v:expr),* $(,)?} $(,)?) => {{
        let _ = ($((&$k, &$v)),*);
        $crate::instrument_future_named($name, $fut, $crate::Source::caller())
    }};
    ($fut:expr, $name:expr, level = $($rest:tt)*) => {{
        compile_error!("`level=` is deprecated");
    }};
    ($fut:expr, $name:expr, kind = $($rest:tt)*) => {{
        compile_error!("`kind=` is deprecated");
    }};
    ($fut:expr, $name:expr, $($rest:tt)+) => {{
        compile_error!("invalid `peep!` arguments");
    }};
}

#[macro_export]
macro_rules! mutex {
    ($name:expr, $value:expr $(,)?) => {{
        $crate::Mutex::new($name, $value, $crate::Source::caller())
    }};
}

#[macro_export]
macro_rules! rwlock {
    ($name:expr, $value:expr $(,)?) => {{
        $crate::RwLock::new($name, $value, $crate::Source::caller())
    }};
}

#[macro_export]
macro_rules! channel {
    ($name:expr, $capacity:expr $(,)?) => {
        $crate::channel($name, $capacity, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! unbounded_channel {
    ($name:expr $(,)?) => {
        $crate::unbounded_channel($name, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! oneshot {
    ($name:expr $(,)?) => {
        $crate::oneshot($name, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! broadcast {
    ($name:expr, $capacity:expr $(,)?) => {
        $crate::broadcast($name, $capacity, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! watch {
    ($name:expr, $initial:expr $(,)?) => {
        $crate::watch($name, $initial, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! notify {
    ($name:expr $(,)?) => {
        $crate::Notify::new($name, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! once_cell {
    ($name:expr $(,)?) => {
        $crate::OnceCell::new($name, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! semaphore {
    ($name:expr, $permits:expr $(,)?) => {
        $crate::Semaphore::new($name, $permits, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! join_set {
    ($name:expr $(,)?) => {
        $crate::JoinSet::named($name, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! spawn_tracked {
    ($name:expr, $fut:expr $(,)?) => {
        $crate::spawn_tracked($name, $fut, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! spawn_blocking_tracked {
    ($name:expr, $f:expr $(,)?) => {
        $crate::spawn_blocking_tracked($name, $f, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! sleep {
    ($duration:expr, $label:expr $(,)?) => {
        $crate::sleep($duration, $label)
    };
}

#[macro_export]
macro_rules! timeout {
    ($duration:expr, $future:expr, $label:expr $(,)?) => {
        $crate::timeout($duration, $future, $label)
    };
}

#[macro_export]
macro_rules! rpc_request {
    ($method:expr, $args_preview:expr $(,)?) => {
        $crate::rpc_request($method, $args_preview, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! rpc_response {
    ($method:expr $(,)?) => {
        $crate::rpc_response($method, $crate::Source::caller())
    };
}

#[macro_export]
macro_rules! rpc_response_for {
    ($method:expr, $request:expr $(,)?) => {
        $crate::rpc_response_for($method, $request, $crate::Source::caller())
    };
}
