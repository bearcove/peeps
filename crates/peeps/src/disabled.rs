use compact_str::CompactString;
use peeps_types::{
    BroadcastChannelDetails, BufferState, ChannelDetails, ChannelEndpointEntity,
    ChannelEndpointLifecycle, CutAck, CutId, Edge, EdgeKind, Entity, EntityBody, EntityId, Event,
    OneshotChannelDetails, OneshotState, PullChangesResponse, RequestEntity, ResponseEntity,
    ResponseStatus, Scope, ScopeBody, SeqNo, StreamCursor, StreamId, WatchChannelDetails,
};
use std::ffi::OsStr;
use std::future::Future;
use std::io;
use std::process::{ExitStatus, Output, Stdio};
use std::sync::Once;
use tokio::sync::{broadcast, mpsc, oneshot, watch};

#[derive(Clone, Debug, Default)]
pub struct EntityRef;

#[derive(Clone, Debug, Default)]
pub struct EntityHandle;

#[derive(Clone, Debug, Default)]
pub struct ScopeRef;

#[derive(Clone, Debug, Default)]
pub struct ScopeHandle;

impl EntityHandle {
    pub fn new(_name: impl Into<CompactString>, _body: EntityBody) -> Self {
        Self
    }

    pub fn entity_ref(&self) -> EntityRef {
        EntityRef
    }

    pub fn link_to(&self, _target: &EntityRef, _kind: EdgeKind) {}

    pub fn link_to_handle(&self, _target: &EntityHandle, _kind: EdgeKind) {}
}

#[derive(Clone, Debug)]
pub struct RpcRequestHandle {
    handle: EntityHandle,
    id: EntityId,
}

impl RpcRequestHandle {
    pub fn id(&self) -> &EntityId {
        &self.id
    }

    pub fn id_for_wire(&self) -> CompactString {
        CompactString::from(self.id.as_str())
    }

    pub fn entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }

    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }
}

#[derive(Clone, Debug)]
pub struct RpcResponseHandle {
    handle: EntityHandle,
    id: EntityId,
}

impl RpcResponseHandle {
    pub fn id(&self) -> &EntityId {
        &self.id
    }

    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub fn set_status(&self, _status: ResponseStatus) {}

    pub fn mark_ok(&self) {}

    pub fn mark_error(&self) {}

    pub fn mark_cancelled(&self) {}
}

impl ScopeHandle {
    pub fn new(_name: impl Into<CompactString>, _body: ScopeBody) -> Self {
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

pub struct Command(tokio::process::Command);

#[derive(Clone, Debug)]
pub struct CommandDiagnostics {
    pub program: CompactString,
    pub args: Vec<CompactString>,
    pub env: Vec<CompactString>,
}

pub struct Child(tokio::process::Child);

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
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
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
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }
}

impl<T> UnboundedSender<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
        self.inner.send(value)
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl<T> UnboundedReceiver<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }
}

impl<T> OneshotSender<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub fn send(mut self, value: T) -> Result<(), T> {
        let Some(inner) = self.inner.take() else {
            return Err(value);
        };
        inner.send(value)
    }
}

impl<T> OneshotReceiver<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv(mut self) -> Result<T, oneshot::error::RecvError> {
        self.inner.take().expect("oneshot receiver consumed").await
    }
}

impl<T: Clone> BroadcastSender<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        BroadcastReceiver {
            inner: self.inner.subscribe(),
            handle: self.receiver_handle.clone(),
        }
    }

    pub fn send(&self, value: T) -> Result<usize, broadcast::error::SendError<T>> {
        self.inner.send(value)
    }
}

impl<T: Clone> BroadcastReceiver<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv(&mut self) -> Result<T, broadcast::error::RecvError> {
        self.inner.recv().await
    }
}

impl<T: Clone> WatchSender<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub fn send(&self, value: T) -> Result<(), watch::error::SendError<T>> {
        self.inner.send(value)
    }

    pub fn send_replace(&self, value: T) -> T {
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
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
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

pub fn channel<T>(_name: impl Into<String>, capacity: usize) -> (Sender<T>, Receiver<T>) {
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

pub fn oneshot<T>(name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let name: CompactString = name.into().into();
    let (tx, rx) = oneshot::channel();
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details: ChannelDetails::Oneshot(OneshotChannelDetails {
                state: OneshotState::Pending,
            }),
        }),
    );
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details: ChannelDetails::Oneshot(OneshotChannelDetails {
                state: OneshotState::Pending,
            }),
        }),
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    (
        OneshotSender {
            inner: Some(tx),
            handle: tx_handle,
        },
        OneshotReceiver {
            inner: Some(rx),
            handle: rx_handle,
        },
    )
}

pub fn oneshot_channel<T>(name: impl Into<String>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    oneshot(name)
}

pub fn broadcast<T: Clone>(
    name: impl Into<CompactString>,
    capacity: usize,
) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
    let name = name.into();
    let (tx, rx) = broadcast::channel(capacity);
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details: ChannelDetails::Broadcast(BroadcastChannelDetails {
                buffer: Some(BufferState {
                    occupancy: 0,
                    capacity: Some(capacity.min(u32::MAX as usize) as u32),
                }),
            }),
        }),
    );
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details: ChannelDetails::Broadcast(BroadcastChannelDetails {
                buffer: Some(BufferState {
                    occupancy: 0,
                    capacity: Some(capacity.min(u32::MAX as usize) as u32),
                }),
            }),
        }),
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    (
        BroadcastSender {
            inner: tx,
            handle: tx_handle,
            receiver_handle: rx_handle.clone(),
        },
        BroadcastReceiver {
            inner: rx,
            handle: rx_handle,
        },
    )
}

pub fn watch<T: Clone>(
    name: impl Into<CompactString>,
    initial: T,
) -> (WatchSender<T>, WatchReceiver<T>) {
    let name = name.into();
    let (tx, rx) = watch::channel(initial);
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details: ChannelDetails::Watch(WatchChannelDetails {
                last_update_at: None,
            }),
        }),
    );
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details: ChannelDetails::Watch(WatchChannelDetails {
                last_update_at: None,
            }),
        }),
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    (
        WatchSender {
            inner: tx,
            handle: tx_handle,
            receiver_handle: rx_handle.clone(),
        },
        WatchReceiver {
            inner: rx,
            handle: rx_handle,
        },
    )
}

pub fn watch_channel<T: Clone>(
    name: impl Into<CompactString>,
    initial: T,
) -> (WatchSender<T>, WatchReceiver<T>) {
    watch(name, initial)
}

impl Notify {
    pub fn new(_name: impl Into<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub async fn notified(&self) {
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
    pub fn new(_name: impl Into<String>) -> Self {
        Self(tokio::sync::OnceCell::new())
    }

    pub fn get(&self) -> Option<&T> {
        self.0.get()
    }

    pub fn initialized(&self) -> bool {
        self.0.initialized()
    }

    pub async fn get_or_init<F, Fut>(&self, f: F) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.0.get_or_init(f).await
    }

    pub async fn get_or_try_init<F, Fut, E>(&self, f: F) -> Result<&T, E>
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
    pub fn new(_name: impl Into<String>, permits: usize) -> Self {
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

    pub async fn acquire(
        &self,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.0.acquire().await
    }

    pub async fn acquire_many(
        &self,
        n: u32,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.0.acquire_many(n).await
    }

    pub async fn acquire_owned(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
        self.0.clone().acquire_owned().await
    }

    pub async fn acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::AcquireError> {
        self.0.clone().acquire_many_owned(n).await
    }

    pub fn try_acquire(
        &self,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        self.0.try_acquire()
    }

    pub fn try_acquire_many(
        &self,
        n: u32,
    ) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        self.0.try_acquire_many(n)
    }

    pub fn try_acquire_owned(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        self.0.clone().try_acquire_owned()
    }

    pub fn try_acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        self.0.clone().try_acquire_many_owned(n)
    }
}

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

    pub fn spawn(&mut self) -> io::Result<Child> {
        self.0.spawn().map(Child)
    }

    pub async fn status(&mut self) -> io::Result<ExitStatus> {
        self.0.status().await
    }

    pub async fn output(&mut self) -> io::Result<Output> {
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
        let std_cmd = self.0.as_std();
        let program = CompactString::from(std_cmd.get_program().to_string_lossy().as_ref());
        let args = std_cmd
            .get_args()
            .map(|arg| CompactString::from(arg.to_string_lossy().as_ref()))
            .collect::<Vec<_>>();
        let env = std_cmd
            .get_envs()
            .filter_map(|(k, v)| {
                v.map(|v| {
                    CompactString::from(format!("{}={}", k.to_string_lossy(), v.to_string_lossy()))
                })
            })
            .collect::<Vec<_>>();
        (self.0, CommandDiagnostics { program, args, env })
    }
}

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

    pub async fn wait(&mut self) -> io::Result<ExitStatus> {
        self.0.wait().await
    }

    pub async fn wait_with_output(self) -> io::Result<Output> {
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

impl<T> JoinSet<T>
where
    T: Send + 'static,
{
    pub fn named(_name: impl Into<String>) -> Self {
        Self(tokio::task::JoinSet::new())
    }

    pub fn with_name(name: impl Into<String>) -> Self {
        Self::named(name)
    }

    pub fn spawn<F>(&mut self, _label: &'static str, future: F)
    where
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

    pub async fn join_next(&mut self) -> Option<Result<T, tokio::task::JoinError>> {
        self.0.join_next().await
    }
}

pub fn interval(period: std::time::Duration) -> tokio::time::Interval {
    tokio::time::interval(period)
}

pub fn interval_at(
    start: tokio::time::Instant,
    period: std::time::Duration,
) -> tokio::time::Interval {
    tokio::time::interval_at(start, period)
}

static DASHBOARD_DISABLED_WARNING_ONCE: Once = Once::new();

pub fn init(_process_name: &str) {
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

pub fn spawn_tracked<F>(_: impl Into<CompactString>, fut: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(fut)
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
    method: impl Into<CompactString>,
    args_preview: impl Into<CompactString>,
) -> RpcRequestHandle {
    let method = method.into();
    let args_preview = args_preview.into();
    let handle = EntityHandle::new(
        format!("rpc.request.{method}"),
        EntityBody::Request(RequestEntity {
            method: method.clone(),
            args_preview,
        }),
    );
    RpcRequestHandle {
        handle,
        id: EntityId::new(format!("disabled-request-{method}")),
    }
}

pub fn rpc_response(method: impl Into<CompactString>) -> RpcResponseHandle {
    let method = method.into();
    let handle = EntityHandle::new(
        format!("rpc.response.{method}"),
        EntityBody::Response(ResponseEntity {
            method: method.clone(),
            status: ResponseStatus::Pending,
        }),
    );
    RpcResponseHandle {
        handle,
        id: EntityId::new(format!("disabled-response-{method}")),
    }
}

pub fn rpc_response_for(
    method: impl Into<CompactString>,
    _request: &EntityRef,
) -> RpcResponseHandle {
    rpc_response(method)
}

pub trait SnapshotSink {
    fn entity(&mut self, _entity: &Entity) {}
    fn scope(&mut self, _scope: &Scope) {}
    fn edge(&mut self, _edge: &Edge) {}
    fn event(&mut self, _event: &Event) {}
}

pub fn write_snapshot_to<S>(_sink: &mut S)
where
    S: SnapshotSink,
{
}

pub fn pull_changes_since(from_seq_no: SeqNo, _max_changes: u32) -> PullChangesResponse {
    PullChangesResponse {
        stream_id: StreamId(CompactString::from("disabled")),
        from_seq_no,
        next_seq_no: from_seq_no,
        changes: Vec::new(),
        truncated: false,
        compacted_before_seq_no: None,
    }
}

pub fn current_cursor() -> StreamCursor {
    StreamCursor {
        stream_id: StreamId(CompactString::from("disabled")),
        next_seq_no: SeqNo::ZERO,
    }
}

pub fn ack_cut(cut_id: impl Into<CompactString>) -> CutAck {
    CutAck {
        cut_id: CutId(cut_id.into()),
        cursor: current_cursor(),
    }
}

pub fn instrument_future_named<F>(_name: impl Into<CompactString>, fut: F) -> F
where
    F: core::future::Future,
{
    fut
}

pub fn instrument_future_on<F>(_name: impl Into<CompactString>, _on: &EntityHandle, fut: F) -> F
where
    F: core::future::Future,
{
    fut
}

#[macro_export]
macro_rules! peeps {
    (name = $name:expr, fut = $fut:expr $(,)?) => {{
        $crate::instrument_future_named($name, $fut)
    }};
    (name = $name:expr, on = $on:expr, fut = $fut:expr $(,)?) => {{
        $crate::instrument_future_on($name, &$on, $fut)
    }};
}

#[macro_export]
macro_rules! peep {
    ($fut:expr, $name:expr $(,)?) => {{
        $crate::instrument_future_named($name, $fut)
    }};
    ($fut:expr, $name:expr, $meta:tt $(,)?) => {{
        let _ = &$meta;
        $crate::instrument_future_named($name, $fut)
    }};
}
