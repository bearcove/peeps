use compact_str::CompactString;
use peeps_types::PTime;
use peeps_types::{
    EdgeKind, Entity, EntityBody, EntityId, Event, EventKind, EventTarget, OperationEdgeMeta,
    OperationKind, OperationState,
};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::db::runtime_db;
use super::handles::{current_causal_target, AsEntityRef, EntityHandle, EntityRef};
use super::{record_event_with_entity_source, Source, FUTURE_CAUSAL_STACK};

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
    source: &Source,
) -> OperationFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let source_text = CompactString::from(source.as_str());
    let krate = source.krate().map(CompactString::from);
    OperationFuture::new(
        fut.into_future(),
        EntityId::new(on.id().as_str()),
        op_kind,
        source_text,
        krate,
    )
}

pub struct InstrumentedFuture<F> {
    inner: F,
    pub(super) future_handle: EntityHandle,
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

fn transition_relation_edge(
    future_id: &EntityId,
    relation: &mut FutureEdgeRelation,
    next_edge: Option<EdgeKind>,
) {
    if relation.current_edge == next_edge {
        return;
    }

    let (src, dst) = match relation.direction {
        FutureEdgeDirection::ParentToChild => (
            EntityId::new(relation.target.id().as_str()),
            EntityId::new(future_id.as_str()),
        ),
        FutureEdgeDirection::ChildToTarget => (
            EntityId::new(future_id.as_str()),
            EntityId::new(relation.target.id().as_str()),
        ),
    };
    if let Ok(mut db) = runtime_db().lock() {
        if let Some(current_edge) = relation.current_edge {
            db.remove_edge(&src, &dst, current_edge);
        }
        if let Some(edge) = next_edge {
            db.upsert_edge(&src, &dst, edge);
        }
    }
    relation.current_edge = next_edge;
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
            transition_relation_edge(&future_id, relation, Some(EdgeKind::Polls));
        }
        if let Some(relation) = this.waits_on.as_mut() {
            transition_relation_edge(&future_id, relation, Some(EdgeKind::Polls));
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
                    transition_relation_edge(&future_id, relation, Some(EdgeKind::Needs));
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    transition_relation_edge(&future_id, relation, Some(EdgeKind::Needs));
                }
                Poll::Pending
            }
            Poll::Ready(output) => {
                if let Some(relation) = this.awaited_by.as_mut() {
                    transition_relation_edge(&future_id, relation, None);
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    transition_relation_edge(&future_id, relation, None);
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
            transition_relation_edge(&future_id, relation, None);
        }
        if let Some(relation) = self.waits_on.as_mut() {
            transition_relation_edge(&future_id, relation, None);
        }
    }
}

pub fn instrument_future<F>(
    name: impl Into<CompactString>,
    fut: F,
    source: Source,
    on: Option<EntityRef>,
    meta: Option<facet_value::Value>,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let fut = fut.into_future();
    let handle = if let Some(meta) = meta {
        let mut builder = Entity::builder(name, EntityBody::Future).source(source.as_str());
        if let Some(krate) = source.krate() {
            builder = builder.krate(krate);
        }
        let mut entity = builder
            .build(&())
            .expect("entity construction with unit meta should be infallible");
        entity.meta = meta;
        EntityHandle::from_entity(entity)
    } else {
        EntityHandle::new_with_source(name, EntityBody::Future, source)
    };
    InstrumentedFuture::new(fut, handle, on)
}
