use facet::Facet;
use peeps_types::{MetaBuilder, Node, NodeKind};

use super::RpcEvent;

#[derive(Facet)]
struct RpcAttrs {
    source: String,
    method: String,
}

#[derive(Facet)]
struct RpcEventAttrs {
    #[facet(rename = "rpc.connection")]
    rpc_connection: Option<String>,
}

#[derive(Facet)]
struct ConnectionAttrs<'a> {
    #[facet(rename = "rpc.connection")]
    rpc_connection: &'a str,
}

/// Record or update a request entity node.
///
/// The node remains present until explicitly removed by wrapper code via
/// `peeps::registry::remove_node(entity_id)`.
pub fn record_request(event: RpcEvent<'_>) {
    record(event, NodeKind::Request);
}

/// Record or update a response entity node.
///
/// The node remains present until explicitly removed by wrapper code via
/// `peeps::registry::remove_node(entity_id)`.
pub fn record_response(event: RpcEvent<'_>) {
    record(event, NodeKind::Response);
}

/// Record or update a request node using stack-built metadata.
///
/// Builds `attrs_json` as:
/// `{"method":"...","source":"..."}`.
#[track_caller]
#[doc(hidden)]
pub fn __peeps_track_rpc_request_with_meta(
    entity_id: &str,
    name: &str,
    _meta: MetaBuilder<'_>,
    parent_entity_id: Option<&str>,
) {
    let caller = std::panic::Location::caller();
    let attrs_json = attrs_json_for_method(name, crate::caller_location(caller));
    record_request(RpcEvent {
        entity_id,
        name,
        attrs_json: &attrs_json,
        parent_entity_id,
    });
}

/// Record or update a response node using stack-built metadata.
///
/// Builds `attrs_json` as:
/// `{"method":"...","source":"..."}`.
#[track_caller]
#[doc(hidden)]
pub fn __peeps_track_rpc_response_with_meta(
    entity_id: &str,
    name: &str,
    _meta: MetaBuilder<'_>,
    parent_entity_id: Option<&str>,
) {
    let caller = std::panic::Location::caller();
    let attrs_json = attrs_json_for_method(name, crate::caller_location(caller));
    record_response(RpcEvent {
        entity_id,
        name,
        attrs_json: &attrs_json,
        parent_entity_id,
    });
}

fn record(event: RpcEvent<'_>, kind: NodeKind) {
    crate::registry::register_node(Node {
        id: event.entity_id.to_string(),
        kind,
        label: Some(event.name.to_string()),
        attrs_json: event.attrs_json.to_string(),
    });

    let parent = event
        .parent_entity_id
        .map(ToOwned::to_owned)
        .or_else(crate::stack::capture_top);
    if let Some(parent_id) = parent {
        if parent_id != event.entity_id {
            crate::registry::edge(&parent_id, event.entity_id);
            crate::registry::touch_edge(&parent_id, event.entity_id);
        }
    }

    if let Some(connection) = extract_connection(event.attrs_json) {
        let connection_node_id = connection_node_id(&connection);
        crate::registry::register_node(Node {
            id: connection_node_id.clone(),
            kind: NodeKind::Connection,
            label: Some(connection.clone()),
            attrs_json: connection_attrs_json(&connection),
        });
        crate::registry::touch_edge(event.entity_id, &connection_node_id);
    }
}

fn attrs_json_for_method(name: &str, source: String) -> String {
    let attrs = RpcAttrs {
        source,
        method: name.to_string(),
    };
    facet_json::to_string(&attrs).unwrap()
}

fn extract_connection(attrs_json: &str) -> Option<String> {
    let attrs = facet_json::from_slice::<RpcEventAttrs>(attrs_json.as_bytes()).ok()?;
    attrs.rpc_connection.filter(|v| !v.is_empty())
}

fn connection_node_id(connection: &str) -> String {
    format!("connection:{connection}")
}

fn connection_attrs_json(connection: &str) -> String {
    let attrs = ConnectionAttrs {
        rpc_connection: connection,
    };
    facet_json::to_string(&attrs).unwrap()
}
