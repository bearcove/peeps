use compact_str::CompactString;
use peeps_types::{
    CutAck, CutId, Edge, EdgeKind, Entity, EntityBody, Event, PullChangesResponse, Scope,
    ScopeBody, SeqNo, StreamCursor, StreamId,
};
use std::future::Future;
use tokio::sync::mpsc;

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

pub fn init(_process_name: &str) {}

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
