use moire_types::{RequestEntity, ResponseEntity};

/// No-op RPC request handle for the disabled (no-instrumentation) backend.
#[derive(Clone)]
pub struct RpcRequestHandle {
    id: String,
}

impl RpcRequestHandle {
    pub fn id_for_wire(&self) -> String {
        self.id.clone()
    }
}

/// No-op RPC response handle for the disabled backend.
#[derive(Clone)]
pub struct RpcResponseHandle;

impl RpcResponseHandle {
    pub fn mutate(&self, _f: impl FnOnce(&mut ResponseEntity)) -> bool {
        false
    }
}

pub fn rpc_request(method: impl Into<String>, args_json: impl Into<String>) -> RpcRequestHandle {
    let method = method.into();
    let args_json = args_json.into();
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

pub fn rpc_request_with_body(
    _name: impl Into<String>,
    _body: RequestEntity,
) -> RpcRequestHandle {
    RpcRequestHandle { id: String::new() }
}

pub fn rpc_response(method: impl Into<String>) -> RpcResponseHandle {
    let method = method.into();
    let (service_name, method_name) = split_method_parts(method.as_str());
    let service_name = String::from(service_name);
    let method_name = String::from(method_name);
    rpc_response_with_body(
        method,
        ResponseEntity {
            service_name,
            method_name,
            status: moire_types::ResponseStatus::Pending,
        },
    )
}

pub fn rpc_response_with_body(
    _name: impl Into<String>,
    _body: ResponseEntity,
) -> RpcResponseHandle {
    RpcResponseHandle
}

pub fn rpc_response_for(method: impl Into<String>, request: &RpcRequestHandle) -> RpcResponseHandle {
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
            status: moire_types::ResponseStatus::Pending,
        },
    )
}

pub fn rpc_response_for_with_body(
    _name: impl Into<String>,
    _request: &RpcRequestHandle,
    _body: ResponseEntity,
) -> RpcResponseHandle {
    RpcResponseHandle
}

fn split_method_parts(full_method: &str) -> (&str, &str) {
    if let Some((service, method)) = full_method.rsplit_once('.') {
        (service, method)
    } else {
        ("", full_method)
    }
}
