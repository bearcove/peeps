use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Instant;

use peeps_types::{Edge, EdgeKind, GraphSnapshot, Node};

// ── Process metadata ─────────────────────────────────────

struct ProcessInfo {
    name: String,
    proc_key: String,
}

static PROCESS_INFO: OnceLock<ProcessInfo> = OnceLock::new();

// ── Canonical edge storage ───────────────────────────────
//
// Stores `needs` edges emitted via `stack::with_top(|src| registry::edge(src, dst))`.
// These represent the current wait graph: which futures are waiting on which resources.

static EDGES: LazyLock<Mutex<HashSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

// ── Historical interaction edge storage ──────────────────
//
// Stores `touches` edges: "src has interacted with dst at least once".
// Retained until either endpoint disappears.

static TOUCH_EDGES: LazyLock<Mutex<HashSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

// ── Spawn lineage edge storage ───────────────────────────
//
// Stores `spawned` edges: "src spawned dst". Permanent historical fact,
// retained for the lifetime of the child node.

static SPAWNED_EDGES: LazyLock<Mutex<HashSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

// ── External node storage ────────────────────────────────
//
// Stores nodes registered by external crates (e.g. roam registering
// request/response/channel nodes). These are included in emit_graph().

struct ExternalNodeEntry {
    node: Node,
    created_at: Instant,
}

static EXTERNAL_NODES: LazyLock<Mutex<HashMap<String, ExternalNodeEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// ── Initialization ───────────────────────────────────────

/// Initialize process metadata for the registry.
///
/// Should be called once at startup. Subsequent calls are ignored (first write wins).
pub(crate) fn init(process_name: &str, proc_key: &str) {
    let _ = PROCESS_INFO.set(ProcessInfo {
        name: process_name.to_string(),
        proc_key: proc_key.to_string(),
    });
}

// ── Accessors ────────────────────────────────────────────

pub(crate) fn process_name() -> Option<&'static str> {
    PROCESS_INFO.get().map(|p| p.name.as_str())
}

pub(crate) fn proc_key() -> Option<&'static str> {
    PROCESS_INFO.get().map(|p| p.proc_key.as_str())
}

// ── Edge tracking ────────────────────────────────────────

/// Record a canonical `needs` edge from `src` to `dst`.
///
/// Called from wrapper code via:
/// `stack::with_top(|src| registry::edge(src, resource_endpoint_id))`
pub fn edge(src: &str, dst: &str) {
    EDGES
        .lock()
        .unwrap()
        .insert((src.to_string(), dst.to_string()));
}

/// Remove a previously recorded edge.
///
/// Called when a resource is no longer being waited on (lock acquired,
/// message received, permits obtained, etc.).
pub fn remove_edge(src: &str, dst: &str) {
    EDGES
        .lock()
        .unwrap()
        .remove(&(src.to_string(), dst.to_string()));
}

/// Remove all edges originating from `src`.
///
/// Called when a future completes or is dropped, to clean up all
/// edges it may have emitted.
pub fn remove_edges_from(src: &str) {
    EDGES.lock().unwrap().retain(|(s, _)| s != src);
}

/// Remove all edges pointing to `dst`.
///
/// Called when a node is removed, to clean up all edges targeting it.
pub fn remove_edges_to(dst: &str) {
    EDGES.lock().unwrap().retain(|(_, d)| d != dst);
}

// ── Touch edge tracking ─────────────────────────────────

/// Record a `touches` edge from `src` to `dst`.
///
/// Indicates that `src` has interacted with `dst` at least once.
/// The edge is retained until either endpoint disappears.
/// Deduplicates: calling this multiple times is a no-op.
pub fn touch_edge(src: &str, dst: &str) {
    TOUCH_EDGES
        .lock()
        .unwrap()
        .insert((src.to_string(), dst.to_string()));
}

/// Remove a previously recorded touch edge.
pub fn remove_touch_edge(src: &str, dst: &str) {
    TOUCH_EDGES
        .lock()
        .unwrap()
        .remove(&(src.to_string(), dst.to_string()));
}

/// Remove all touch edges originating from `src`.
pub fn remove_touch_edges_from(src: &str) {
    TOUCH_EDGES.lock().unwrap().retain(|(s, _)| s != src);
}

/// Remove all touch edges pointing to `dst`.
pub fn remove_touch_edges_to(dst: &str) {
    TOUCH_EDGES.lock().unwrap().retain(|(_, d)| d != dst);
}

// ── Spawn edge tracking ─────────────────────────────────

/// Record a `spawned` edge from `src` to `dst`.
///
/// Indicates that `src` spawned `dst`. This is a permanent historical fact
/// retained for the lifetime of the child node.
pub fn spawn_edge(src: &str, dst: &str) {
    SPAWNED_EDGES
        .lock()
        .unwrap()
        .insert((src.to_string(), dst.to_string()));
}

/// Remove all spawn edges pointing to `dst`.
///
/// Called when the child node is dropped.
pub fn remove_spawn_edges_to(dst: &str) {
    SPAWNED_EDGES.lock().unwrap().retain(|(_, d)| d != dst);
}

// ── External node registration ──────────────────────────

/// Register a node in the global registry.
///
/// Used by external crates (e.g. roam) to register request/response/channel
/// nodes that should appear in the canonical graph.
pub fn register_node(node: Node) {
    let mut nodes = EXTERNAL_NODES.lock().unwrap();
    nodes
        .entry(node.id.clone())
        .and_modify(|entry| entry.node = node.clone())
        .or_insert_with(|| ExternalNodeEntry {
            node,
            created_at: Instant::now(),
        });
}

/// Remove a node from the global registry.
///
/// Also removes all edges (needs and touches) to/from this node.
pub fn remove_node(id: &str) {
    EXTERNAL_NODES.lock().unwrap().remove(id);
    EDGES.lock().unwrap().retain(|(s, d)| s != id && d != id);
    TOUCH_EDGES
        .lock()
        .unwrap()
        .retain(|(s, d)| s != id && d != id);
    SPAWNED_EDGES
        .lock()
        .unwrap()
        .retain(|(s, d)| s != id && d != id);
}

fn inject_elapsed_ns(attrs_json: &str, elapsed_ns: u64) -> String {
    if attrs_json.contains("\"elapsed_ns\"") {
        return attrs_json.to_string();
    }

    let trimmed = attrs_json.trim();
    if trimmed == "{}" {
        return format!("{{\"elapsed_ns\":{elapsed_ns}}}");
    }
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return attrs_json.to_string();
    }

    // Insert before trailing `}`.
    // Assumes attrs_json is valid JSON object (as required by Node.attrs_json).
    let insert_at = attrs_json.rfind('}').unwrap_or(attrs_json.len());
    let (head, tail) = attrs_json.split_at(insert_at);
    let needs_comma = head
        .chars()
        .rev()
        .find(|c| !c.is_whitespace())
        .is_some_and(|c| c != '{');
    if needs_comma {
        format!("{head},\"elapsed_ns\":{elapsed_ns}{tail}")
    } else {
        format!("{head}\"elapsed_ns\":{elapsed_ns}{tail}")
    }
}

// ── Graph emission ───────────────────────────────────────

/// Emit the canonical graph snapshot for all tracked resources.
///
/// Combines:
/// - Process metadata from `init()`
/// - Canonical `needs` edges from stack-mediated interactions
/// - Externally registered nodes (from `register_node()`)
/// - Resource-specific nodes and edges from each resource module
pub(crate) fn emit_graph() -> GraphSnapshot {
    let Some(info) = PROCESS_INFO.get() else {
        return GraphSnapshot::default();
    };

    let now = Instant::now();

    let mut canonical_edges: Vec<Edge> = EDGES
        .lock()
        .unwrap()
        .iter()
        .map(|(src, dst)| Edge {
            src: src.clone(),
            dst: dst.clone(),
            kind: EdgeKind::Needs,
            attrs_json: "{}".to_string(),
        })
        .collect();

    canonical_edges.extend(TOUCH_EDGES.lock().unwrap().iter().map(|(src, dst)| Edge {
        src: src.clone(),
        dst: dst.clone(),
        kind: EdgeKind::Touches,
        attrs_json: "{}".to_string(),
    }));

    canonical_edges.extend(SPAWNED_EDGES.lock().unwrap().iter().map(|(src, dst)| Edge {
        src: src.clone(),
        dst: dst.clone(),
        kind: EdgeKind::Spawned,
        attrs_json: "{}".to_string(),
    }));

    let external_nodes: Vec<Node> = EXTERNAL_NODES
        .lock()
        .unwrap()
        .values()
        .map(|entry| {
            let mut node = entry.node.clone();
            if matches!(node.kind, peeps_types::NodeKind::Request | peeps_types::NodeKind::Response)
            {
                let elapsed_ns =
                    (now.duration_since(entry.created_at).as_nanos().min(u64::MAX as u128)) as u64;
                node.attrs_json = inject_elapsed_ns(&node.attrs_json, elapsed_ns);
            }
            node
        })
        .collect();

    let mut graph = GraphSnapshot {
        process_name: info.name.clone(),
        proc_key: info.proc_key.clone(),
        nodes: external_nodes,
        edges: canonical_edges,
    };

    // Collect nodes and edges from each resource module.
    crate::futures::emit_into_graph(&mut graph);
    crate::locks::emit_into_graph(&mut graph);
    crate::sync::emit_into_graph(&mut graph);

    let elapsed = now.elapsed();

    let mut needs = 0u32;
    let mut touches = 0u32;
    let mut spawned = 0u32;
    let mut closed_by = 0u32;
    for e in &graph.edges {
        match e.kind {
            EdgeKind::Needs => needs += 1,
            EdgeKind::Touches => touches += 1,
            EdgeKind::Spawned => spawned += 1,
            EdgeKind::ClosedBy => closed_by += 1,
        }
    }

    let mut futures = 0u32;
    let mut locks = 0u32;
    let mut tx = 0u32;
    let mut rx = 0u32;
    let mut remote_tx = 0u32;
    let mut remote_rx = 0u32;
    let mut requests = 0u32;
    let mut responses = 0u32;
    let mut join_sets = 0u32;
    let mut semaphores = 0u32;
    let mut once_cells = 0u32;
    let mut commands = 0u32;
    let mut file_ops = 0u32;
    let mut notifies = 0u32;
    let mut sleeps = 0u32;
    let mut intervals = 0u32;
    let mut timeouts = 0u32;
    let mut net_connects = 0u32;
    let mut net_accepts = 0u32;
    let mut net_readables = 0u32;
    let mut net_writables = 0u32;
    let mut syscalls = 0u32;
    for n in &graph.nodes {
        match n.kind {
            peeps_types::NodeKind::Future => futures += 1,
            peeps_types::NodeKind::Lock => locks += 1,
            peeps_types::NodeKind::Tx => tx += 1,
            peeps_types::NodeKind::Rx => rx += 1,
            peeps_types::NodeKind::RemoteTx => remote_tx += 1,
            peeps_types::NodeKind::RemoteRx => remote_rx += 1,
            peeps_types::NodeKind::Request => requests += 1,
            peeps_types::NodeKind::Response => responses += 1,
            peeps_types::NodeKind::JoinSet => join_sets += 1,
            peeps_types::NodeKind::Semaphore => semaphores += 1,
            peeps_types::NodeKind::OnceCell => once_cells += 1,
            peeps_types::NodeKind::Command => commands += 1,
            peeps_types::NodeKind::FileOp => file_ops += 1,
            peeps_types::NodeKind::Notify => notifies += 1,
            peeps_types::NodeKind::Sleep => sleeps += 1,
            peeps_types::NodeKind::Interval => intervals += 1,
            peeps_types::NodeKind::Timeout => timeouts += 1,
            peeps_types::NodeKind::NetConnect => net_connects += 1,
            peeps_types::NodeKind::NetAccept => net_accepts += 1,
            peeps_types::NodeKind::NetReadable => net_readables += 1,
            peeps_types::NodeKind::NetWritable => net_writables += 1,
            peeps_types::NodeKind::Syscall => syscalls += 1,
        }
    }

    tracing::warn!(
        needs,
        touches,
        spawned,
        closed_by,
        futures,
        locks,
        tx,
        rx,
        remote_tx,
        remote_rx,
        requests,
        responses,
        join_sets,
        semaphores,
        once_cells,
        commands,
        file_ops,
        notifies,
        sleeps,
        intervals,
        timeouts,
        net_connects,
        net_accepts,
        net_readables,
        net_writables,
        syscalls,
        nodes = graph.nodes.len(),
        edges = graph.edges.len(),
        elapsed_us = elapsed.as_micros() as u64,
        "emit_graph completed"
    );

    graph
}

#[cfg(test)]
mod tests {
    use super::inject_elapsed_ns;

    #[test]
    fn inject_elapsed_ns_empty_object() {
        assert_eq!(inject_elapsed_ns("{}", 123), "{\"elapsed_ns\":123}");
    }

    #[test]
    fn inject_elapsed_ns_inserts_with_comma() {
        assert_eq!(inject_elapsed_ns("{\"a\":1}", 9), "{\"a\":1,\"elapsed_ns\":9}");
    }

    #[test]
    fn inject_elapsed_ns_inserts_without_comma_when_whitespace() {
        assert_eq!(
            inject_elapsed_ns("{  }", 42),
            "{  \"elapsed_ns\":42}"
        );
    }

    #[test]
    fn inject_elapsed_ns_noop_if_present() {
        assert_eq!(
            inject_elapsed_ns("{\"elapsed_ns\":1,\"a\":2}", 9),
            "{\"elapsed_ns\":1,\"a\":2}"
        );
    }

    #[test]
    fn inject_elapsed_ns_noop_if_not_object() {
        assert_eq!(inject_elapsed_ns("[]", 9), "[]");
        assert_eq!(inject_elapsed_ns("nope", 9), "nope");
    }
}
