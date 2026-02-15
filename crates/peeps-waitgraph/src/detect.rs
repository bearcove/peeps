//! Deadlock candidate detection via Tarjan's SCC algorithm.
//!
//! Operates on the blocking-edge subgraph of a [`WaitGraph`]. Blocking edges
//! are those that form wait-for chains: `TaskWaitsOnResource` and
//! `ResourceOwnedByTask`. When these form a cycle, we have a deadlock
//! candidate.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::{EdgeKind, NodeId, NodeKind, WaitEdge, WaitGraph};

// ── Configuration ───────────────────────────────────────────────

/// Thresholds for severity classification.
#[derive(Debug, Clone)]
pub struct SeverityConfig {
    /// Score at or above which a candidate is "danger".
    pub danger_threshold: u32,
    /// Score at or above which a candidate is "warn".
    pub warn_threshold: u32,
}

impl Default for SeverityConfig {
    fn default() -> Self {
        Self {
            danger_threshold: 50,
            warn_threshold: 20,
        }
    }
}

// ── Severity levels ─────────────────────────────────────────────

/// Severity classification of a deadlock candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Danger,
}

// ── Candidate payload ───────────────────────────────────────────

/// A deadlock candidate detected in the wait graph.
#[derive(Debug, Clone)]
pub struct DeadlockCandidate {
    /// The set of nodes participating in this candidate cycle.
    pub nodes: BTreeSet<NodeId>,
    /// The edges that form the cycle (subset of the graph's edges).
    pub edges: Vec<WaitEdge>,
    /// A representative cycle path: sequence of node IDs forming the loop.
    pub cycle_path: Vec<NodeId>,
    /// Numeric severity score (higher = worse).
    pub severity_score: u32,
    /// Classification based on thresholds.
    pub severity: Severity,
    /// Human-readable rationale strings explaining the score.
    pub rationale: Vec<String>,
}

// ── Public API ──────────────────────────────────────────────────

/// Detect deadlock candidates in the wait graph.
///
/// Returns candidates sorted by severity score (highest first).
pub fn find_deadlock_candidates(graph: &WaitGraph) -> Vec<DeadlockCandidate> {
    find_deadlock_candidates_with_config(graph, &SeverityConfig::default())
}

/// Detect deadlock candidates with custom severity thresholds.
pub fn find_deadlock_candidates_with_config(
    graph: &WaitGraph,
    config: &SeverityConfig,
) -> Vec<DeadlockCandidate> {
    let blocking_adj = build_blocking_adjacency(graph);
    let sccs = tarjan_scc(&blocking_adj);
    let blocking_edges = collect_blocking_edges(graph);

    let mut candidates: Vec<DeadlockCandidate> = Vec::new();

    for scc in &sccs {
        // Single-node SCCs without a self-loop are not cycles
        if scc.len() == 1 {
            let node = &scc[0];
            let has_self_loop = blocking_adj
                .get(node)
                .map_or(false, |neighbors| neighbors.contains(node));
            if !has_self_loop {
                continue;
            }
        }

        let scc_set: BTreeSet<NodeId> = scc.iter().cloned().collect();

        // Collect edges within the SCC
        let scc_edges: Vec<WaitEdge> = blocking_edges
            .iter()
            .filter(|e| scc_set.contains(&e.from) && scc_set.contains(&e.to))
            .cloned()
            .collect();

        // Find a representative cycle path through the SCC
        let cycle_path = find_representative_cycle(&scc_set, &blocking_adj);

        // Score and classify
        let (score, rationale) = compute_severity(graph, &scc_set, &scc_edges);
        let severity = classify(score, config);

        candidates.push(DeadlockCandidate {
            nodes: scc_set,
            edges: scc_edges,
            cycle_path,
            severity_score: score,
            severity,
            rationale,
        });
    }

    // Sort by severity score descending (worst first)
    candidates.sort_by(|a, b| b.severity_score.cmp(&a.severity_score));
    candidates
}

// ── Blocking-edge subgraph ──────────────────────────────────────

/// Edge kinds that participate in deadlock cycles.
fn is_blocking_edge(kind: &EdgeKind) -> bool {
    matches!(
        kind,
        EdgeKind::TaskWaitsOnResource
            | EdgeKind::ResourceOwnedByTask
            | EdgeKind::RpcClientToRequest
            | EdgeKind::RpcRequestToServerTask
            | EdgeKind::RpcCrossProcessStitch
    )
}

/// Build adjacency list for blocking edges only.
fn build_blocking_adjacency(graph: &WaitGraph) -> BTreeMap<NodeId, Vec<NodeId>> {
    let mut adj: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();

    // Ensure all nodes that participate in blocking edges are in the map
    for edge in &graph.edges {
        if is_blocking_edge(&edge.kind) {
            adj.entry(edge.from.clone()).or_default().push(edge.to.clone());
            adj.entry(edge.to.clone()).or_default();
        }
    }

    adj
}

/// Collect all blocking edges for SCC edge extraction.
fn collect_blocking_edges(graph: &WaitGraph) -> Vec<WaitEdge> {
    graph
        .edges
        .iter()
        .filter(|e| is_blocking_edge(&e.kind))
        .cloned()
        .collect()
}

// ── Tarjan's SCC ────────────────────────────────────────────────

struct TarjanState {
    index_counter: usize,
    stack: Vec<NodeId>,
    on_stack: HashSet<NodeId>,
    index: HashMap<NodeId, usize>,
    lowlink: HashMap<NodeId, usize>,
    sccs: Vec<Vec<NodeId>>,
}

/// Tarjan's strongly connected components algorithm.
///
/// Returns SCCs with size >= 1 that represent actual cycles (multi-node SCCs
/// or single-node self-loops).
fn tarjan_scc(adj: &BTreeMap<NodeId, Vec<NodeId>>) -> Vec<Vec<NodeId>> {
    let mut state = TarjanState {
        index_counter: 0,
        stack: Vec::new(),
        on_stack: HashSet::new(),
        index: HashMap::new(),
        lowlink: HashMap::new(),
        sccs: Vec::new(),
    };

    // Process all nodes in deterministic order (BTreeMap gives us this)
    let nodes: Vec<NodeId> = adj.keys().cloned().collect();
    for node in &nodes {
        if !state.index.contains_key(node) {
            strongconnect(node, adj, &mut state);
        }
    }

    state.sccs
}

fn strongconnect(v: &NodeId, adj: &BTreeMap<NodeId, Vec<NodeId>>, state: &mut TarjanState) {
    let v_index = state.index_counter;
    state.index.insert(v.clone(), v_index);
    state.lowlink.insert(v.clone(), v_index);
    state.index_counter += 1;
    state.stack.push(v.clone());
    state.on_stack.insert(v.clone());

    if let Some(neighbors) = adj.get(v) {
        for w in neighbors {
            if !state.index.contains_key(w) {
                strongconnect(w, adj, state);
                let w_low = state.lowlink[w];
                let v_low = state.lowlink.get_mut(v).unwrap();
                if w_low < *v_low {
                    *v_low = w_low;
                }
            } else if state.on_stack.contains(w) {
                let w_idx = state.index[w];
                let v_low = state.lowlink.get_mut(v).unwrap();
                if w_idx < *v_low {
                    *v_low = w_idx;
                }
            }
        }
    }

    // If v is a root node, pop the SCC
    if state.lowlink[v] == state.index[v] {
        let mut scc = Vec::new();
        loop {
            let w = state.stack.pop().unwrap();
            state.on_stack.remove(&w);
            scc.push(w.clone());
            if w == *v {
                break;
            }
        }
        // Reverse so the root comes first
        scc.reverse();
        state.sccs.push(scc);
    }
}

// ── Representative cycle extraction ─────────────────────────────

/// Find a representative cycle through an SCC using DFS.
/// Returns a path that forms a cycle (first node = last node).
fn find_representative_cycle(
    scc: &BTreeSet<NodeId>,
    adj: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Vec<NodeId> {
    if scc.is_empty() {
        return vec![];
    }

    let start = scc.iter().next().unwrap();

    // DFS from start, staying within the SCC, looking for a path back to start
    let mut visited: HashSet<&NodeId> = HashSet::new();
    let mut path: Vec<NodeId> = vec![start.clone()];
    visited.insert(start);

    if dfs_find_cycle(start, start, adj, scc, &mut visited, &mut path) {
        path.push(start.clone()); // close the cycle
        return path;
    }

    // Fallback: just list the SCC nodes (shouldn't happen for a real SCC)
    let mut fallback: Vec<NodeId> = scc.iter().cloned().collect();
    if let Some(first) = fallback.first().cloned() {
        fallback.push(first);
    }
    fallback
}

fn dfs_find_cycle<'a>(
    current: &NodeId,
    target: &NodeId,
    adj: &BTreeMap<NodeId, Vec<NodeId>>,
    scc: &'a BTreeSet<NodeId>,
    visited: &mut HashSet<&'a NodeId>,
    path: &mut Vec<NodeId>,
) -> bool {
    if let Some(neighbors) = adj.get(current) {
        for next in neighbors {
            if !scc.contains(next) {
                continue;
            }
            if next == target && path.len() > 1 {
                // Found cycle back to start
                return true;
            }
            // SAFETY: we need to borrow `next` from `neighbors` which borrows from `adj`,
            // but visited holds references to `scc` nodes. We use a contains check instead.
            if visited.contains(next) {
                continue;
            }

            // We need the reference to live as long as `scc`, but `next` is from `adj`.
            // Since we only check membership, find the reference in scc.
            let scc_ref = scc.get(next).unwrap();
            visited.insert(scc_ref);
            path.push(next.clone());

            if dfs_find_cycle(next, target, adj, scc, visited, path) {
                return true;
            }

            path.pop();
            visited.remove(scc_ref);
        }
    }
    false
}

// ── Severity scoring ────────────────────────────────────────────

fn compute_severity(
    graph: &WaitGraph,
    scc_nodes: &BTreeSet<NodeId>,
    scc_edges: &[WaitEdge],
) -> (u32, Vec<String>) {
    let mut score: u32 = 0;
    let mut rationale: Vec<String> = Vec::new();

    // Base score: cycle exists at all
    score += 10;
    rationale.push("blocking cycle detected".to_string());

    // Worst wait age among tasks in the SCC
    let mut worst_age: f64 = 0.0;
    let mut task_count: usize = 0;
    let mut has_cross_process = false;
    let mut pids: BTreeSet<u32> = BTreeSet::new();

    for node_id in scc_nodes {
        match node_id {
            NodeId::Task { pid, .. } => {
                pids.insert(*pid);
                if let Some(NodeKind::Task { age_secs, .. }) = graph.nodes.get(node_id) {
                    task_count += 1;
                    if *age_secs > worst_age {
                        worst_age = *age_secs;
                    }
                }
            }
            NodeId::RpcRequest { pid, .. } => {
                pids.insert(*pid);
                if let Some(NodeKind::RpcRequest { elapsed_secs, .. }) = graph.nodes.get(node_id) {
                    if *elapsed_secs > worst_age {
                        worst_age = *elapsed_secs;
                    }
                }
            }
            NodeId::Process { pid } => {
                pids.insert(*pid);
            }
            NodeId::Lock { pid, .. }
            | NodeId::Future { pid, .. }
            | NodeId::MpscChannel { pid, .. }
            | NodeId::OneshotChannel { pid, .. }
            | NodeId::WatchChannel { pid, .. }
            | NodeId::OnceCell { pid, .. } => {
                pids.insert(*pid);
            }
        }
    }

    if pids.len() > 1 {
        has_cross_process = true;
    }

    // Wait age scoring
    if worst_age > 30.0 {
        score += 30;
        rationale.push(format!("worst wait age: {worst_age:.1}s (>30s)"));
    } else if worst_age > 10.0 {
        score += 20;
        rationale.push(format!("worst wait age: {worst_age:.1}s (>10s)"));
    } else if worst_age > 1.0 {
        score += 10;
        rationale.push(format!("worst wait age: {worst_age:.1}s (>1s)"));
    }

    // Blocked task count: count tasks in the graph that are waiting on any node
    // in this SCC but are themselves outside the SCC
    let blocked_outside = count_blocked_tasks(graph, scc_nodes);
    if blocked_outside > 10 {
        score += 20;
        rationale.push(format!("{blocked_outside} tasks blocked outside cycle"));
    } else if blocked_outside > 0 {
        score += blocked_outside as u32 * 2;
        rationale.push(format!("{blocked_outside} tasks blocked outside cycle"));
    }

    // Cross-process involvement
    if has_cross_process {
        score += 15;
        rationale.push(format!("spans {} processes", pids.len()));
    }

    // Edge severity hints
    let max_hint: u8 = scc_edges
        .iter()
        .map(|e| e.meta.severity_hint)
        .max()
        .unwrap_or(0);
    if max_hint >= 3 {
        score += 10;
        rationale.push("high-severity edges in cycle".to_string());
    }

    // Cycle size bonus (larger cycles are harder to debug)
    if task_count > 4 {
        score += 5;
        rationale.push(format!("{task_count} tasks in cycle"));
    }

    (score, rationale)
}

/// Count tasks outside the SCC that are directly blocked by nodes in the SCC.
fn count_blocked_tasks(graph: &WaitGraph, scc_nodes: &BTreeSet<NodeId>) -> usize {
    let mut blocked: BTreeSet<&NodeId> = BTreeSet::new();

    for edge in &graph.edges {
        if edge.kind == EdgeKind::TaskWaitsOnResource && scc_nodes.contains(&edge.to) {
            if !scc_nodes.contains(&edge.from) {
                if matches!(edge.from, NodeId::Task { .. }) {
                    blocked.insert(&edge.from);
                }
            }
        }
    }

    blocked.len()
}

fn classify(score: u32, config: &SeverityConfig) -> Severity {
    if score >= config.danger_threshold {
        Severity::Danger
    } else if score >= config.warn_threshold {
        Severity::Warn
    } else {
        Severity::Info
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EdgeMeta, NodeKind, SnapshotSource, WaitEdge};
    use peeps_types::TaskState;

    /// Helper: build a graph from nodes and edges directly.
    fn make_graph(
        nodes: Vec<(NodeId, NodeKind)>,
        edges: Vec<WaitEdge>,
    ) -> WaitGraph {
        WaitGraph {
            nodes: nodes.into_iter().collect(),
            edges,
        }
    }

    fn task_node(pid: u32, id: u64) -> NodeId {
        NodeId::Task { pid, task_id: id }
    }

    fn lock_node(pid: u32, name: &str) -> NodeId {
        NodeId::Lock {
            pid,
            name: name.to_string(),
        }
    }

    fn task_kind(name: &str, age: f64) -> NodeKind {
        NodeKind::Task {
            name: name.to_string(),
            state: TaskState::Pending,
            age_secs: age,
        }
    }

    fn lock_kind(name: &str) -> NodeKind {
        NodeKind::Lock {
            name: name.to_string(),
            acquires: 10,
            releases: 9,
        }
    }

    fn blocking_edge(from: NodeId, to: NodeId, kind: EdgeKind) -> WaitEdge {
        WaitEdge {
            from,
            to,
            kind,
            meta: EdgeMeta {
                source_snapshot: SnapshotSource::Locks,
                count: 1,
                severity_hint: 1,
            },
        }
    }

    #[test]
    fn no_cycles_in_dag() {
        // A -> lock -> B, no cycle
        let graph = make_graph(
            vec![
                (task_node(1, 1), task_kind("a", 1.0)),
                (lock_node(1, "m"), lock_kind("m")),
                (task_node(1, 2), task_kind("b", 1.0)),
            ],
            vec![
                blocking_edge(task_node(1, 1), lock_node(1, "m"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "m"), task_node(1, 2), EdgeKind::ResourceOwnedByTask),
            ],
        );

        let candidates = find_deadlock_candidates(&graph);
        assert!(candidates.is_empty());
    }

    #[test]
    fn simple_two_task_deadlock() {
        // Classic: Task A holds Lock X, waits on Lock Y
        //          Task B holds Lock Y, waits on Lock X
        //
        // Edges:  A -> waits -> LX -> owned -> B -> waits -> LY -> owned -> A
        let graph = make_graph(
            vec![
                (task_node(1, 1), task_kind("task-a", 5.0)),
                (task_node(1, 2), task_kind("task-b", 5.0)),
                (lock_node(1, "lock-x"), lock_kind("lock-x")),
                (lock_node(1, "lock-y"), lock_kind("lock-y")),
            ],
            vec![
                blocking_edge(task_node(1, 1), lock_node(1, "lock-y"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "lock-y"), task_node(1, 2), EdgeKind::ResourceOwnedByTask),
                blocking_edge(task_node(1, 2), lock_node(1, "lock-x"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "lock-x"), task_node(1, 1), EdgeKind::ResourceOwnedByTask),
            ],
        );

        let candidates = find_deadlock_candidates(&graph);
        assert_eq!(candidates.len(), 1);

        let c = &candidates[0];
        assert_eq!(c.nodes.len(), 4); // 2 tasks + 2 locks
        assert!(c.severity_score >= 20); // at least warn level
        assert!(!c.cycle_path.is_empty());
        // Cycle path should close (first == last)
        assert_eq!(c.cycle_path.first(), c.cycle_path.last());
    }

    #[test]
    fn self_loop_detected() {
        // A task that waits on a resource it itself owns (bizarre but possible)
        let graph = make_graph(
            vec![
                (task_node(1, 1), task_kind("self-blocker", 10.0)),
                (lock_node(1, "m"), lock_kind("m")),
            ],
            vec![
                blocking_edge(task_node(1, 1), lock_node(1, "m"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "m"), task_node(1, 1), EdgeKind::ResourceOwnedByTask),
            ],
        );

        let candidates = find_deadlock_candidates(&graph);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].nodes.len(), 2); // task + lock
    }

    #[test]
    fn cross_process_rpc_cycle() {
        // Process 1 task -> RPC request -> Process 2 task -> RPC request -> Process 1 task
        let rpc1 = NodeId::RpcRequest {
            pid: 1,
            connection: "conn".to_string(),
            request_id: 1,
        };
        let rpc2 = NodeId::RpcRequest {
            pid: 2,
            connection: "conn".to_string(),
            request_id: 2,
        };

        let graph = make_graph(
            vec![
                (task_node(1, 1), task_kind("p1-task", 20.0)),
                (task_node(2, 1), task_kind("p2-task", 20.0)),
                (
                    rpc1.clone(),
                    NodeKind::RpcRequest {
                        method_name: Some("call_p2".to_string()),
                        direction: peeps_types::Direction::Outgoing,
                        elapsed_secs: 20.0,
                    },
                ),
                (
                    rpc2.clone(),
                    NodeKind::RpcRequest {
                        method_name: Some("call_p1".to_string()),
                        direction: peeps_types::Direction::Outgoing,
                        elapsed_secs: 20.0,
                    },
                ),
            ],
            vec![
                blocking_edge(task_node(1, 1), rpc1.clone(), EdgeKind::RpcClientToRequest),
                blocking_edge(rpc1, task_node(2, 1), EdgeKind::RpcRequestToServerTask),
                blocking_edge(task_node(2, 1), rpc2.clone(), EdgeKind::RpcClientToRequest),
                blocking_edge(rpc2, task_node(1, 1), EdgeKind::RpcRequestToServerTask),
            ],
        );

        let candidates = find_deadlock_candidates(&graph);
        assert_eq!(candidates.len(), 1);

        let c = &candidates[0];
        // Cross-process should boost the score
        assert!(c.rationale.iter().any(|r| r.contains("processes")));
        // base(10) + age>10s(20) + cross-process(15) = 45 minimum
        assert!(c.severity_score >= 45);
        assert!(c.severity >= Severity::Warn);
    }

    #[test]
    fn multiple_independent_cycles() {
        // Two separate deadlock cycles
        let graph = make_graph(
            vec![
                // Cycle 1
                (task_node(1, 1), task_kind("a1", 2.0)),
                (task_node(1, 2), task_kind("a2", 2.0)),
                (lock_node(1, "m1"), lock_kind("m1")),
                (lock_node(1, "m2"), lock_kind("m2")),
                // Cycle 2
                (task_node(1, 3), task_kind("b1", 30.0)),
                (task_node(1, 4), task_kind("b2", 30.0)),
                (lock_node(1, "n1"), lock_kind("n1")),
                (lock_node(1, "n2"), lock_kind("n2")),
            ],
            vec![
                // Cycle 1: a1 -> m2 -> a2 -> m1 -> a1
                blocking_edge(task_node(1, 1), lock_node(1, "m2"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "m2"), task_node(1, 2), EdgeKind::ResourceOwnedByTask),
                blocking_edge(task_node(1, 2), lock_node(1, "m1"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "m1"), task_node(1, 1), EdgeKind::ResourceOwnedByTask),
                // Cycle 2: b1 -> n2 -> b2 -> n1 -> b1
                blocking_edge(task_node(1, 3), lock_node(1, "n2"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "n2"), task_node(1, 4), EdgeKind::ResourceOwnedByTask),
                blocking_edge(task_node(1, 4), lock_node(1, "n1"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "n1"), task_node(1, 3), EdgeKind::ResourceOwnedByTask),
            ],
        );

        let candidates = find_deadlock_candidates(&graph);
        assert_eq!(candidates.len(), 2);
        // Cycle 2 has higher age (30s) so should rank first
        assert!(candidates[0].severity_score >= candidates[1].severity_score);
        assert!(candidates[0]
            .rationale
            .iter()
            .any(|r| r.contains("30.0")));
    }

    #[test]
    fn blocked_tasks_outside_cycle_boost_score() {
        // Cycle: task1 -> lock -> task2 -> lock2 -> task1
        // External: task3 waits on lock (inside the cycle)
        let graph = make_graph(
            vec![
                (task_node(1, 1), task_kind("t1", 5.0)),
                (task_node(1, 2), task_kind("t2", 5.0)),
                (task_node(1, 3), task_kind("t3-blocked", 5.0)),
                (lock_node(1, "la"), lock_kind("la")),
                (lock_node(1, "lb"), lock_kind("lb")),
            ],
            vec![
                // Cycle
                blocking_edge(task_node(1, 1), lock_node(1, "lb"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "lb"), task_node(1, 2), EdgeKind::ResourceOwnedByTask),
                blocking_edge(task_node(1, 2), lock_node(1, "la"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "la"), task_node(1, 1), EdgeKind::ResourceOwnedByTask),
                // External task blocked by lock in cycle
                blocking_edge(task_node(1, 3), lock_node(1, "la"), EdgeKind::TaskWaitsOnResource),
            ],
        );

        let candidates = find_deadlock_candidates(&graph);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0]
            .rationale
            .iter()
            .any(|r| r.contains("blocked outside")));
    }

    #[test]
    fn severity_config_thresholds() {
        let graph = make_graph(
            vec![
                (task_node(1, 1), task_kind("a", 0.5)),
                (lock_node(1, "m"), lock_kind("m")),
            ],
            vec![
                blocking_edge(task_node(1, 1), lock_node(1, "m"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "m"), task_node(1, 1), EdgeKind::ResourceOwnedByTask),
            ],
        );

        // With very low thresholds, even a mild cycle is danger
        let strict = SeverityConfig {
            danger_threshold: 5,
            warn_threshold: 2,
        };
        let candidates = find_deadlock_candidates_with_config(&graph, &strict);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].severity, Severity::Danger);

        // With very high thresholds, same cycle is just info
        let lenient = SeverityConfig {
            danger_threshold: 200,
            warn_threshold: 100,
        };
        let candidates = find_deadlock_candidates_with_config(&graph, &lenient);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].severity, Severity::Info);
    }

    #[test]
    fn non_blocking_edges_ignored() {
        // TaskSpawnedTask and TaskWakesFuture should not form deadlock cycles
        let graph = make_graph(
            vec![
                (task_node(1, 1), task_kind("parent", 1.0)),
                (task_node(1, 2), task_kind("child", 1.0)),
            ],
            vec![
                WaitEdge {
                    from: task_node(1, 1),
                    to: task_node(1, 2),
                    kind: EdgeKind::TaskSpawnedTask,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::Tasks,
                        count: 1,
                        severity_hint: 0,
                    },
                },
                WaitEdge {
                    from: task_node(1, 2),
                    to: task_node(1, 1),
                    kind: EdgeKind::TaskWakesFuture,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::WakeEdges,
                        count: 1,
                        severity_hint: 0,
                    },
                },
            ],
        );

        let candidates = find_deadlock_candidates(&graph);
        assert!(candidates.is_empty());
    }

    #[test]
    fn empty_graph_no_candidates() {
        let graph = WaitGraph {
            nodes: BTreeMap::new(),
            edges: vec![],
        };
        let candidates = find_deadlock_candidates(&graph);
        assert!(candidates.is_empty());
    }

    #[test]
    fn cycle_path_is_valid() {
        let graph = make_graph(
            vec![
                (task_node(1, 1), task_kind("a", 1.0)),
                (task_node(1, 2), task_kind("b", 1.0)),
                (lock_node(1, "x"), lock_kind("x")),
                (lock_node(1, "y"), lock_kind("y")),
            ],
            vec![
                blocking_edge(task_node(1, 1), lock_node(1, "y"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "y"), task_node(1, 2), EdgeKind::ResourceOwnedByTask),
                blocking_edge(task_node(1, 2), lock_node(1, "x"), EdgeKind::TaskWaitsOnResource),
                blocking_edge(lock_node(1, "x"), task_node(1, 1), EdgeKind::ResourceOwnedByTask),
            ],
        );

        let candidates = find_deadlock_candidates(&graph);
        assert_eq!(candidates.len(), 1);

        let path = &candidates[0].cycle_path;
        // Path should close
        assert!(path.len() >= 3); // at least 2 nodes + closing node
        assert_eq!(path.first(), path.last());
        // All nodes in path should be in the SCC
        for node in path {
            assert!(candidates[0].nodes.contains(node));
        }
    }
}
