use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, OnceLock};

use peeps_types::{Edge, GraphSnapshot, Node};

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

// ── External node storage ────────────────────────────────
//
// Stores nodes registered by external crates (e.g. roam registering
// request/response/channel nodes). These are included in emit_graph().

static EXTERNAL_NODES: LazyLock<Mutex<HashMap<String, Node>>> =
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

// ── External node registration ──────────────────────────

/// Register a node in the global registry.
///
/// Used by external crates (e.g. roam) to register request/response/channel
/// nodes that should appear in the canonical graph.
pub fn register_node(node: Node) {
    EXTERNAL_NODES
        .lock()
        .unwrap()
        .insert(node.id.clone(), node);
}

/// Remove a node from the global registry.
///
/// Also removes all edges to/from this node.
pub fn remove_node(id: &str) {
    EXTERNAL_NODES.lock().unwrap().remove(id);
    let mut edges = EDGES.lock().unwrap();
    edges.retain(|(s, d)| s != id && d != id);
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

    let canonical_edges: Vec<Edge> = EDGES
        .lock()
        .unwrap()
        .iter()
        .map(|(src, dst)| Edge {
            src: src.clone(),
            dst: dst.clone(),
            attrs_json: "{}".to_string(),
        })
        .collect();

    let external_nodes: Vec<Node> = EXTERNAL_NODES
        .lock()
        .unwrap()
        .values()
        .cloned()
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

    graph
}
