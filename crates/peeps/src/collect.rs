use std::collections::HashMap;

use peeps_types::{
    canonical_id, meta_key, Direction, Diagnostics, GraphEdgeOrigin, GraphEdgeSnapshot,
    GraphNodeSnapshot, MetaBuilder, MetaValue, ProcessDump,
    SessionSnapshot,
};

/// Manually collect a diagnostic dump.
pub fn collect_dump(process_name: &str, custom: HashMap<String, String>) -> ProcessDump {
    let timestamp = format_timestamp();

    let tasks = peeps_tasks::snapshot_all_tasks();
    let wake_edges = peeps_tasks::snapshot_wake_edges();
    let future_wake_edges = peeps_tasks::snapshot_future_wake_edges();
    let future_waits = peeps_tasks::snapshot_future_waits();
    let future_spawn_edges = peeps_tasks::snapshot_future_spawn_edges();
    let future_poll_edges = peeps_tasks::snapshot_future_poll_edges();
    let future_resume_edges = peeps_tasks::snapshot_future_resume_edges();
    let threads = peeps_threads::collect_all_thread_stacks();

    #[cfg(feature = "locks")]
    let locks = Some(peeps_locks::snapshot_lock_diagnostics());
    #[cfg(not(feature = "locks"))]
    let locks = None;

    let sync = {
        let snap = peeps_sync::snapshot_all();
        if snap.mpsc_channels.is_empty()
            && snap.oneshot_channels.is_empty()
            && snap.watch_channels.is_empty()
            && snap.once_cells.is_empty()
        {
            None
        } else {
            Some(snap)
        }
    };

    // Collect roam diagnostics from inventory-registered sources
    let all_diags = peeps_types::collect_all_diagnostics();
    let mut roam = None;
    let mut shm = None;
    for diag in all_diags {
        match diag {
            Diagnostics::RoamSession(s) => roam = Some(s),
            Diagnostics::RoamShm(s) => shm = Some(s),
        }
    }

    // Extract cross-process request parent edges from incoming request metadata.
    let request_parents = extract_request_parents(process_name, &roam);
    let future_resource_edges = collect_future_resource_edges(process_name, &future_waits);

    let pid = std::process::id();
    let proc_key = peeps_types::make_proc_key(process_name, pid);

    // Emit canonical graph from task/future instrumentation
    let mut graph = peeps_tasks::emit_graph(&proc_key);
    // Merge RPC request/response and roam channel nodes/edges
    emit_roam_graph(process_name, &proc_key, &roam, &mut graph);
    let graph = if graph.nodes.is_empty() && graph.edges.is_empty() {
        None
    } else {
        Some(graph)
    };

    ProcessDump {
        process_name: process_name.to_string(),
        pid,
        timestamp,
        tasks,
        wake_edges,
        future_wake_edges,
        future_waits,
        threads,
        locks,
        sync,
        roam,
        shm,
        future_spawn_edges,
        future_poll_edges,
        future_resume_edges,
        future_resource_edges,
        request_parents,
        graph,
        custom,
    }
}

fn collect_future_resource_edges(
    process_name: &str,
    waits: &[peeps_types::FutureWaitSnapshot],
) -> Vec<peeps_types::FutureResourceEdgeSnapshot> {
    waits
        .iter()
        .map(|w| {
            let resource = classify_resource_ref(process_name, &w.resource);
            peeps_types::FutureResourceEdgeSnapshot {
                future_id: w.future_id,
                resource,
                wait_count: w.pending_count,
                total_wait_secs: w.total_pending_secs,
                last_wait_age_secs: w.last_seen_age_secs,
            }
        })
        .collect()
}

fn classify_resource_ref(process_name: &str, raw: &str) -> peeps_types::ResourceRefSnapshot {
    use peeps_types::{ResourceRefSnapshot, SocketWaitDirection};

    if let Some(fd) = raw.strip_prefix("socket:").and_then(|s| s.parse::<u64>().ok()) {
        return ResourceRefSnapshot::Socket {
            process: process_name.to_string(),
            fd,
            label: Some(raw.to_string()),
            direction: None,
            peer: None,
        };
    }
    if raw.starts_with("socket.") || raw.contains(".socket.") || raw == "socket" {
        let direction = if raw.contains(".read") || raw.contains(".recv") {
            Some(SocketWaitDirection::Readable)
        } else if raw.contains(".write")
            || raw.contains(".send")
            || raw.contains(".flush")
            || raw.contains(".connect")
        {
            Some(SocketWaitDirection::Writable)
        } else {
            None
        };
        return ResourceRefSnapshot::Socket {
            process: process_name.to_string(),
            fd: 0,
            label: Some(raw.to_string()),
            direction,
            peer: None,
        };
    }

    if raw.starts_with("lock:") || raw.starts_with("lock.") {
        return ResourceRefSnapshot::Lock {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("lock:")
                .or_else(|| raw.strip_prefix("lock."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("mpsc:") || raw.starts_with("mpsc.") || raw.starts_with("channel.") {
        return ResourceRefSnapshot::Mpsc {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("mpsc:")
                .or_else(|| raw.strip_prefix("mpsc."))
                .or_else(|| raw.strip_prefix("channel."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("oneshot:") || raw.starts_with("oneshot.") {
        return ResourceRefSnapshot::Oneshot {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("oneshot:")
                .or_else(|| raw.strip_prefix("oneshot."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("watch:") || raw.starts_with("watch.") {
        return ResourceRefSnapshot::Watch {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("watch:")
                .or_else(|| raw.strip_prefix("watch."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("semaphore:") || raw.starts_with("semaphore.") {
        return ResourceRefSnapshot::Semaphore {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("semaphore:")
                .or_else(|| raw.strip_prefix("semaphore."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("once_cell:")
        || raw.starts_with("once_cell.")
        || raw.starts_with("oncecell:")
        || raw.starts_with("oncecell.")
    {
        return ResourceRefSnapshot::OnceCell {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("once_cell:")
                .or_else(|| raw.strip_prefix("once_cell."))
                .or_else(|| raw.strip_prefix("oncecell:"))
                .or_else(|| raw.strip_prefix("oncecell."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if let Some(id) = raw
        .strip_prefix("roam.channel.")
        .and_then(|s| s.parse::<u64>().ok())
    {
        return ResourceRefSnapshot::RoamChannel {
            process: process_name.to_string(),
            channel_id: id,
        };
    }

    ResourceRefSnapshot::Unknown {
        label: raw.to_string(),
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

            match req.direction {
                Direction::Outgoing => {
                    let attrs = build_request_attrs(
                        req,
                        method,
                        elapsed_ns,
                        conn_name,
                        peer,
                        &correlation,
                    );
                    graph.nodes.push(GraphNodeSnapshot {
                        id: canonical_id::request(proc_key, conn_name, req.request_id),
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
                    let response_id =
                        canonical_id::response(proc_key, conn_name, req.request_id);
                    graph.nodes.push(GraphNodeSnapshot {
                        id: response_id.clone(),
                        kind: "response".to_string(),
                        process: process_name.to_string(),
                        proc_key: proc_key.to_string(),
                        label: Some(format!("{method} (response)")),
                        attrs_json: attrs,
                    });

                    // request → response edge
                    let request_node_id =
                        resolve_caller_request_id(req, proc_key, conn_name);
                    graph.edges.push(GraphEdgeSnapshot {
                        src_id: request_node_id,
                        dst_id: response_id.clone(),
                        kind: "needs".to_string(),
                        observed_at_ns: None,
                        attrs_json: "{}".to_string(),
                        origin: GraphEdgeOrigin::Explicit,
                    });

                    // response → task edge when handler task is known
                    if let Some(task_id) = req.server_task_id.or(req.task_id) {
                        graph.edges.push(GraphEdgeSnapshot {
                            src_id: response_id,
                            dst_id: canonical_id::task(proc_key, task_id),
                            kind: "needs".to_string(),
                            observed_at_ns: None,
                            attrs_json: "{}".to_string(),
                            origin: GraphEdgeOrigin::Explicit,
                        });
                    }
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
        let node_id = canonical_id::roam_channel(proc_key, ch.channel_id, endpoint);
        let attrs = build_channel_attrs(ch);

        graph.nodes.push(GraphNodeSnapshot {
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

        // task → endpoint edge
        if let Some(task_id) = ch.task_id {
            graph.edges.push(GraphEdgeSnapshot {
                src_id: canonical_id::task(proc_key, task_id),
                dst_id: node_id.clone(),
                kind: "needs".to_string(),
                observed_at_ns: None,
                attrs_json: "{}".to_string(),
                origin: GraphEdgeOrigin::Explicit,
            });
        }

        // request → endpoint edge
        if let Some(request_id) = ch.request_id {
            if let Some(conn_name) =
                find_connection_for_channel(&session.connections, ch.channel_id)
            {
                graph.edges.push(GraphEdgeSnapshot {
                    src_id: canonical_id::request(proc_key, conn_name, request_id),
                    dst_id: node_id.clone(),
                    kind: "needs".to_string(),
                    observed_at_ns: None,
                    attrs_json: "{}".to_string(),
                    origin: GraphEdgeOrigin::Explicit,
                });
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
            graph.edges.push(GraphEdgeSnapshot {
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

/// Resolve the caller's request node ID from propagated metadata.
fn resolve_caller_request_id(
    req: &peeps_types::RequestSnapshot,
    local_proc_key: &str,
    local_conn: &str,
) -> String {
    if let Some(ref meta) = req.metadata {
        let caller_process = meta.get(peeps_types::PEEPS_CALLER_PROCESS_KEY);
        let caller_connection = meta.get(peeps_types::PEEPS_CALLER_CONNECTION_KEY);
        let caller_request_id = meta
            .get(peeps_types::PEEPS_CALLER_REQUEST_ID_KEY)
            .and_then(|v| v.parse::<u64>().ok());

        if let (Some(parent_process), Some(parent_connection), Some(parent_request_id)) =
            (caller_process, caller_connection, caller_request_id)
        {
            let caller_proc_key = peeps_types::sanitize_id_segment(parent_process);
            return canonical_id::request(&caller_proc_key, parent_connection, parent_request_id);
        }
    }
    canonical_id::request(local_proc_key, local_conn, req.request_id)
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

// ── Request parent extraction ───────────────────────────────────

/// Extract `RequestParentSnapshot` entries from incoming requests that carry
/// explicit caller identity metadata (`peeps.caller_process`, `peeps.caller_connection`,
/// `peeps.caller_request_id`).
fn extract_request_parents(
    process_name: &str,
    roam: &Option<peeps_types::SessionSnapshot>,
) -> Vec<peeps_types::RequestParentSnapshot> {
    let Some(session) = roam else {
        return vec![];
    };
    let mut parents = Vec::new();
    for conn in &session.connections {
        for req in &conn.in_flight {
            if !matches!(req.direction, peeps_types::Direction::Incoming) {
                continue;
            }
            let Some(ref meta) = req.metadata else {
                continue;
            };
            let caller_process = meta.get(peeps_types::PEEPS_CALLER_PROCESS_KEY);
            let caller_connection = meta.get(peeps_types::PEEPS_CALLER_CONNECTION_KEY);
            let caller_request_id = meta
                .get(peeps_types::PEEPS_CALLER_REQUEST_ID_KEY)
                .and_then(|v| v.parse::<u64>().ok());
            if let (Some(parent_process), Some(parent_connection), Some(parent_request_id)) =
                (caller_process, caller_connection, caller_request_id)
            {
                parents.push(peeps_types::RequestParentSnapshot {
                    child_process: process_name.to_string(),
                    child_connection: conn.name.clone(),
                    child_request_id: req.request_id,
                    parent_process: parent_process.clone(),
                    parent_connection: parent_connection.clone(),
                    parent_request_id,
                });
            }
        }
    }
    parents
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

#[cfg(test)]
mod tests {
    use super::*;
    use peeps_types::{
        ChannelDir, ChannelSnapshot, ConnectionSnapshot, Direction, GraphSnapshot,
        RoamChannelSnapshot, RequestSnapshot, SessionSnapshot, TransportStats,
    };

    fn make_transport() -> TransportStats {
        TransportStats {
            frames_sent: 0,
            frames_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            last_sent_ago_secs: None,
            last_recv_ago_secs: None,
        }
    }

    fn empty_graph() -> GraphSnapshot {
        GraphSnapshot::empty()
    }

    #[test]
    fn emit_outgoing_request_node() {
        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn_1".to_string(),
                peer_name: Some("backend".to_string()),
                age_secs: 5.0,
                total_completed: 10,
                max_concurrent_requests: 32,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 42,
                    method_name: Some("get_page".to_string()),
                    method_id: 3,
                    direction: Direction::Outgoing,
                    elapsed_secs: 0.1,
                    task_id: Some(5),
                    task_name: Some("caller".to_string()),
                    metadata: None,
                    args: Some(HashMap::from([
                        ("path".to_string(), "/index.html".to_string()),
                    ])),
                    backtrace: None,
                    server_task_id: None,
                    server_task_name: None,
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![],
        };

        let mut graph = empty_graph();
        emit_roam_graph("frontend", "frontend-100", &Some(session), &mut graph);

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "request:frontend-100:conn_1:42");
        assert_eq!(graph.nodes[0].kind, "request");
        assert!(graph.nodes[0].attrs_json.contains("\"request_id\":42"));
        assert!(graph.nodes[0].attrs_json.contains("\"method\":\"get_page\""));
        assert!(graph.nodes[0].attrs_json.contains("\"direction\":\"outgoing\""));
        assert!(graph.nodes[0].attrs_json.contains("\"correlation_key\":\"conn_1:42\""));
        assert!(graph.nodes[0].attrs_json.contains("\"args_preview\":\"path: /index.html\""));
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn emit_incoming_response_node_and_edges() {
        let mut meta = HashMap::new();
        meta.insert(
            peeps_types::PEEPS_CALLER_PROCESS_KEY.to_string(),
            "frontend".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_CONNECTION_KEY.to_string(),
            "conn_1".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_REQUEST_ID_KEY.to_string(),
            "42".to_string(),
        );

        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn_2".to_string(),
                peer_name: Some("frontend".to_string()),
                age_secs: 5.0,
                total_completed: 10,
                max_concurrent_requests: 32,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 7,
                    method_name: Some("get_page".to_string()),
                    method_id: 3,
                    direction: Direction::Incoming,
                    elapsed_secs: 0.05,
                    task_id: Some(20),
                    task_name: Some("handler".to_string()),
                    metadata: Some(meta),
                    args: None,
                    backtrace: None,
                    server_task_id: Some(20),
                    server_task_name: Some("handler".to_string()),
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![],
        };

        let mut graph = empty_graph();
        emit_roam_graph("backend", "backend-200", &Some(session), &mut graph);

        // Should have 1 response node
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].id, "response:backend-200:conn_2:7");
        assert_eq!(graph.nodes[0].kind, "response");
        assert!(graph.nodes[0].attrs_json.contains("\"status\":\"in_flight\""));
        assert!(graph.nodes[0].attrs_json.contains("\"correlation_key\":\"conn_2:7\""));
        assert!(graph.nodes[0].attrs_json.contains("\"server_task_id\":20"));

        // Should have 2 edges: request→response, response→task
        assert_eq!(graph.edges.len(), 2);
        // request→response uses caller metadata to build cross-process ID
        assert_eq!(graph.edges[0].src_id, "request:frontend:conn_1:42");
        assert_eq!(graph.edges[0].dst_id, "response:backend-200:conn_2:7");
        assert_eq!(graph.edges[0].kind, "needs");
        // response→task
        assert_eq!(graph.edges[1].src_id, "response:backend-200:conn_2:7");
        assert_eq!(graph.edges[1].dst_id, "task:backend-200:20");
    }

    #[test]
    fn emit_roam_channel_nodes_and_edges() {
        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn_1".to_string(),
                peer_name: None,
                age_secs: 1.0,
                total_completed: 0,
                max_concurrent_requests: 10,
                initial_credit: 65536,
                in_flight: vec![],
                recent_completions: vec![],
                channels: vec![ChannelSnapshot {
                    channel_id: 99,
                    direction: ChannelDir::Tx,
                    age_secs: 0.5,
                    request_id: Some(42),
                }],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![
                RoamChannelSnapshot {
                    channel_id: 99,
                    name: "events".to_string(),
                    direction: ChannelDir::Tx,
                    age_secs: 0.5,
                    request_id: Some(42),
                    task_id: Some(10),
                    task_name: Some("sender".to_string()),
                    queue_depth: Some(5),
                    closed: false,
                },
                RoamChannelSnapshot {
                    channel_id: 99,
                    name: "events".to_string(),
                    direction: ChannelDir::Rx,
                    age_secs: 0.5,
                    request_id: Some(42),
                    task_id: Some(11),
                    task_name: Some("receiver".to_string()),
                    queue_depth: None,
                    closed: false,
                },
            ],
        };

        let mut graph = empty_graph();
        emit_roam_graph("app", "app-300", &Some(session), &mut graph);

        // 2 channel endpoint nodes
        assert_eq!(graph.nodes.len(), 2);
        let tx_node = graph.nodes.iter().find(|n| n.kind == "roam_channel_tx").unwrap();
        let rx_node = graph.nodes.iter().find(|n| n.kind == "roam_channel_rx").unwrap();
        assert_eq!(tx_node.id, "roam-channel:app-300:99:tx");
        assert_eq!(rx_node.id, "roam-channel:app-300:99:rx");
        assert!(tx_node.attrs_json.contains("\"queue_depth\":5"));
        assert!(rx_node.attrs_json.contains("\"queue_depth\":null"));

        // Edges: task→tx, request→tx, task→rx, request→rx, tx→rx
        assert_eq!(graph.edges.len(), 5);
        let has_edge = |src: &str, dst: &str| graph.edges.iter().any(|e| e.src_id == src && e.dst_id == dst);
        assert!(has_edge("task:app-300:10", "roam-channel:app-300:99:tx"));
        assert!(has_edge("task:app-300:11", "roam-channel:app-300:99:rx"));
        assert!(has_edge("request:app-300:conn_1:42", "roam-channel:app-300:99:tx"));
        assert!(has_edge("request:app-300:conn_1:42", "roam-channel:app-300:99:rx"));
        assert!(has_edge("roam-channel:app-300:99:tx", "roam-channel:app-300:99:rx"));
    }

    #[test]
    fn emit_no_graph_when_no_session() {
        let mut graph = empty_graph();
        emit_roam_graph("app", "app-100", &None, &mut graph);
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn args_preview_elides_large_values() {
        let large_value = "x".repeat(300);
        let args = Some(HashMap::from([("data".to_string(), large_value)]));
        let preview = format_args_preview(&args);
        assert!(preview.contains("bytes elided"));
        assert!(preview.len() < 300);
    }

    #[test]
    fn method_resolved_from_method_names_map() {
        let mut method_names = HashMap::new();
        method_names.insert(5u64, "subscribe".to_string());

        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn_1".to_string(),
                peer_name: None,
                age_secs: 1.0,
                total_completed: 0,
                max_concurrent_requests: 10,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 1,
                    method_name: None,
                    method_id: 5,
                    direction: Direction::Outgoing,
                    elapsed_secs: 0.01,
                    task_id: None,
                    task_name: None,
                    metadata: None,
                    args: None,
                    backtrace: None,
                    server_task_id: None,
                    server_task_name: None,
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names,
            channel_details: vec![],
        };

        let mut graph = empty_graph();
        emit_roam_graph("app", "app-100", &Some(session), &mut graph);

        assert_eq!(graph.nodes.len(), 1);
        assert!(graph.nodes[0].attrs_json.contains("\"method\":\"subscribe\""));
    }

    #[test]
    fn extract_request_parents_from_incoming_metadata() {
        let mut meta = HashMap::new();
        meta.insert(
            peeps_types::PEEPS_CALLER_PROCESS_KEY.to_string(),
            "frontend".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_CONNECTION_KEY.to_string(),
            "conn-a".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_REQUEST_ID_KEY.to_string(),
            "42".to_string(),
        );

        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn-b".to_string(),
                peer_name: None,
                age_secs: 1.0,
                total_completed: 0,
                max_concurrent_requests: 10,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 7,
                    method_name: Some("get_page".to_string()),
                    method_id: 1,
                    direction: Direction::Incoming,
                    elapsed_secs: 0.5,
                    task_id: Some(10),
                    task_name: Some("handler".to_string()),
                    metadata: Some(meta),
                    args: None,
                    backtrace: None,
                    server_task_id: Some(10),
                    server_task_name: Some("handler".to_string()),
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![],
        };

        let parents = extract_request_parents("backend", &Some(session));
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].child_process, "backend");
        assert_eq!(parents[0].child_connection, "conn-b");
        assert_eq!(parents[0].child_request_id, 7);
        assert_eq!(parents[0].parent_process, "frontend");
        assert_eq!(parents[0].parent_connection, "conn-a");
        assert_eq!(parents[0].parent_request_id, 42);
    }

    #[test]
    fn extract_request_parents_skips_outgoing() {
        let mut meta = HashMap::new();
        meta.insert(
            peeps_types::PEEPS_CALLER_PROCESS_KEY.to_string(),
            "other".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_CONNECTION_KEY.to_string(),
            "conn".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_REQUEST_ID_KEY.to_string(),
            "1".to_string(),
        );

        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn".to_string(),
                peer_name: None,
                age_secs: 1.0,
                total_completed: 0,
                max_concurrent_requests: 10,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 1,
                    method_name: None,
                    method_id: 1,
                    direction: Direction::Outgoing,
                    elapsed_secs: 0.5,
                    task_id: None,
                    task_name: None,
                    metadata: Some(meta),
                    args: None,
                    backtrace: None,
                    server_task_id: None,
                    server_task_name: None,
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![],
        };

        let parents = extract_request_parents("app", &Some(session));
        assert!(parents.is_empty());
    }

    #[test]
    fn extract_request_parents_skips_missing_metadata() {
        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn".to_string(),
                peer_name: None,
                age_secs: 1.0,
                total_completed: 0,
                max_concurrent_requests: 10,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 1,
                    method_name: None,
                    method_id: 1,
                    direction: Direction::Incoming,
                    elapsed_secs: 0.5,
                    task_id: None,
                    task_name: None,
                    metadata: None,
                    args: None,
                    backtrace: None,
                    server_task_id: None,
                    server_task_name: None,
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![],
        };

        let parents = extract_request_parents("app", &Some(session));
        assert!(parents.is_empty());
    }
}
