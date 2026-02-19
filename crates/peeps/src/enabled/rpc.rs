use peeps_types::{
    EdgeKind, EntityBody, EntityId, Event, EventKind, EventTarget, RequestEntity, ResponseEntity,
    ResponseStatus,
};

use super::{Source, SourceRight};
use peeps_runtime::{record_event_with_source, EntityHandle, EntityRef};

#[derive(Clone)]
pub struct RpcRequestHandle {
    handle: EntityHandle<peeps_types::Request>,
}

impl RpcRequestHandle {
    pub fn id(&self) -> &EntityId {
        self.handle.id()
    }

    pub fn id_for_wire(&self) -> String {
        String::from(self.handle.id().as_str())
    }

    pub fn entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }

    pub fn handle(&self) -> &EntityHandle<peeps_types::Request> {
        &self.handle
    }
}

#[derive(Clone)]
pub struct RpcResponseHandle {
    handle: EntityHandle<peeps_types::Response>,
}

impl RpcResponseHandle {
    pub fn id(&self) -> &EntityId {
        self.handle.id()
    }

    pub fn handle(&self) -> &EntityHandle<peeps_types::Response> {
        &self.handle
    }

    #[doc(hidden)]
    pub fn set_status_with_source(&self, status: ResponseStatus, source: Source) {
        let changed = self.handle.mutate(|body| body.status = status);
        if !changed {
            return;
        }
        let event = Event::new_with_source(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::StateChanged,
            source.clone(),
        );
        record_event_with_source(event, &source);
    }

    pub fn mark_ok(&self) {
        self.set_status_with_source(
            ResponseStatus::Ok,
            Source::new(SourceRight::caller().into_string(), None),
        );
    }

    pub fn mark_error(&self) {
        self.set_status_with_source(
            ResponseStatus::Error,
            Source::new(SourceRight::caller().into_string(), None),
        );
    }

    pub fn mark_cancelled(&self) {
        self.set_status_with_source(
            ResponseStatus::Cancelled,
            Source::new(SourceRight::caller().into_string(), None),
        );
    }
}

pub fn rpc_request(
    method: impl Into<String>,
    args_preview: impl Into<String>,
    source: SourceRight,
) -> RpcRequestHandle {
    let method = method.into();
    let body = EntityBody::Request(RequestEntity {
        method: method.clone(),
        args_preview: args_preview.into(),
    });
    RpcRequestHandle {
        handle: EntityHandle::new(method, body, source).into_typed::<peeps_types::Request>(),
    }
}

pub fn rpc_response(method: impl Into<String>, source: SourceRight) -> RpcResponseHandle {
    let method = method.into();
    let body = EntityBody::Response(ResponseEntity {
        method: method.clone(),
        status: ResponseStatus::Pending,
    });
    RpcResponseHandle {
        handle: EntityHandle::new(method, body, source).into_typed::<peeps_types::Response>(),
    }
}

pub fn rpc_response_for(
    method: impl Into<String>,
    request: &EntityRef,
    source: SourceRight,
) -> RpcResponseHandle {
    let method = method.into();
    let body = EntityBody::Response(ResponseEntity {
        method: method.clone(),
        status: ResponseStatus::Pending,
    });
    let response = RpcResponseHandle {
        handle: EntityHandle::new(method, body, source).into_typed::<peeps_types::Response>(),
    };
    response.handle.link_to(request, EdgeKind::PairedWith);
    response
}
