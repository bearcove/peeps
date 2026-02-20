use moire_types::{EdgeKind, EntityBody, EntityId, RequestEntity, ResponseEntity, ResponseStatus};

use super::capture_backtrace_id;
use moire_runtime::{EntityHandle, EntityRef};

/// Instrumented request handle for a wrapped RPC request entity.
#[derive(Clone)]
pub struct RpcRequestHandle {
    handle: EntityHandle<moire_types::Request>,
}

impl RpcRequestHandle {
    /// Returns the underlying entity ID for this request.
    pub fn id(&self) -> &EntityId {
        self.handle.id()
    }

    /// Returns the request entity ID formatted for wire payloads.
    pub fn id_for_wire(&self) -> String {
        String::from(self.handle.id().as_str())
    }

    /// Returns a borrowed reference to the underlying entity handle.
    pub fn entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }

    #[doc(hidden)]
    pub fn handle(&self) -> &EntityHandle<moire_types::Request> {
        &self.handle
    }
}

// r[impl api.rpc-request]
/// Creates an instrumented RPC request handle equivalent to constructing a request entity.
pub fn rpc_request(method: impl Into<String>, args_json: impl Into<String>) -> RpcRequestHandle {
    let method = method.into();
    let (service_name, method_name) = split_method_parts(method.as_str());
    let service_name = String::from(service_name);
    let method_name = String::from(method_name);
    rpc_request_with_body(
        method,
        RequestEntity {
            service_name,
            method_name,
            args_json: moire_types::Json::new(args_json),
        },
    )
}

#[doc(hidden)]
pub fn rpc_request_with_body(name: impl Into<String>, body: RequestEntity) -> RpcRequestHandle {
    let source = capture_backtrace_id();
    let name = name.into();
    let body = EntityBody::Request(body);
    RpcRequestHandle {
        handle: EntityHandle::new(name, body, source).into_typed::<moire_types::Request>(),
    }
}

/// Creates an instrumented RPC response handle for the given method name.
pub fn rpc_response(method: impl Into<String>) -> EntityHandle<moire_types::Response> {
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
    )
}

#[doc(hidden)]
pub fn rpc_response_with_body(
    name: impl Into<String>,
    body: ResponseEntity,
) -> EntityHandle<moire_types::Response> {
    let source = capture_backtrace_id();
    let name = name.into();
    let body = EntityBody::Response(body);
    EntityHandle::new(name, body, source).into_typed::<moire_types::Response>()
}

// r[impl api.rpc-response]
/// Creates a response handle for a specific request entity, matching the upstream request.
pub fn rpc_response_for(
    method: impl Into<String>,
    request: &EntityRef,
) -> EntityHandle<moire_types::Response> {
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
    )
}

#[doc(hidden)]
pub fn rpc_response_for_with_body(
    name: impl Into<String>,
    request: &EntityRef,
    body: ResponseEntity,
) -> EntityHandle<moire_types::Response> {
    let source = capture_backtrace_id();
    let name = name.into();
    let body = EntityBody::Response(body);
    let response = EntityHandle::new(name, body, source).into_typed::<moire_types::Response>();
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
