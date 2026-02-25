use moire_trace_types::BacktraceId;
use moire_types::{EdgeKind, EntityId, FutureEntity};
use std::cell::RefCell;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

use super::FUTURE_CAUSAL_STACK;
use super::db::runtime_db;
use super::handles::{EntityHandle, EntityRef, current_causal_target_from_stack};

pub struct OperationFuture<F> {
    inner: F,
    actor_id: Option<EntityId>,
    resource_id: EntityId,
    current_edge: Option<EdgeKind>,
    backtrace: BacktraceId,
}

impl<F> OperationFuture<F> {
    fn new(inner: F, resource_id: EntityId) -> Self {
        Self::new_with_actor(
            inner,
            resource_id,
            current_causal_target_from_stack().map(|target| target.id().clone()),
        )
    }

    fn new_with_actor(inner: F, resource_id: EntityId, actor_id: Option<EntityId>) -> Self {
        Self {
            inner,
            actor_id,
            resource_id,
            current_edge: None,
            backtrace: super::capture_backtrace_id(),
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
                db.upsert_edge(actor_id, &self.resource_id, edge, self.backtrace);
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

pub fn instrument_operation_on_with_actor<F, S>(
    on: &EntityHandle<S>,
    actor: Option<&EntityRef>,
    fut: F,
) -> OperationFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    OperationFuture::new_with_actor(
        fut.into_future(),
        EntityId::new(on.id().as_str()),
        actor.map(|target| target.id().clone()),
    )
}

pub struct InstrumentedFuture<F> {
    inner: F,
    pub(super) future_handle: EntityHandle<FutureEntity>,
    backtrace: BacktraceId,
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
        let awaited_by = current_causal_target_from_stack().and_then(|parent| {
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
            backtrace: super::capture_backtrace_id(),
            awaited_by,
            waits_on,
        }
    }

    /// Sets how many entry frames to skip when displaying this future in the dashboard.
    pub fn skip_entry_frames(self, n: u8) -> Self {
        self.future_handle.mutate(|f| f.skip_entry_frames = Some(n));
        self
    }

    /// Sets the entity this future is waiting on, for dashboard edge display.
    pub fn on(mut self, target: EntityRef) -> Self {
        self.waits_on = Some(FutureEdgeRelation::new(
            target,
            FutureEdgeDirection::ChildToTarget,
        ));
        self
    }
}

fn transition_relation_edge(
    future_id: &EntityId,
    backtrace: BacktraceId,
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
            db.upsert_edge(&src, &dst, edge, backtrace);
        }
    }
    relation.current_edge = next_edge;
}

impl<F: Future> InstrumentedFuture<F> {
    fn poll_inner(&mut self, cx: &mut Context<'_>) -> Poll<F::Output> {
        let future_id = EntityId::new(self.future_handle.id().as_str());
        if let Ok(mut db) = runtime_db().lock() {
            let _ = db.link_entity_to_current_task_scope(&future_id);
        }
        FUTURE_CAUSAL_STACK.with(|stack| {
            stack.borrow_mut().push(EntityId::new(future_id.as_str()));
        });

        if let Some(relation) = self.awaited_by.as_mut() {
            transition_relation_edge(&future_id, self.backtrace, relation, Some(EdgeKind::Polls));
        }
        if let Some(relation) = self.waits_on.as_mut() {
            transition_relation_edge(&future_id, self.backtrace, relation, Some(EdgeKind::Polls));
        }

        let poll = unsafe { Pin::new_unchecked(&mut self.inner) }.poll(cx);
        FUTURE_CAUSAL_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });

        match poll {
            Poll::Pending => {
                if let Some(relation) = self.awaited_by.as_mut() {
                    transition_relation_edge(
                        &future_id,
                        self.backtrace,
                        relation,
                        Some(EdgeKind::WaitingOn),
                    );
                }
                if let Some(relation) = self.waits_on.as_mut() {
                    transition_relation_edge(
                        &future_id,
                        self.backtrace,
                        relation,
                        Some(EdgeKind::WaitingOn),
                    );
                }
                Poll::Pending
            }
            Poll::Ready(output) => {
                if let Some(relation) = self.awaited_by.as_mut() {
                    transition_relation_edge(&future_id, self.backtrace, relation, None);
                }
                if let Some(relation) = self.waits_on.as_mut() {
                    transition_relation_edge(&future_id, self.backtrace, relation, None);
                }
                Poll::Ready(output)
            }
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
        let has_stack = FUTURE_CAUSAL_STACK.try_with(|_| ()).is_ok();
        if has_stack {
            this.poll_inner(cx)
        } else {
            FUTURE_CAUSAL_STACK.sync_scope(RefCell::new(Vec::new()), || this.poll_inner(cx))
        }
    }
}

impl<F> Drop for InstrumentedFuture<F> {
    fn drop(&mut self) {
        let future_id = EntityId::new(self.future_handle.id().as_str());
        if let Some(relation) = self.awaited_by.as_mut() {
            transition_relation_edge(&future_id, self.backtrace, relation, None);
        }
        if let Some(relation) = self.waits_on.as_mut() {
            transition_relation_edge(&future_id, self.backtrace, relation, None);
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
    let handle = EntityHandle::new(name, FutureEntity::default());
    instrument_future_with_handle(handle, fut, on, None)
}

pub fn instrument_future_with_handle<F>(
    handle: EntityHandle<FutureEntity>,
    fut: F,
    on: Option<EntityRef>,
    _meta: Option<facet_value::Value>,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    InstrumentedFuture::new(fut.into_future(), handle, on)
}
