use compact_str::CompactString;
use peeps_types::{
    ChannelDetails, ChannelEndpointEntity, ChannelEndpointLifecycle, CutAck, CutId, Edge, EdgeKind,
    Entity, EntityBody, EntityId, Event, OneshotChannelDetails, OneshotState, PullChangesResponse,
    RequestEntity, ResponseEntity, ResponseStatus, Scope, ScopeBody, SeqNo, StreamCursor, StreamId,
};
use std::future::Future;
use std::sync::Once;
use tokio::sync::{mpsc, oneshot};

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

pub struct OneshotSender<T> {
    inner: Option<oneshot::Sender<T>>,
    handle: EntityHandle,
}

pub struct OneshotReceiver<T> {
    inner: Option<oneshot::Receiver<T>>,
    handle: EntityHandle,
}

impl<T> Clone for Sender<T> {
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
}

impl<T> Receiver<T> {
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

pub fn channel<T>(_name: impl Into<CompactString>, capacity: usize) -> (Sender<T>, Receiver<T>) {
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

pub fn oneshot<T>(name: impl Into<CompactString>) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let name = name.into();
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
