/// Collect the canonical graph snapshot for all tracked resources.
///
/// Returns `None` if the graph is empty (no live resources).
///
/// Currently emits: futures, locks, sync primitives, and canonical edges.
/// Roam RPC nodes/edges will be re-added once roam migrates to the registry.
pub fn collect_graph(process_name: &str) -> Option<peeps_types::GraphSnapshot> {
    let pid = std::process::id();
    let proc_key = peeps_types::make_proc_key(process_name, pid);

    // Ensure registry has process info (first call wins via OnceLock).
    crate::registry::init(process_name, &proc_key);

    let graph = crate::registry::emit_graph();

    if graph.nodes.is_empty() && graph.edges.is_empty() {
        None
    } else {
        Some(graph)
    }
}
