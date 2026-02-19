use peeps_types::{
    EdgeKind, EntityBody, EntityId, Event, EventKind, EventTarget, RequestEntity, ResponseEntity,
    ResponseError, ResponseStatus,
};

use super::{local_source, Source, SourceRight};
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

    #[track_caller]
    pub fn mark_ok(&self) {
        self.mark_ok_json(peeps_types::Json::new("null"));
    }

    #[track_caller]
    pub fn mark_ok_json(&self, body_json: peeps_types::Json) {
        self.set_status_with_source(ResponseStatus::Ok(body_json), local_source(SourceRight::caller()));
    }

    #[track_caller]
    pub fn mark_error(&self) {
        self.mark_internal_error("error");
    }

    #[track_caller]
    pub fn mark_internal_error(&self, message: impl Into<String>) {
        self.set_status_with_source(
            ResponseStatus::Error(ResponseError::Internal(message.into())),
            local_source(SourceRight::caller()),
        );
    }

    #[track_caller]
    pub fn mark_user_error_json(&self, error_json: peeps_types::Json) {
        self.set_status_with_source(
            ResponseStatus::Error(ResponseError::UserJson(error_json)),
            local_source(SourceRight::caller()),
        );
    }

    #[track_caller]
    pub fn mark_cancelled(&self) {
        self.set_status_with_source(
            ResponseStatus::Cancelled,
            local_source(SourceRight::caller()),
        );
    }
}

pub fn rpc_request(
    method: impl Into<String>,
    args_json: impl Into<String>,
    source: SourceRight,
) -> RpcRequestHandle {
    let method = method.into();
    let (service_name, method_name) = split_method_parts(method.as_str());
    let body = EntityBody::Request(RequestEntity {
        service_name: String::from(service_name),
        method_name: String::from(method_name),
        args_json: peeps_types::Json::new(args_json),
    });
    RpcRequestHandle {
        handle: EntityHandle::new(method, body, local_source(source))
            .into_typed::<peeps_types::Request>(),
    }
}

pub fn rpc_response(method: impl Into<String>, source: SourceRight) -> RpcResponseHandle {
    let method = method.into();
    let (service_name, method_name) = split_method_parts(method.as_str());
    let body = EntityBody::Response(ResponseEntity {
        service_name: String::from(service_name),
        method_name: String::from(method_name),
        status: ResponseStatus::Pending,
    });
    RpcResponseHandle {
        handle: EntityHandle::new(method, body, local_source(source))
            .into_typed::<peeps_types::Response>(),
    }
}

pub fn rpc_response_for(
    method: impl Into<String>,
    request: &EntityRef,
    source: SourceRight,
) -> RpcResponseHandle {
    let method = method.into();
    let (service_name, method_name) = split_method_parts(method.as_str());
    let body = EntityBody::Response(ResponseEntity {
        service_name: String::from(service_name),
        method_name: String::from(method_name),
        status: ResponseStatus::Pending,
    });
    let source = local_source(source);
    let response = RpcResponseHandle {
        handle: EntityHandle::new(method, body, source.clone())
            .into_typed::<peeps_types::Response>(),
    };
    response
        .handle
        .link_to_with_source(request, EdgeKind::PairedWith, source);
    response
}

fn split_method_parts(full_method: &str) -> (&str, &str) {
    if let Some((service, method)) = full_method.rsplit_once('.') {
        (service, method)
    } else {
        ("", full_method)
    }
}
