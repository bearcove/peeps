use moire_types::{
    CustomEventKind, EdgeKind, Entity, EntityBody, EntityBodySlot, EntityId, EventKind,
    EventTarget, Json, Scope, ScopeBody, ScopeId,
};
use std::marker::PhantomData;
use std::sync::{Arc, Weak};

use super::db::runtime_db;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityRef {
    id: EntityId,
}

impl EntityRef {
    pub fn id(&self) -> &EntityId {
        &self.id
    }
}

pub fn entity_ref_from_wire(id: impl Into<String>) -> EntityRef {
    EntityRef {
        id: EntityId::new(id.into()),
    }
}

pub fn current_causal_target() -> Option<EntityRef> {
    current_causal_target_from_stack()
}

pub fn current_causal_target_from_stack() -> Option<EntityRef> {
    super::FUTURE_CAUSAL_STACK
        .try_with(|stack| {
            stack.borrow().last().map(|id| EntityRef {
                id: EntityId::new(id.as_str()),
            })
        })
        .ok()
        .flatten()
}

pub fn current_causal_target_with_task_fallback() -> Option<EntityRef> {
    current_causal_target_from_stack()
        .or_else(|| super::aether_entity_for_current_task().map(|id| EntityRef { id }))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeRef {
    id: ScopeId,
}

impl ScopeRef {
    pub fn id(&self) -> &ScopeId {
        &self.id
    }
}

struct ScopeHandleInner {
    id: ScopeId,
}

impl Drop for ScopeHandleInner {
    fn drop(&mut self) {
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_scope(&self.id);
        }
    }
}

#[derive(Clone)]
pub struct ScopeHandle {
    inner: Arc<ScopeHandleInner>,
}

impl ScopeHandle {
    pub fn new(name: impl Into<String>, body: ScopeBody) -> Self {
        let scope = Scope::new(super::capture_backtrace_id(), name, body);
        let id = ScopeId::new(scope.id.as_str());

        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_scope(scope);
        }

        Self {
            inner: Arc::new(ScopeHandleInner { id }),
        }
    }

    pub fn id(&self) -> &ScopeId {
        &self.inner.id
    }

    pub fn scope_ref(&self) -> ScopeRef {
        ScopeRef {
            id: ScopeId::new(self.inner.id.as_str()),
        }
    }
}

struct HandleInner {
    id: EntityId,
    kind_name: &'static str,
}

impl Drop for HandleInner {
    fn drop(&mut self) {
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_entity(&self.id);
        }
    }
}

pub struct EntityHandle<S> {
    inner: Arc<HandleInner>,
    _slot: PhantomData<S>,
}

impl<S> Clone for EntityHandle<S> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _slot: PhantomData,
        }
    }
}

impl<S> EntityHandle<S> {
    fn from_entity(entity: Entity) -> Self {
        let kind_name = entity.body.kind_name();
        let id = EntityId::new(entity.id.as_str());

        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_entity(entity);
        }

        Self {
            inner: Arc::new(HandleInner { id, kind_name }),
            _slot: PhantomData,
        }
    }
}

impl<S> EntityHandle<S>
where
    S: EntityBodySlot<Value = S> + Into<EntityBody>,
{
    pub fn new(name: impl Into<String>, body: S) -> Self {
        let entity = Entity::new(super::capture_backtrace_id(), name, body.into());
        Self::from_entity(entity)
    }
}

impl<S> EntityHandle<S> {
    pub fn id(&self) -> &EntityId {
        &self.inner.id
    }

    pub fn rename(&self, name: impl Into<String>) -> bool {
        let mut db = runtime_db()
            .lock()
            .expect("runtime db lock poisoned during entity rename");
        db.rename_entity_and_maybe_upsert(self.id(), name)
    }

    pub fn kind_name(&self) -> &'static str {
        self.inner.kind_name
    }

    pub fn entity_ref(&self) -> EntityRef {
        EntityRef {
            id: EntityId::new(self.inner.id.as_str()),
        }
    }

    pub fn link_to(&self, target: &EntityRef, kind: EdgeKind) {
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge(self.id(), target.id(), kind, super::capture_backtrace_id());
        }
    }

    pub fn link_to_handle<T>(&self, target: &EntityHandle<T>, kind: EdgeKind) {
        self.link_to(&target.entity_ref(), kind);
    }
}

impl<S> EntityHandle<S>
where
    S: EntityBodySlot,
{
    pub fn mutate(&self, f: impl FnOnce(&mut S::Value)) -> bool {
        if self.kind_name() != S::KIND_NAME {
            panic!(
                "entity kind mismatch for mutate: handle kind={} slot kind={} entity_id={}",
                self.kind_name(),
                S::KIND_NAME,
                self.id().as_str(),
            );
        }

        let mut db = runtime_db()
            .lock()
            .expect("runtime db lock poisoned during entity mutate");
        db.mutate_entity_body_and_maybe_upsert(self.id(), |body| {
            let slot = S::project_mut(body).unwrap_or_else(|| {
                panic!(
                    "entity body projection failed after kind check: kind={} entity_id={}",
                    S::KIND_NAME,
                    self.id().as_str(),
                )
            });
            f(slot);
        })
    }
}

impl<S> EntityHandle<S> {
    /// Emit a custom event on this entity.
    pub fn emit_event(
        &self,
        kind: impl Into<String>,
        display_name: impl Into<String>,
        payload: Json,
    ) {
        let event = super::new_event(
            EventTarget::Entity(EntityId::new(self.id().as_str())),
            EventKind::Custom(CustomEventKind {
                kind: kind.into(),
                display_name: display_name.into(),
                payload,
            }),
        );
        super::record_event(event);
    }
}

impl<S> EntityHandle<S> {
    pub fn downgrade(&self) -> WeakEntityHandle<S> {
        WeakEntityHandle {
            inner: Arc::downgrade(&self.inner),
            _slot: PhantomData,
        }
    }
}

/// A non-owning reference to an entity. Does not keep the entity alive.
/// When the last `EntityHandle` for the entity drops, the entity is removed
/// from the graph and subsequent `mutate` calls on any `WeakEntityHandle`
/// pointing to it become no-ops.
pub struct WeakEntityHandle<S> {
    inner: Weak<HandleInner>,
    _slot: PhantomData<S>,
}

impl<S> Clone for WeakEntityHandle<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _slot: PhantomData,
        }
    }
}

impl<S> WeakEntityHandle<S>
where
    S: EntityBodySlot,
{
    pub fn rename(&self, name: impl Into<String>) -> bool {
        let Some(inner) = self.inner.upgrade() else {
            return false;
        };
        let mut db = runtime_db()
            .lock()
            .expect("runtime db lock poisoned during weak entity rename");
        db.rename_entity_and_maybe_upsert(&inner.id, name)
    }

    pub fn mutate(&self, f: impl FnOnce(&mut S::Value)) -> bool {
        let Some(inner) = self.inner.upgrade() else {
            return false;
        };
        let mut db = runtime_db()
            .lock()
            .expect("runtime db lock poisoned during weak entity mutate");
        db.mutate_entity_body_and_maybe_upsert(&inner.id, |body| {
            let slot = S::project_mut(body).unwrap_or_else(|| {
                panic!(
                    "entity body projection failed: kind={} entity_id={}",
                    S::KIND_NAME,
                    inner.id.as_str(),
                )
            });
            f(slot);
        })
    }
}

/// An owned edge that removes itself from the graph when dropped.
///
/// Does not keep either endpoint entity alive — it only stores `EntityId` values.
/// If an endpoint entity is removed before this handle drops, the edge is already
/// gone and the Drop impl becomes a no-op.
pub struct EdgeHandle {
    src: EntityId,
    dst: EntityId,
    kind: EdgeKind,
}

impl Drop for EdgeHandle {
    fn drop(&mut self) {
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_edge(&self.src, &self.dst, self.kind);
        }
    }
}

impl<S> EntityHandle<S> {
    pub fn link_to_owned(&self, target: &impl AsEntityRef, kind: EdgeKind) -> EdgeHandle {
        self.as_entity_ref().link_to_owned(target, kind)
    }
}

impl EntityRef {
    pub fn link_to_owned(&self, target: &impl AsEntityRef, kind: EdgeKind) -> EdgeHandle {
        let src = self.id().clone();
        let dst = target.as_entity_ref().id().clone();
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge(&src, &dst, kind, super::capture_backtrace_id());
        }
        EdgeHandle { src, dst, kind }
    }
}

/// A type that can be used as the `on =` argument of the `moire!()` macro.
pub trait AsEntityRef {
    fn as_entity_ref(&self) -> EntityRef;
}

impl<S> AsEntityRef for EntityHandle<S> {
    fn as_entity_ref(&self) -> EntityRef {
        self.entity_ref()
    }
}

impl AsEntityRef for EntityRef {
    fn as_entity_ref(&self) -> EntityRef {
        self.clone()
    }
}
