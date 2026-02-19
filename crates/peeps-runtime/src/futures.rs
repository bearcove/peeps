use peeps_types::{EdgeKind, EntityBody, EntityId, FutureEntity};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::db::runtime_db;
use super::handles::{current_causal_target, AsEntityRef, EntityHandle, EntityRef};
use super::{Source, FUTURE_CAUSAL_STACK};

pub struct OperationFuture<F> {
    inner: F,
    actor_id: Option<EntityId>,
    resource_id: EntityId,
    current_edge: Option<EdgeKind>,
}

impl<F> OperationFuture<F> {
    fn new(inner: F, resource_id: EntityId) -> Self {
        Self {
            inner,
            actor_id: current_causal_target().map(|target| target.id().clone()),
            resource_id,
            current_edge: None,
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
                db.upsert_edge(actor_id, &self.resource_id, edge);
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

pub fn instrument_operation_on_with_source<F>(
    on: &EntityHandle,
    fut: F,
    _source: &Source,
) -> OperationFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    OperationFuture::new(fut.into_future(), EntityId::new(on.id().as_str()))
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
                    transition_relation_edge(&future_id, relation, Some(EdgeKind::WaitingOn));
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    transition_relation_edge(&future_id, relation, Some(EdgeKind::WaitingOn));
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
    name: impl Into<String>,
    fut: F,
    source: Source,
    on: Option<EntityRef>,
    _meta: Option<facet_value::Value>,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let handle = EntityHandle::new_with_source(name, EntityBody::Future(FutureEntity {}), source);
    InstrumentedFuture::new(fut.into_future(), handle, on)
}
