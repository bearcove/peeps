use compact_str::CompactString;
use peeps_types::{EdgeKind, Entity, EntityBody, EntityId, Scope, ScopeBody, ScopeId};
use std::sync::Arc;

use super::db::runtime_db;
use super::UnqualSource;

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
pub fn entity_ref_from_wire(id: impl Into<CompactString>) -> EntityRef {
    EntityRef {
        id: EntityId::new(id.into()),
    }
}

pub(super) fn current_causal_target() -> Option<EntityRef> {
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
    pub fn new(name: impl Into<CompactString>, body: ScopeBody, source: UnqualSource) -> Self {
        let scope = Scope::builder(name, body)
            .source(source.into_compact_string())
            .build(&())
            .expect("scope construction with unit meta should be infallible");
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
    pub fn new(name: impl Into<CompactString>, body: EntityBody, source: UnqualSource) -> Self {
        Self::new_with_source(name, body, source)
    }

    pub fn new_with_source(
        name: impl Into<CompactString>,
        body: EntityBody,
        source: UnqualSource,
    ) -> Self {
        let entity = Entity::builder(name, body)
            .source(source.into_compact_string())
            .build(&())
            .expect("entity construction with unit meta should be infallible");
        Self::from_entity(entity)
    }

    pub(super) fn from_entity(entity: Entity) -> Self {
        let id = EntityId::new(entity.id.as_str());

        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_entity(entity);
        }

        Self {
            inner: Arc::new(HandleInner { id }),
        }
    }

    #[track_caller]
    pub fn id(&self) -> &EntityId {
        &self.inner.id
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
    pub fn link_to_handle(&self, target: &EntityHandle, kind: EdgeKind) {
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

/// A type that can be used as the `on =` argument of the `peeps!()` macro.
pub trait AsEntityRef {
    fn as_entity_ref(&self) -> EntityRef;
}

impl AsEntityRef for EntityHandle {
    fn as_entity_ref(&self) -> EntityRef {
        self.entity_ref()
    }
}

impl AsEntityRef for EntityRef {
    fn as_entity_ref(&self) -> EntityRef {
        self.clone()
    }
}
