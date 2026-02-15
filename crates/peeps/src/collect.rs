use std::collections::HashMap;

use peeps_types::{
    canonical_id, meta_key, Direction, Edge, GraphEdgeOrigin, MetaBuilder, MetaValue, Node,
    SessionSnapshot,
};

/// Collect only the canonical graph (tasks + roam), skipping all other diagnostics.
pub fn collect_graph(process_name: &str) -> Option<peeps_types::GraphSnapshot> {
    let pid = std::process::id();
    let proc_key = peeps_types::make_proc_key(process_name, pid);

    let mut graph = peeps_tasks::emit_graph(process_name, &proc_key);
    let roam = peeps_types::collect_roam_session();
    emit_roam_graph(process_name, &proc_key, &roam, &mut graph);

    if graph.nodes.is_empty() && graph.edges.is_empty() {
        None
    } else {
        Some(graph)
    }
}

// ── RPC + roam channel graph emission ───────────────────────────

/// Emit canonical graph nodes and edges for RPC requests and roam channels
/// into an existing `GraphSnapshot`.
fn emit_roam_graph(
    process_name: &str,
    proc_key: &str,
    roam: &Option<SessionSnapshot>,
    graph: &mut peeps_types::GraphSnapshot,
) {
    let Some(session) = roam.as_ref() else {
        return;
    };

    let mut request_node_ids: HashMap<(String, u64), String> = HashMap::new();
    let mut request_span_ids: HashMap<(String, u64), String> = HashMap::new();

    // ── RPC request/response nodes ──────────────────────────────
    for conn in &session.connections {
        let conn_name = &conn.name;
        let peer = conn.peer_name.as_deref().unwrap_or("");

        for req in &conn.in_flight {
            let method = req
                .method_name
                .as_deref()
                .or_else(|| session.method_names.get(&req.method_id).map(|s| s.as_str()))
                .unwrap_or("unknown");
            let elapsed_ns = (req.elapsed_secs * 1_000_000_000.0) as u64;
            let correlation = canonical_id::correlation_key(conn_name, req.request_id);

            let span_id = req
                .metadata
                .as_ref()
                .and_then(|m| m.get(peeps_types::PEEPS_SPAN_ID_KEY))
                .cloned();

            if let Some(ref sid) = span_id {
                request_span_ids.insert((conn_name.clone(), req.request_id), sid.clone());
            }

            match req.direction {
                Direction::Outgoing => {
                    let node_id = match &span_id {
                        Some(sid) => canonical_id::request_from_span_id(sid),
                        None => peeps_types::new_node_id("request"),
                    };
                    request_node_ids
                        .insert((conn_name.to_string(), req.request_id), node_id.clone());
                    let attrs =
                        build_request_attrs(req, method, elapsed_ns, conn_name, peer, &correlation);
                    graph.nodes.push(Node {
                        id: node_id,
                        kind: "request".to_string(),
                        process: process_name.to_string(),
                        proc_key: proc_key.to_string(),
                        label: Some(format!("{method} (outgoing)")),
                        attrs_json: attrs,
                    });
                }
                Direction::Incoming => {
                    let attrs = build_response_attrs(
                        req,
                        method,
                        elapsed_ns,
                        conn_name,
                        peer,
                        &correlation,
                    );
                    let response_id = peeps_types::new_node_id("response");
                    graph.nodes.push(Node {
                        id: response_id.clone(),
                        kind: "response".to_string(),
                        process: process_name.to_string(),
                        proc_key: proc_key.to_string(),
                        label: Some(format!("{method} (response)")),
                        attrs_json: attrs,
                    });

                    // request → response edge: use span_id to reference caller's request node
                    let caller_request_id = match &span_id {
                        Some(sid) => canonical_id::request_from_span_id(sid),
                        None => peeps_types::new_node_id("request"),
                    };
                    request_node_ids.insert(
                        (conn_name.to_string(), req.request_id),
                        caller_request_id.clone(),
                    );
                    graph.edges.push(Edge {
                        src_id: caller_request_id,
                        dst_id: response_id,
                        kind: "needs".to_string(),
                        observed_at_ns: None,
                        attrs_json: "{}".to_string(),
                        origin: GraphEdgeOrigin::Explicit,
                    });
                }
            }
        }
    }

    // ── Roam channel endpoint nodes ─────────────────────────────
    let mut tx_ids: HashMap<u64, String> = HashMap::new();
    let mut rx_ids: HashMap<u64, String> = HashMap::new();

    for ch in &session.channel_details {
        let endpoint = match ch.direction {
            peeps_types::ChannelDir::Tx => "tx",
            peeps_types::ChannelDir::Rx => "rx",
        };
        let kind = match ch.direction {
            peeps_types::ChannelDir::Tx => "roam_channel_tx",
            peeps_types::ChannelDir::Rx => "roam_channel_rx",
        };
        // Derive channel node ID from span_id (as chain_id) for cross-process matching
        let conn_for_channel = find_connection_for_channel(&session.connections, ch.channel_id);
        let chain_id = ch.request_id.and_then(|rid| {
            conn_for_channel.and_then(|cn| request_span_ids.get(&(cn.to_string(), rid)).cloned())
        });

        let node_id = match &chain_id {
            Some(cid) => canonical_id::roam_channel(cid, ch.channel_id, endpoint),
            None => peeps_types::new_node_id(kind),
        };
        let attrs = build_channel_attrs(ch);

        graph.nodes.push(Node {
            id: node_id.clone(),
            kind: kind.to_string(),
            process: process_name.to_string(),
            proc_key: proc_key.to_string(),
            label: if ch.name.is_empty() {
                None
            } else {
                Some(ch.name.clone())
            },
            attrs_json: attrs,
        });

        // request → endpoint edge
        if let Some(request_id) = ch.request_id {
            if let Some(conn_name) = conn_for_channel {
                if let Some(req_node_id) =
                    request_node_ids.get(&(conn_name.to_string(), request_id))
                {
                    graph.edges.push(Edge {
                        src_id: req_node_id.clone(),
                        dst_id: node_id.clone(),
                        kind: "needs".to_string(),
                        observed_at_ns: None,
                        attrs_json: "{}".to_string(),
                        origin: GraphEdgeOrigin::Explicit,
                    });
                }
            }
        }

        match ch.direction {
            peeps_types::ChannelDir::Tx => {
                tx_ids.insert(ch.channel_id, node_id);
            }
            peeps_types::ChannelDir::Rx => {
                rx_ids.insert(ch.channel_id, node_id);
            }
        }
    }

    // tx → rx edges
    for (channel_id, tx_node_id) in &tx_ids {
        if let Some(rx_node_id) = rx_ids.get(channel_id) {
            graph.edges.push(Edge {
                src_id: tx_node_id.clone(),
                dst_id: rx_node_id.clone(),
                kind: "needs".to_string(),
                observed_at_ns: None,
                attrs_json: "{}".to_string(),
                origin: GraphEdgeOrigin::Explicit,
            });
        }
    }
}

/// Build attrs_json for a request node (outgoing/caller side).
fn build_request_attrs(
    req: &peeps_types::RequestSnapshot,
    method: &str,
    elapsed_ns: u64,
    connection: &str,
    peer: &str,
    correlation_key: &str,
) -> String {
    let direction_str = match req.direction {
        Direction::Outgoing => "outgoing",
        Direction::Incoming => "incoming",
    };

    let mut meta = MetaBuilder::<16>::new();
    meta.push(meta_key::REQUEST_ID, MetaValue::U64(req.request_id));
    meta.push(meta_key::REQUEST_METHOD, MetaValue::Str(method));
    meta.push(
        meta_key::REQUEST_CORRELATION_KEY,
        MetaValue::Str(correlation_key),
    );
    meta.push(meta_key::RPC_CONNECTION, MetaValue::Str(connection));
    meta.push(meta_key::RPC_PEER, MetaValue::Str(peer));
    if let Some(tid) = req.task_id {
        meta.push(meta_key::TASK_ID, MetaValue::U64(tid));
    }
    let meta_json = meta.to_json_object();

    let args_preview = format_args_preview(&req.args);
    let rpc_metadata_json = format_rpc_metadata(&req.metadata);

    let task_id_str = match req.task_id {
        Some(id) => format!("{id}"),
        None => "null".to_string(),
    };

    let mut out = String::with_capacity(512);
    out.push('{');
    json_kv_u64(&mut out, "request_id", req.request_id, true);
    json_kv_str(&mut out, "method", method, false);
    json_kv_u64(&mut out, "method_id", req.method_id, false);
    json_kv_str(&mut out, "direction", direction_str, false);
    json_kv_u64(&mut out, "elapsed_ns", elapsed_ns, false);
    json_kv_str(&mut out, "connection", connection, false);
    json_kv_str(&mut out, "peer", peer, false);
    json_kv_raw(&mut out, "task_id", &task_id_str, false);
    json_kv_str(&mut out, "correlation_key", correlation_key, false);
    json_kv_str(&mut out, "args_preview", &args_preview, false);
    json_kv_raw(&mut out, "rpc_metadata_json", &rpc_metadata_json, false);
    if !meta_json.is_empty() {
        json_kv_raw(&mut out, "meta", &meta_json, false);
    }
    out.push('}');
    out
}

/// Build attrs_json for a response node (incoming/receiver side).
fn build_response_attrs(
    req: &peeps_types::RequestSnapshot,
    method: &str,
    elapsed_ns: u64,
    connection: &str,
    peer: &str,
    correlation_key: &str,
) -> String {
    let status = "in_flight";

    let mut meta = MetaBuilder::<16>::new();
    meta.push(meta_key::REQUEST_ID, MetaValue::U64(req.request_id));
    meta.push(meta_key::REQUEST_METHOD, MetaValue::Str(method));
    meta.push(
        meta_key::REQUEST_CORRELATION_KEY,
        MetaValue::Str(correlation_key),
    );
    meta.push(meta_key::RPC_CONNECTION, MetaValue::Str(connection));
    meta.push(meta_key::RPC_PEER, MetaValue::Str(peer));
    if let Some(tid) = req.server_task_id.or(req.task_id) {
        meta.push(meta_key::TASK_ID, MetaValue::U64(tid));
    }
    let meta_json = meta.to_json_object();

    let server_task_id_str = match req.server_task_id {
        Some(id) => format!("{id}"),
        None => "null".to_string(),
    };

    let mut out = String::with_capacity(512);
    out.push('{');
    json_kv_u64(&mut out, "request_id", req.request_id, true);
    json_kv_str(&mut out, "method", method, false);
    json_kv_str(&mut out, "status", status, false);
    json_kv_u64(&mut out, "elapsed_ns", elapsed_ns, false);
    json_kv_str(&mut out, "connection", connection, false);
    json_kv_str(&mut out, "peer", peer, false);
    json_kv_raw(&mut out, "server_task_id", &server_task_id_str, false);
    json_kv_str(&mut out, "correlation_key", correlation_key, false);
    if !meta_json.is_empty() {
        json_kv_raw(&mut out, "meta", &meta_json, false);
    }
    out.push('}');
    out
}

/// Build attrs_json for a roam channel endpoint node.
fn build_channel_attrs(ch: &peeps_types::RoamChannelSnapshot) -> String {
    let direction_str = match ch.direction {
        peeps_types::ChannelDir::Tx => "tx",
        peeps_types::ChannelDir::Rx => "rx",
    };
    let task_id_str = match ch.task_id {
        Some(id) => format!("{id}"),
        None => "null".to_string(),
    };
    let request_id_str = match ch.request_id {
        Some(id) => format!("{id}"),
        None => "null".to_string(),
    };
    let queue_depth_str = match ch.queue_depth {
        Some(d) => format!("{d}"),
        None => "null".to_string(),
    };

    let mut out = String::with_capacity(256);
    out.push('{');
    json_kv_u64(&mut out, "channel_id", ch.channel_id, true);
    json_kv_str(&mut out, "name", &ch.name, false);
    json_kv_str(&mut out, "direction", direction_str, false);
    json_kv_raw(&mut out, "queue_depth", &queue_depth_str, false);
    json_kv_raw(
        &mut out,
        "closed",
        if ch.closed { "true" } else { "false" },
        false,
    );
    json_kv_raw(&mut out, "request_id", &request_id_str, false);
    json_kv_raw(&mut out, "task_id", &task_id_str, false);
    out.push('}');
    out
}

/// Find which connection owns a given channel_id.
fn find_connection_for_channel<'a>(
    connections: &'a [peeps_types::ConnectionSnapshot],
    channel_id: u64,
) -> Option<&'a str> {
    for conn in connections {
        for ch in &conn.channels {
            if ch.channel_id == channel_id {
                return Some(&conn.name);
            }
        }
    }
    None
}

/// Format args as a preview string with middle-elision for large values.
fn format_args_preview(args: &Option<HashMap<String, String>>) -> String {
    let Some(args) = args else {
        return String::new();
    };
    if args.is_empty() {
        return String::new();
    }

    const MAX_VALUE_LEN: usize = 128;
    const ELIDE_THRESHOLD: usize = 256;

    let mut parts: Vec<String> = Vec::with_capacity(args.len());
    for (key, value) in args {
        let display_value = if value.len() > ELIDE_THRESHOLD {
            let prefix = &value[..MAX_VALUE_LEN / 2];
            let suffix = &value[value.len() - MAX_VALUE_LEN / 2..];
            format!(
                "{prefix}...({} bytes elided)...{suffix}",
                value.len() - MAX_VALUE_LEN
            )
        } else {
            value.clone()
        };
        parts.push(format!("{key}: {display_value}"));
    }
    parts.join(", ")
}

/// Format RPC metadata as a JSON string.
fn format_rpc_metadata(metadata: &Option<HashMap<String, String>>) -> String {
    let Some(meta) = metadata else {
        return "null".to_string();
    };
    if meta.is_empty() {
        return "{}".to_string();
    }
    let mut out = String::with_capacity(meta.len() * 32);
    out.push('{');
    let mut first = true;
    for (k, v) in meta {
        if !first {
            out.push(',');
        }
        first = false;
        out.push('"');
        peeps_types::json_escape_into(&mut out, k);
        out.push_str("\":\"");
        peeps_types::json_escape_into(&mut out, v);
        out.push('"');
    }
    out.push('}');
    out
}

// ── JSON builder helpers ────────────────────────────────────────

fn json_kv_str(out: &mut String, key: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":\"");
    peeps_types::json_escape_into(out, value);
    out.push('"');
}

fn json_kv_u64(out: &mut String, key: &str, value: u64, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(&value.to_string());
}

fn json_kv_raw(out: &mut String, key: &str, raw_value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(raw_value);
}

fn format_timestamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = d.as_secs();
    let millis = d.subsec_millis();

    let day_secs = (total_secs % 86400) as u32;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let seconds = day_secs % 60;

    let days = (total_secs / 86400) as i64;
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}
