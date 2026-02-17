use compact_str::CompactString;
use peeps_types::{Edge, EdgeKind, Entity, EntityBody, EntityId, Event, EventKind, EventTarget};
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll};

const MAX_EVENTS: usize = 16_384;

fn runtime_db() -> &'static Mutex<RuntimeDb> {
    static DB: OnceLock<Mutex<RuntimeDb>> = OnceLock::new();
    DB.get_or_init(|| Mutex::new(RuntimeDb::new(MAX_EVENTS)))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct EdgeKey {
    src: EntityId,
    dst: EntityId,
    kind: EdgeKind,
}

struct RuntimeDb {
    entities: BTreeMap<EntityId, Entity>,
    edges: BTreeMap<EdgeKey, Edge>,
    events: VecDeque<Event>,
    max_events: usize,
}

impl RuntimeDb {
    fn new(max_events: usize) -> Self {
        Self {
            entities: BTreeMap::new(),
            edges: BTreeMap::new(),
            events: VecDeque::with_capacity(max_events.min(256)),
            max_events,
        }
    }

    fn upsert_entity(&mut self, entity: Entity) {
        self.entities.insert(entity.id.clone(), entity);
    }

    fn remove_entity(&mut self, id: &EntityId) {
        self.entities.remove(id);
        self.edges.retain(|k, _| &k.src != id && &k.dst != id);
    }

    fn upsert_edge(&mut self, src: &EntityId, dst: &EntityId, kind: EdgeKind) {
        let key = EdgeKey {
            src: src.clone(),
            dst: dst.clone(),
            kind,
        };
        let edge = Edge {
            src: src.clone(),
            dst: dst.clone(),
            kind,
            meta: facet_value::Value::NULL,
        };
        self.edges.insert(key, edge);
    }

    fn remove_edge(&mut self, src: &EntityId, dst: &EntityId, kind: EdgeKind) {
        self.edges.remove(&EdgeKey {
            src: src.clone(),
            dst: dst.clone(),
            kind,
        });
    }

    fn record_event(&mut self, event: Event) {
        self.events.push_back(event);
        while self.events.len() > self.max_events {
            self.events.pop_front();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityRef {
    id: EntityId,
}

impl EntityRef {
    pub fn id(&self) -> &EntityId {
        &self.id
    }
}

pub fn entity_ref_from_wire(id: impl Into<CompactString>) -> EntityRef {
    EntityRef {
        id: EntityId::new(id.into()),
    }
}

struct HandleInner {
    id: EntityId,
}

impl Drop for HandleInner {
    fn drop(&mut self) {
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_entity(&self.id);
        }
    }
}

#[derive(Clone)]
pub struct EntityHandle {
    inner: Arc<HandleInner>,
}

impl EntityHandle {
    pub fn new(name: impl Into<CompactString>, body: EntityBody) -> Self {
        let entity = Entity::builder(name, body)
            .build(&())
            .expect("entity construction with unit meta should be infallible");
        let id = EntityId::new(entity.id.as_str());

        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_entity(entity);
        }

        Self {
            inner: Arc::new(HandleInner { id }),
        }
    }

    pub fn id(&self) -> &EntityId {
        &self.inner.id
    }

    pub fn entity_ref(&self) -> EntityRef {
        EntityRef {
            id: self.inner.entity.id.clone(),
        }
    }

    pub fn link_to(&self, target: &EntityRef, kind: EdgeKind) {
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge(self.id(), target.id(), kind);
        }
    }

    pub fn link_to_handle(&self, target: &EntityHandle, kind: EdgeKind) {
        self.link_to(&target.entity_ref(), kind);
    }
}

pub trait SnapshotSink {
    fn entity(&mut self, entity: &Entity);
    fn edge(&mut self, edge: &Edge);
    fn event(&mut self, event: &Event);
}

pub fn write_snapshot_to<S>(sink: &mut S)
where
    S: SnapshotSink,
{
    let Ok(db) = runtime_db().lock() else {
        return;
    };
    for entity in db.entities.values() {
        sink.entity(entity);
    }
    for edge in db.edges.values() {
        sink.edge(edge);
    }
    for event in &db.events {
        sink.event(event);
    }
}

pub struct InstrumentedFuture<F> {
    inner: F,
    future_handle: EntityHandle,
    target: Option<EntityRef>,
}

impl<F> InstrumentedFuture<F> {
    fn new(inner: F, future_handle: EntityHandle, target: Option<EntityRef>) -> Self {
        Self {
            inner,
            future_handle,
            target,
        }
    }
}

impl<F> Future for InstrumentedFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        if let Some(target) = &this.target {
            if let Ok(mut db) = runtime_db().lock() {
                db.upsert_edge(this.future_handle.id(), target.id(), EdgeKind::Polls);
            }
        }

        let poll = unsafe { Pin::new_unchecked(&mut this.inner) }.poll(cx);
        match poll {
            Poll::Pending => {
                if let Some(target) = &this.target {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.remove_edge(this.future_handle.id(), target.id(), EdgeKind::Polls);
                        db.upsert_edge(this.future_handle.id(), target.id(), EdgeKind::Needs);
                    }
                }
                Poll::Pending
            }
            Poll::Ready(output) => {
                if let Some(target) = &this.target {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.remove_edge(this.future_handle.id(), target.id(), EdgeKind::Polls);
                        db.remove_edge(this.future_handle.id(), target.id(), EdgeKind::Needs);
                    }
                }

                if let Ok(event) = Event::new(
                    EventTarget::Entity(this.future_handle.id().clone()),
                    EventKind::StateChanged,
                    &(),
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }

                Poll::Ready(output)
            }
        }
    }
}

pub fn instrument_future_named<F>(name: impl Into<CompactString>, fut: F) -> InstrumentedFuture<F>
where
    F: Future,
{
    let handle = EntityHandle::new(name, EntityBody::Future);
    InstrumentedFuture::new(fut, handle, None)
}

pub fn instrument_future_on<F>(
    name: impl Into<CompactString>,
    on: &EntityHandle,
    fut: F,
) -> InstrumentedFuture<F>
where
    F: Future,
{
    let handle = EntityHandle::new(name, EntityBody::Future);
    InstrumentedFuture::new(fut, handle, Some(on.entity_ref()))
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
