use compact_str::CompactString;
use peeps_types::{
    EdgeKind, Entity, EntityBody, EntityId, Event, EventKind, EventTarget, OperationEdgeMeta,
    OperationKind, OperationState,
};
use peeps_types::{infer_krate_from_source_with_manifest_dir, PTime};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::db::runtime_db;
use super::handles::{current_causal_target, AsEntityRef, EntityHandle, EntityRef};
use super::{record_event_with_entity_source, PeepsContext, Source, FUTURE_CAUSAL_STACK};

pub(super) struct OperationFuture<F> {
    inner: F,
    actor_id: Option<EntityId>,
    resource_id: EntityId,
    op_kind: OperationKind,
    source: CompactString,
    krate: Option<CompactString>,
    poll_count: u64,
    pending_since_ptime_ms: Option<u64>,
    has_edge: bool,
}

impl<F> OperationFuture<F> {
    fn new(
        inner: F,
        resource_id: EntityId,
        op_kind: OperationKind,
        source: CompactString,
        krate: Option<CompactString>,
    ) -> Self {
        let actor_id = current_causal_target().map(|target| target.id().clone());
        if let Some(actor_id) = actor_id.as_ref() {
            if let Ok(mut db) = runtime_db().lock() {
                db.upsert_edge(actor_id, &resource_id, EdgeKind::Touches);
            }
        }
        Self {
            inner,
            actor_id,
            resource_id,
            op_kind,
            source,
            krate,
            poll_count: 0,
            pending_since_ptime_ms: None,
            has_edge: false,
        }
    }

    fn edge_meta(&self, state: OperationState) -> facet_value::Value {
        let meta = OperationEdgeMeta {
            op_kind: self.op_kind,
            state,
            pending_since_ptime_ms: self.pending_since_ptime_ms,
            last_change_ptime_ms: PTime::now().as_millis(),
            source: CompactString::from(self.source.as_str()),
            krate: self.krate.as_ref().map(|k| CompactString::from(k.as_str())),
            poll_count: Some(self.poll_count),
            details: None,
        };
        facet_value::to_value(&meta).unwrap_or(facet_value::Value::NULL)
    }

    fn upsert_edge(&mut self, state: OperationState) {
        let Some(actor_id) = self.actor_id.as_ref() else {
            return;
        };
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge_with_meta(
                actor_id,
                &self.resource_id,
                EdgeKind::Needs,
                self.edge_meta(state),
            );
            self.has_edge = true;
        }
    }

    fn clear_edge(&mut self) {
        if !self.has_edge {
            return;
        }
        let Some(actor_id) = self.actor_id.as_ref() else {
            return;
        };
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_edge(actor_id, &self.resource_id, EdgeKind::Needs);
            self.has_edge = false;
        }
    }
}

impl<F> Future for OperationFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        this.poll_count = this.poll_count.saturating_add(1);
        if !this.has_edge {
            this.upsert_edge(OperationState::Active);
        }

        match unsafe { Pin::new_unchecked(&mut this.inner) }.poll(cx) {
            Poll::Pending => {
                if this.pending_since_ptime_ms.is_none() {
                    this.pending_since_ptime_ms = Some(PTime::now().as_millis());
                }
                this.upsert_edge(OperationState::Pending);
                Poll::Pending
            }
            Poll::Ready(output) => {
                this.clear_edge();
                Poll::Ready(output)
            }
        }
    }
}

impl<F> Drop for OperationFuture<F> {
    fn drop(&mut self) {
        self.clear_edge();
    }
}

pub(super) fn instrument_operation_on_with_source<F>(
    on: &EntityHandle,
    op_kind: OperationKind,
    fut: F,
    source: Source,
    cx: PeepsContext,
) -> OperationFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let source = source.into_compact_string();
    let krate = infer_krate_from_source_with_manifest_dir(source.as_str(), Some(cx.manifest_dir()));
    OperationFuture::new(
        fut.into_future(),
        EntityId::new(on.id().as_str()),
        op_kind,
        source,
        krate,
    )
}

pub struct InstrumentedFuture<F> {
    inner: F,
    future_handle: EntityHandle,
    awaited_by: Option<FutureEdgeRelation>,
    waits_on: Option<FutureEdgeRelation>,
}

#[derive(Clone, Copy)]
enum FutureEdgeDirection {
    ParentToChild,
    ChildToTarget,
}

struct FutureEdgeRelation {
    target: EntityRef,
    direction: FutureEdgeDirection,
    current_edge: Option<EdgeKind>,
}

impl FutureEdgeRelation {
    fn new(target: EntityRef, direction: FutureEdgeDirection) -> Self {
        Self {
            target,
            direction,
            current_edge: None,
        }
    }
}

impl<F> InstrumentedFuture<F> {
    fn new(inner: F, future_handle: EntityHandle, target: Option<EntityRef>) -> Self {
        let awaited_by = current_causal_target().and_then(|parent| {
            if parent.id().as_str() == future_handle.id().as_str() {
                None
            } else {
                Some(FutureEdgeRelation::new(
                    parent,
                    FutureEdgeDirection::ParentToChild,
                ))
            }
        });
        let waits_on = target
            .map(|target| FutureEdgeRelation::new(target, FutureEdgeDirection::ChildToTarget));
        Self {
            inner,
            future_handle,
            awaited_by,
            waits_on,
        }
    }
}

fn future_relation_endpoints(
    future_id: &EntityId,
    relation: &FutureEdgeRelation,
) -> (EntityId, EntityId) {
    match relation.direction {
        FutureEdgeDirection::ParentToChild => (
            EntityId::new(relation.target.id().as_str()),
            EntityId::new(future_id.as_str()),
        ),
        FutureEdgeDirection::ChildToTarget => (
            EntityId::new(future_id.as_str()),
            EntityId::new(relation.target.id().as_str()),
        ),
    }
}

fn ensure_relation_polls_edge(future_id: &EntityId, relation: &mut FutureEdgeRelation) {
    if relation.current_edge.is_some() {
        return;
    }
    let (src, dst) = future_relation_endpoints(future_id, relation);
    if let Ok(mut db) = runtime_db().lock() {
        db.upsert_edge(&src, &dst, EdgeKind::Polls);
    }
    relation.current_edge = Some(EdgeKind::Polls);
}

fn ensure_relation_needs_edge(future_id: &EntityId, relation: &mut FutureEdgeRelation) {
    if relation.current_edge == Some(EdgeKind::Needs) {
        return;
    }
    let (src, dst) = future_relation_endpoints(future_id, relation);
    if let Ok(mut db) = runtime_db().lock() {
        if relation.current_edge == Some(EdgeKind::Polls) {
            db.remove_edge(&src, &dst, EdgeKind::Polls);
        }
        db.upsert_edge(&src, &dst, EdgeKind::Needs);
    }
    relation.current_edge = Some(EdgeKind::Needs);
}

fn clear_relation_edge(future_id: &EntityId, relation: &mut FutureEdgeRelation) {
    let Some(kind) = relation.current_edge else {
        return;
    };
    let (src, dst) = future_relation_endpoints(future_id, relation);
    if let Ok(mut db) = runtime_db().lock() {
        db.remove_edge(&src, &dst, kind);
    }
    relation.current_edge = None;
}

impl<F> Future for InstrumentedFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let future_id = EntityId::new(this.future_handle.id().as_str());
        let pushed = FUTURE_CAUSAL_STACK
            .try_with(|stack| {
                stack.borrow_mut().push(EntityId::new(future_id.as_str()));
            })
            .is_ok();

        if let Some(relation) = this.awaited_by.as_mut() {
            ensure_relation_polls_edge(&future_id, relation);
        }
        if let Some(relation) = this.waits_on.as_mut() {
            ensure_relation_polls_edge(&future_id, relation);
        }

        let poll = unsafe { Pin::new_unchecked(&mut this.inner) }.poll(cx);
        if pushed {
            let _ = FUTURE_CAUSAL_STACK.try_with(|stack| {
                stack.borrow_mut().pop();
            });
        }
        match poll {
            Poll::Pending => {
                if let Some(relation) = this.awaited_by.as_mut() {
                    ensure_relation_needs_edge(&future_id, relation);
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    ensure_relation_needs_edge(&future_id, relation);
                }
                Poll::Pending
            }
            Poll::Ready(output) => {
                if let Some(relation) = this.awaited_by.as_mut() {
                    clear_relation_edge(&future_id, relation);
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    clear_relation_edge(&future_id, relation);
                }

                if let Ok(event) = Event::new(
                    EventTarget::Entity(EntityId::new(future_id.as_str())),
                    EventKind::StateChanged,
                    &(),
                ) {
                    record_event_with_entity_source(event, &future_id);
                }

                Poll::Ready(output)
            }
        }
    }
}

impl<F> Drop for InstrumentedFuture<F> {
    fn drop(&mut self) {
        let future_id = EntityId::new(self.future_handle.id().as_str());
        if let Some(relation) = self.awaited_by.as_mut() {
            clear_relation_edge(&future_id, relation);
        }
        if let Some(relation) = self.waits_on.as_mut() {
            clear_relation_edge(&future_id, relation);
        }
    }
}

pub fn instrument_future_named<F>(
    name: impl Into<CompactString>,
    fut: F,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    instrument_future_named_with_source(name, fut, source)
}

pub fn instrument_future_named_with_source<F>(
    name: impl Into<CompactString>,
    fut: F,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let fut = fut.into_future();
    let handle = EntityHandle::new_with_source(name, EntityBody::Future, source);
    InstrumentedFuture::new(fut, handle, None)
}

pub fn instrument_future_on<F>(
    name: impl Into<CompactString>,
    on: &impl AsEntityRef,
    fut: F,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    instrument_future_on_with_source(name, on, fut, source)
}

pub fn instrument_future_on_with_source<F>(
    name: impl Into<CompactString>,
    on: &impl AsEntityRef,
    fut: F,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let fut = fut.into_future();
    let on_ref = on.as_entity_ref();
    let handle = EntityHandle::new_with_source(name, EntityBody::Future, source);
    InstrumentedFuture::new(fut, handle, Some(on_ref))
}

#[doc(hidden)]
pub fn instrument_future_named_with_meta<F>(
    name: impl Into<CompactString>,
    fut: F,
    meta: &facet_value::Value,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let fut = fut.into_future();
    let mut entity = Entity::builder(name, EntityBody::Future)
        .source(source.into_compact_string())
        .build(&())
        .expect("entity construction with unit meta should be infallible");
    entity.meta = meta.clone();
    let handle = EntityHandle::from_entity(entity);
    InstrumentedFuture::new(fut, handle, None)
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
        let mut __peeps_meta_pairs: Vec<(&'static str, $crate::facet_value::Value)> = Vec::new();
        $(
            __peeps_meta_pairs.push((
                $k,
                $crate::facet_value::to_value(&$v)
                    .expect("`peep!` metadata value must be Facet-serializable"),
            ));
        )*
        let __peeps_meta: $crate::facet_value::Value = __peeps_meta_pairs.into_iter().collect();
        $crate::instrument_future_named_with_meta(
            $name,
            $fut,
            &__peeps_meta,
            $crate::Source::caller(),
        )
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
