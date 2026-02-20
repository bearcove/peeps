use peeps_types::{
    EdgeKind, EntityBody, EntityId, RequestEntity, ResponseEntity, ResponseStatus,
};

use super::SourceId;
use peeps_runtime::{EntityHandle, EntityRef};

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

pub fn rpc_request(
    method: impl Into<String>,
    args_json: impl Into<String>,
    source: SourceId,
) -> RpcRequestHandle {
    let method = method.into();
    let (service_name, method_name) = split_method_parts(method.as_str());
    let service_name = String::from(service_name);
    let method_name = String::from(method_name);
    rpc_request_with_body(
        method,
        RequestEntity {
            service_name,
            method_name,
            args_json: peeps_types::Json::new(args_json),
        },
        source,
    )
}

pub fn rpc_request_with_body(
    name: impl Into<String>,
    body: RequestEntity,
    source: SourceId,
) -> RpcRequestHandle {
    let name = name.into();
    let body = EntityBody::Request(body);
    RpcRequestHandle {
        handle: EntityHandle::new(name, body, source).into_typed::<peeps_types::Request>(),
    }
}

pub fn rpc_response(
    method: impl Into<String>,
    source: SourceId,
) -> EntityHandle<peeps_types::Response> {
    let method = method.into();
    let (service_name, method_name) = split_method_parts(method.as_str());
    let service_name = String::from(service_name);
    let method_name = String::from(method_name);
    rpc_response_with_body(
        method,
        ResponseEntity {
            service_name,
            method_name,
            status: ResponseStatus::Pending,
        },
        source,
    )
}

pub fn rpc_response_with_body(
    name: impl Into<String>,
    body: ResponseEntity,
    source: SourceId,
) -> EntityHandle<peeps_types::Response> {
    let name = name.into();
    let body = EntityBody::Response(body);
    EntityHandle::new(name, body, source).into_typed::<peeps_types::Response>()
}

pub fn rpc_response_for(
    method: impl Into<String>,
    request: &EntityRef,
    source: SourceId,
) -> EntityHandle<peeps_types::Response> {
    let method = method.into();
    let (service_name, method_name) = split_method_parts(method.as_str());
    let service_name = String::from(service_name);
    let method_name = String::from(method_name);
    rpc_response_for_with_body(
        method,
        request,
        ResponseEntity {
            service_name,
            method_name,
            status: ResponseStatus::Pending,
        },
        source,
    )
}

pub fn rpc_response_for_with_body(
    name: impl Into<String>,
    request: &EntityRef,
    body: ResponseEntity,
    source: SourceId,
) -> EntityHandle<peeps_types::Response> {
    let name = name.into();
    let body = EntityBody::Response(body);
    let response = EntityHandle::new(name, body, source).into_typed::<peeps_types::Response>();
    response.link_to_with_source(request, EdgeKind::PairedWith, source);
    response
}

fn split_method_parts(full_method: &str) -> (&str, &str) {
    if let Some((service, method)) = full_method.rsplit_once('.') {
        (service, method)
    } else {
        ("", full_method)
    }
}
