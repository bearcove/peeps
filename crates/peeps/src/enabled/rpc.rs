use compact_str::CompactString;
use peeps_types::{
    EdgeKind, Entity, EntityBody, EntityId, Event, EventKind, EventTarget, RequestEntity,
    ResponseEntity, ResponseStatus,
};

use super::handles::{EntityHandle, EntityRef};
use super::UnqualSource;

#[derive(Clone)]
pub struct RpcRequestHandle {
    handle: EntityHandle,
}

impl RpcRequestHandle {
    #[track_caller]
    pub fn id(&self) -> &EntityId {
        self.handle.id()
    }

    #[track_caller]
    pub fn id_for_wire(&self) -> CompactString {
        CompactString::from(self.handle.id().as_str())
    }

    #[track_caller]
    pub fn entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }

    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }
}

#[derive(Clone)]
pub struct RpcResponseHandle {
    handle: EntityHandle,
}

impl RpcResponseHandle {
    #[track_caller]
    pub fn id(&self) -> &EntityId {
        self.handle.id()
    }

    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn set_status(&self, status: ResponseStatus) {
        let mut changed = false;
        if let Ok(mut db) = super::db::runtime_db().lock() {
            changed = db.update_response_status(self.handle.id(), status);
        }
        if !changed {
            return;
        }
        let source = UnqualSource::caller();
        if let Ok(event) = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::StateChanged,
            &status,
            source.into_compact_string(),
            None,
        ) {
            if let Ok(mut db) = super::db::runtime_db().lock() {
                db.record_event(event);
            }
        }
    }

    #[track_caller]
    pub fn mark_ok(&self) {
        self.set_status(ResponseStatus::Ok);
    }

    #[track_caller]
    pub fn mark_error(&self) {
        self.set_status(ResponseStatus::Error);
    }

    #[track_caller]
    pub fn mark_cancelled(&self) {
        self.set_status(ResponseStatus::Cancelled);
    }
}

#[track_caller]
pub fn rpc_request(
    method: impl Into<CompactString>,
    args_preview: impl Into<CompactString>,
    source: UnqualSource,
) -> RpcRequestHandle {
    let method = method.into();
    let body = EntityBody::Request(RequestEntity {
        method: method.clone(),
        args_preview: args_preview.into(),
    });
    RpcRequestHandle {
        handle: EntityHandle::new(method, body, source),
    }
}

#[macro_export]
macro_rules! rpc_request {
    ($method:expr, $args_preview:expr $(,)?) => {
        $crate::rpc_request($method, $args_preview, $crate::Source::caller())
    };
}

pub fn rpc_response(method: impl Into<CompactString>, source: UnqualSource) -> RpcResponseHandle {
    let method = method.into();
    let body = EntityBody::Response(ResponseEntity {
        method: method.clone(),
        status: ResponseStatus::Pending,
    });
    RpcResponseHandle {
        handle: EntityHandle::new(format!("{method}"), body, source),
    }
}

#[macro_export]
macro_rules! rpc_response {
    ($method:expr $(,)?) => {
        $crate::rpc_response($method, $crate::Source::caller())
    };
}

pub fn rpc_response_for(
    method: impl Into<CompactString>,
    request: &EntityRef,
    source: UnqualSource,
) -> RpcResponseHandle {
    let method = method.into();
    let request_source_and_krate = if let Ok(db) = super::db::runtime_db().lock() {
        db.entities
            .get(request.id())
            .map(|entity| (entity.source.clone(), entity.krate.clone()))
            .or_else(|| {
                let process_scope_id = super::current_process_scope_id()?;
                db.scopes
                    .get(&process_scope_id)
                    .map(|scope| (scope.source.clone(), scope.krate.clone()))
            })
    } else {
        None
    };

    let body = EntityBody::Response(ResponseEntity {
        method: method.clone(),
        status: ResponseStatus::Pending,
    });
    let mut builder = Entity::builder(format!("{method}"), body);
    if let Some((request_source, request_krate)) = request_source_and_krate {
        builder = builder.source(request_source);
        if let Some(request_krate) = request_krate {
            builder = builder.krate(request_krate);
        }
    } else {
        builder = builder.source(source.into_compact_string());
    }
    let entity = builder
        .build(&())
        .expect("response construction with unit meta should be infallible");
    let response = RpcResponseHandle {
        handle: EntityHandle::from_entity(entity),
    };
    if let Ok(mut db) = super::db::runtime_db().lock() {
        db.upsert_edge(request.id(), response.id(), EdgeKind::RpcLink);
    }
    response
}

#[macro_export]
macro_rules! rpc_response_for {
    ($method:expr, $request:expr $(,)?) => {
        $crate::rpc_response_for($method, $request, $crate::Source::caller())
    };
}
