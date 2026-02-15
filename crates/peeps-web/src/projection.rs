use std::collections::{HashMap, HashSet};

use peeps_types::{Edge, Node, GraphSt};, Node

/// A validated node row ready for SQLite insertion.
#[derive(Debug)]
pub struct ProjectedNode {
    pub id: String,
    pub kind: String,
    pub process: String,
    pub proc_key: String,
    pub label: Option<String>,
    pub attrs_json: String,
}

/// A validated edge row ready for SQLite insertion.
#[derive(Debug)]
pub struct ProjectedEdge {
    pub src_id: String,
    pub dst_id: String,
}

/// An edge whose endpoint could not be resolved in this snapshot.
#[derive(Debug)]
pub struct UnresolvedEdge {
    pub src_id: String,
    pub dst_id: String,
    pub missing_id: String,
}

/// An ingest-time error from projection validation.
#[derive(Debug)]
pub struct IngestError {
    pub message: String,
}

/// Result of projecting a set of per-process graph snapshots into canonical rows.
#[derive(Debug)]
pub struct ProjectionResult {
    pub nodes: Vec<ProjectedNode>,
    pub edges: Vec<ProjectedEdge>,
    pub unresolved_edges: Vec<UnresolvedEdge>,
    pub errors: Vec<IngestError>,
}

/// Project graph snapshots from multiple processes into canonical SQLite rows.
///
/// `graphs` maps process name to that process's `GraphSnapshot`.
/// `responded_processes` is the set of processes that actually responded
/// to the snapshot request — used to distinguish "unresolved" (process
/// didn't respond) from "ingest error" (process responded but node missing).
pub fn project_graphs(
    graphs: &HashMap<String, &GraphSnapshot>,
    responded_processes: &HashSet<String>,
) -> ProjectionResult {
    let mut result = ProjectionResult {
        nodes: Vec::new(),
        edges: Vec::new(),
        unresolved_edges: Vec::new(),
        errors: Vec::new(),
    };

    // Collect all node IDs and the process that owns each node.
    let mut node_ids: HashSet<String> = HashSet::new();
    let mut node_process: HashMap<String, String> = HashMap::new();

    for (_process_name, graph) in graphs {
        for node in &graph.nodes {
            if let Some(projected) = project_node(node, &mut result.errors) {
                node_ids.insert(projected.id.clone());
                node_process.insert(projected.id.clone(), projected.process.clone());
                result.nodes.push(projected);
            }
        }
    }

    for (_process_name, graph) in graphs {
        for edge in &graph.edges {
            project_edge(
                edge,
                &node_ids,
                &node_process,
                responded_processes,
                &mut result.edges,
                &mut result.unresolved_edges,
                &mut result.errors,
            );
        }
    }

    result
}

/// Project a single process's graph snapshot into canonical rows.
///
/// Convenience wrapper when you only have one process dump at a time.
pub fn project_single_process(process_name: &str, graph: &GraphSnapshot) -> ProjectionResult {
    let mut graphs = HashMap::new();
    graphs.insert(process_name.to_string(), graph);
    let mut responded = HashSet::new();
    responded.insert(process_name.to_string());
    project_graphs(&graphs, &responded)
}

fn project_node(node: &Node, errors: &mut Vec<IngestError>) -> Option<ProjectedNode> {
    if node.kind.is_empty() {
        errors.push(IngestError {
            message: format!("node '{}' has empty kind", node.id),
        });
        return None;
    }

    if node.id.is_empty() {
        errors.push(IngestError {
            message: "node has empty id".to_string(),
        });
        return None;
    }

    if node.proc_key.is_empty() {
        errors.push(IngestError {
            message: format!("node '{}' has empty proc_key", node.id),
        });
        return None;
    }

    Some(ProjectedNode {
        id: node.id.clone(),
        kind: node.kind.clone(),
        process: node.process.clone(),
        proc_key: node.proc_key.clone(),
        label: node.label.clone(),
        attrs_json: node.attrs_json.clone(),
    })
}

fn project_edge(
    edge: &Edge,
    node_ids: &HashSet<String>,
    node_process: &HashMap<String, String>,
    responded_processes: &HashSet<String>,
    edges: &mut Vec<ProjectedEdge>,
    unresolved: &mut Vec<UnresolvedEdge>,
    errors: &mut Vec<IngestError>,
) {
    // Only "needs" edges are accepted.
    if edge.kind != "needs" {
        errors.push(IngestError {
            message: format!(
                "edge {}→{} has kind '{}', only 'needs' is accepted",
                edge.src_id, edge.dst_id, edge.kind
            ),
        });
        return;
    }

    let src_exists = node_ids.contains(&edge.src_id);
    let dst_exists = node_ids.contains(&edge.dst_id);

    if src_exists && dst_exists {
        edges.push(ProjectedEdge {
            src_id: edge.src_id.clone(),
            dst_id: edge.dst_id.clone(),
        });
        return;
    }

    // Determine which endpoint is missing and whether that's an unresolved
    // (process didn't respond) or an ingest error (process responded but
    // the node wasn't in its graph).
    for (missing_id, exists) in [(&edge.src_id, src_exists), (&edge.dst_id, dst_exists)] {
        if exists {
            continue;
        }

        // Try to figure out which process should own this node by parsing
        // the ID format: {kind}:{proc_key}:{rest...}
        // The proc_key maps back to a process.
        let owning_process = infer_process_from_id(missing_id, node_process);

        match owning_process {
            Some(process) if responded_processes.contains(&process) => {
                // Process responded but node is missing — this is an ingest error.
                errors.push(IngestError {
                    message: format!(
                        "edge {}→{}: endpoint '{}' missing from process '{}' which responded",
                        edge.src_id, edge.dst_id, missing_id, process
                    ),
                });
            }
            _ => {
                // Process didn't respond or we can't determine the owner — unresolved.
                unresolved.push(UnresolvedEdge {
                    src_id: edge.src_id.clone(),
                    dst_id: edge.dst_id.clone(),
                    missing_id: missing_id.clone(),
                });
            }
        }
    }
}

/// Try to infer which process a node ID belongs to by extracting the proc_key
/// segment and looking it up in the known node→process mapping.
///
/// ID format: `{kind}:{proc_key}:{rest...}`
fn infer_process_from_id(node_id: &str, node_process: &HashMap<String, String>) -> Option<String> {
    // First: check if any existing node shares the same proc_key prefix.
    // Extract proc_key: skip first segment (kind), take second segment.
    let proc_key = extract_proc_key(node_id)?;

    // Find any node that has the same proc_key prefix and return its process.
    for (existing_id, process) in node_process {
        if let Some(existing_pk) = extract_proc_key(existing_id) {
            if existing_pk == proc_key {
                return Some(process.clone());
            }
        }
    }
    None
}

/// Extract the proc_key segment from a canonical node ID.
///
/// Format: `{kind}:{proc_key}:{rest...}`
/// Returns `None` if the ID doesn't have at least two colon-separated segments.
fn extract_proc_key(id: &str) -> Option<&str> {
    let mut parts = id.splitn(3, ':');
    let _kind = parts.next()?;
    let proc_key = parts.next()?;
    if proc_key.is_empty() {
        return None;
    }
    Some(proc_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use peeps_types::{GraphEdgeOrigin, Edge, Node, GraphSt};, Node

    fn make_node(id: &str, kind: &str, proc_key: &str) -> Node {
        Node {
            id: id.to_string(),
            kind: kind.to_string(),
            process: "test-process".to_string(),
            proc_key: proc_key.to_string(),
            label: None,
            attrs_json: "{}".to_string(),
        }
    }

    fn make_edge(src: &str, dst: &str) -> Edge {
        Edge {
            src_id: src.to_string(),
            dst_id: dst.to_string(),
            kind: "needs".to_string(),
            observed_at_ns: None,
            attrs_json: "{}".to_string(),
            origin: GraphEdgeOrigin::Explicit,
        }
    }

    #[test]
    fn project_valid_nodes_and_edges() {
        let graph = GraphSnapshot {
            nodes: vec![
                make_node("task:app-1:1", "task", "app-1"),
                make_node("future:app-1:10", "future", "app-1"),
            ],
            edges: vec![make_edge("task:app-1:1", "future:app-1:10")],
        };

        let result = project_single_process("test-process", &graph);
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);
        assert!(result.unresolved_edges.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn rejects_empty_kind() {
        let graph = GraphSnapshot {
            nodes: vec![make_node("bad:app-1:1", "", "app-1")],
            edges: vec![],
        };

        let result = project_single_process("test-process", &graph);
        assert!(result.nodes.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("empty kind"));
    }

    #[test]
    fn rejects_empty_id() {
        let mut node = make_node("", "task", "app-1");
        node.id = String::new();
        let graph = GraphSnapshot {
            nodes: vec![node],
            edges: vec![],
        };

        let result = project_single_process("test-process", &graph);
        assert!(result.nodes.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("empty id"));
    }

    #[test]
    fn rejects_non_needs_edge() {
        let graph = GraphSnapshot {
            nodes: vec![
                make_node("task:app-1:1", "task", "app-1"),
                make_node("task:app-1:2", "task", "app-1"),
            ],
            edges: vec![Edge {
                src_id: "task:app-1:1".to_string(),
                dst_id: "task:app-1:2".to_string(),
                kind: "spawned".to_string(),
                observed_at_ns: None,
                attrs_json: "{}".to_string(),
                origin: GraphEdgeOrigin::Explicit,
            }],
        };

        let result = project_single_process("test-process", &graph);
        assert!(result.edges.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0]
            .message
            .contains("only 'needs' is accepted"));
    }

    #[test]
    fn unresolved_edge_when_process_not_responded() {
        // Edge references a node in a different process that didn't respond.
        let graph = GraphSnapshot {
            nodes: vec![make_node("task:app-1:1", "task", "app-1")],
            edges: vec![make_edge("task:app-1:1", "response:other-2:conn_1:5")],
        };

        let result = project_single_process("test-process", &graph);
        assert!(result.edges.is_empty());
        assert_eq!(result.unresolved_edges.len(), 1);
        assert_eq!(
            result.unresolved_edges[0].missing_id,
            "response:other-2:conn_1:5"
        );
    }

    #[test]
    fn ingest_error_when_process_responded_but_node_missing() {
        // Both processes responded, but the destination node is missing.
        let graph_a = GraphSnapshot {
            nodes: vec![make_node("task:app-1:1", "task", "app-1")],
            edges: vec![make_edge("task:app-1:1", "future:app-1:99")],
        };

        // app-1 process responded but future:app-1:99 wasn't in its nodes
        let mut graphs = HashMap::new();
        graphs.insert("test-process".to_string(), &graph_a);
        let mut responded = HashSet::new();
        responded.insert("test-process".to_string());

        let result = project_graphs(&graphs, &responded);
        assert!(result.edges.is_empty());
        assert!(result.unresolved_edges.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("missing from process"));
    }

    #[test]
    fn multi_process_projection() {
        let graph_a = GraphSnapshot {
            nodes: vec![make_node("request:app-1:conn_1:5", "request", "app-1")],
            edges: vec![make_edge(
                "request:app-1:conn_1:5",
                "response:svc-2:conn_1:5",
            )],
        };
        let graph_b = GraphSnapshot {
            nodes: vec![make_node("response:svc-2:conn_1:5", "response", "svc-2")],
            edges: vec![],
        };

        let mut graphs = HashMap::new();
        graphs.insert("app".to_string(), &graph_a);
        graphs.insert("svc".to_string(), &graph_b);
        let mut responded = HashSet::new();
        responded.insert("app".to_string());
        responded.insert("svc".to_string());

        let result = project_graphs(&graphs, &responded);
        assert_eq!(result.nodes.len(), 2);
        assert_eq!(result.edges.len(), 1);
        assert!(result.unresolved_edges.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn extract_proc_key_parses_correctly() {
        assert_eq!(extract_proc_key("task:app-1:42"), Some("app-1"));
        assert_eq!(extract_proc_key("response:svc-2:conn_1:5"), Some("svc-2"));
        assert_eq!(extract_proc_key("nocolons"), None);
        assert_eq!(extract_proc_key("kind::rest"), None); // empty proc_key
    }

    #[test]
    fn rejects_empty_proc_key() {
        let mut node = make_node("task:app-1:1", "task", "app-1");
        node.proc_key = String::new();
        let graph = GraphSnapshot {
            nodes: vec![node],
            edges: vec![],
        };

        let result = project_single_process("test-process", &graph);
        assert!(result.nodes.is_empty());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("empty proc_key"));
    }
}
