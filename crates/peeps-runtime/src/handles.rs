use peeps_types::{
    EdgeKind, Entity, EntityBody, EntityBodySlot, EntityId, Scope, ScopeBody, ScopeId,
};
use std::marker::PhantomData;
use std::sync::{Arc, Weak};

use super::db::runtime_db;
use super::Source;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityRef {
    id: EntityId,
}

impl EntityRef {
    #[track_caller]
    pub fn id(&self) -> &EntityId {
        &self.id
    }
}

#[track_caller]
pub fn entity_ref_from_wire(id: impl Into<String>) -> EntityRef {
    EntityRef {
        id: EntityId::new(id.into()),
    }
}

pub fn current_causal_target() -> Option<EntityRef> {
    super::FUTURE_CAUSAL_STACK
        .try_with(|stack| {
            stack.borrow().last().map(|id| EntityRef {
                id: EntityId::new(id.as_str()),
            })
        })
        .ok()
        .flatten()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeRef {
    id: ScopeId,
}

impl ScopeRef {
    #[track_caller]
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
    pub fn new(name: impl Into<String>, body: ScopeBody, source: impl Into<Source>) -> Self {
        let source: Source = source.into();
        let scope = Scope::new(source.clone(), name, body);
        let id = ScopeId::new(scope.id.as_str());

        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_scope(scope);
        }

        Self {
            inner: Arc::new(ScopeHandleInner { id }),
        }
    }

    #[track_caller]
    pub fn id(&self) -> &ScopeId {
        &self.inner.id
    }

    #[track_caller]
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

pub struct EntityHandle<S = ()> {
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

impl EntityHandle<()> {
    pub fn new(name: impl Into<String>, body: EntityBody, source: impl Into<Source>) -> Self {
        Self::new_with_source(name, body, source)
    }

    pub fn new_with_source(
        name: impl Into<String>,
        body: EntityBody,
        source: impl Into<Source>,
    ) -> Self {
        let source: Source = source.into();
        let entity = Entity::new(source, name, body);
        Self::from_entity(entity)
    }

    pub fn from_entity(entity: Entity) -> Self {
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

    pub fn into_typed<S>(self) -> EntityHandle<S> {
        EntityHandle {
            inner: self.inner,
            _slot: PhantomData,
        }
    }
}

impl<S> EntityHandle<S> {
    #[track_caller]
    pub fn id(&self) -> &EntityId {
        &self.inner.id
    }

    #[track_caller]
    pub fn kind_name(&self) -> &'static str {
        self.inner.kind_name
    }

    #[track_caller]
    pub fn entity_ref(&self) -> EntityRef {
        EntityRef {
            id: EntityId::new(self.inner.id.as_str()),
        }
    }

    #[track_caller]
    pub fn link_to(&self, target: &EntityRef, kind: EdgeKind) {
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge(self.id(), target.id(), kind);
        }
    }

    #[track_caller]
    pub fn link_to_handle<T>(&self, target: &EntityHandle<T>, kind: EdgeKind) {
        self.link_to(&target.entity_ref(), kind);
    }

    #[track_caller]
    pub fn link_to_scope(&self, scope: &ScopeRef) {
        if let Ok(mut db) = runtime_db().lock() {
            db.link_entity_to_scope(self.id(), scope.id());
        }
    }

    #[track_caller]
    pub fn link_to_scope_handle(&self, scope: &ScopeHandle) {
        self.link_to_scope(&scope.scope_ref());
    }

    #[track_caller]
    pub fn unlink_from_scope(&self, scope: &ScopeRef) {
        if let Ok(mut db) = runtime_db().lock() {
            db.unlink_entity_from_scope(self.id(), scope.id());
        }
    }

    #[track_caller]
    pub fn unlink_from_scope_handle(&self, scope: &ScopeHandle) {
        self.unlink_from_scope(&scope.scope_ref());
    }
}

impl<S> EntityHandle<S>
where
    S: EntityBodySlot,
{
    #[track_caller]
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
pub struct WeakEntityHandle<S = ()> {
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
    /// Create an edge and return a handle that removes it when dropped.
    pub fn link_to_owned(&self, target: &EntityRef, kind: EdgeKind) -> EdgeHandle {
        let src = self.id().clone();
        let dst = target.id().clone();
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge(&src, &dst, kind);
        }
        EdgeHandle { src, dst, kind }
    }
}

/// A type that can be used as the `on =` argument of the `peeps!()` macro.
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
