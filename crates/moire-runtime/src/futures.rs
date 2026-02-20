use moire_trace_types::BacktraceId;
use moire_types::{EdgeKind, EntityId, FutureEntity};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::db::runtime_db;
use super::handles::{current_causal_target, EntityHandle, EntityRef};
use super::FUTURE_CAUSAL_STACK;

pub struct OperationFuture<F> {
    inner: F,
    actor_id: Option<EntityId>,
    resource_id: EntityId,
    current_edge: Option<EdgeKind>,
    source: BacktraceId,
}

impl<F> OperationFuture<F> {
    fn new(inner: F, resource_id: EntityId) -> Self {
        Self {
            inner,
            actor_id: current_causal_target().map(|target| target.id().clone()),
            resource_id,
            current_edge: None,
            source: super::capture_backtrace_id(),
        }
    }

    fn transition_edge(&mut self, next: Option<EdgeKind>) {
        if self.current_edge == next {
            return;
        }
        let Some(actor_id) = self.actor_id.as_ref() else {
            self.current_edge = next;
            return;
        };
        if let Ok(mut db) = runtime_db().lock() {
            if let Some(current) = self.current_edge {
                db.remove_edge(actor_id, &self.resource_id, current);
            }
            if let Some(edge) = next {
                db.upsert_edge_with_source(actor_id, &self.resource_id, edge, self.source);
            }
        }
        self.current_edge = next;
    }
}

impl<F> Future for OperationFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        if this.current_edge.is_none() {
            this.transition_edge(Some(EdgeKind::Polls));
        }

        match unsafe { Pin::new_unchecked(&mut this.inner) }.poll(cx) {
            Poll::Pending => {
                this.transition_edge(Some(EdgeKind::WaitingOn));
                Poll::Pending
            }
            Poll::Ready(output) => {
                this.transition_edge(None);
                Poll::Ready(output)
            }
        }
    }
}

impl<F> Drop for OperationFuture<F> {
    fn drop(&mut self) {
        self.transition_edge(None);
    }
}

pub fn instrument_operation_on<F, S>(on: &EntityHandle<S>, fut: F) -> OperationFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    OperationFuture::new(fut.into_future(), EntityId::new(on.id().as_str()))
}

pub struct InstrumentedFuture<F> {
    inner: F,
    pub(super) future_handle: EntityHandle<FutureEntity>,
    source: BacktraceId,
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
    fn new(inner: F, future_handle: EntityHandle<FutureEntity>, target: Option<EntityRef>) -> Self {
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
            source: super::capture_backtrace_id(),
            awaited_by,
            waits_on,
        }
    }
}

fn transition_relation_edge(
    future_id: &EntityId,
    source: BacktraceId,
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
            db.upsert_edge_with_source(&src, &dst, edge, source);
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
            transition_relation_edge(&future_id, this.source, relation, Some(EdgeKind::Polls));
        }
        if let Some(relation) = this.waits_on.as_mut() {
            transition_relation_edge(&future_id, this.source, relation, Some(EdgeKind::Polls));
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
                    transition_relation_edge(
                        &future_id,
                        this.source,
                        relation,
                        Some(EdgeKind::WaitingOn),
                    );
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    transition_relation_edge(
                        &future_id,
                        this.source,
                        relation,
                        Some(EdgeKind::WaitingOn),
                    );
                }
                Poll::Pending
            }
            Poll::Ready(output) => {
                if let Some(relation) = this.awaited_by.as_mut() {
                    transition_relation_edge(&future_id, this.source, relation, None);
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    transition_relation_edge(&future_id, this.source, relation, None);
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
            transition_relation_edge(&future_id, self.source, relation, None);
        }
        if let Some(relation) = self.waits_on.as_mut() {
            transition_relation_edge(&future_id, self.source, relation, None);
        }
    }
}

pub fn instrument_future<F>(
    name: impl Into<String>,
    fut: F,
    on: Option<EntityRef>,
    _meta: Option<facet_value::Value>,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let handle = EntityHandle::new(name, FutureEntity {});
    InstrumentedFuture::new(fut.into_future(), handle, on)
}
