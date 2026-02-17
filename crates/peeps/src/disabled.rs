use compact_str::CompactString;
use peeps_types::{Edge, EdgeKind, Entity, EntityBody, Event};

#[derive(Clone, Debug, Default)]
pub struct EntityRef;

#[derive(Clone, Debug, Default)]
pub struct EntityHandle;

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

pub fn entity_ref_from_wire(_id: impl Into<CompactString>) -> EntityRef {
    EntityRef
}

pub trait SnapshotSink {
    fn entity(&mut self, _entity: &Entity) {}
    fn edge(&mut self, _edge: &Edge) {}
    fn event(&mut self, _event: &Event) {}
}

pub fn write_snapshot_to<S>(_sink: &mut S)
where
    S: SnapshotSink,
{
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
