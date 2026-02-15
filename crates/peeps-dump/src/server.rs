use std::collections::HashMap;
use std::sync::Arc;

use peeps_types::{DashboardPayload, ProcessDump};
use peeps_waitgraph::detect::{self, Severity};
use peeps_waitgraph::{EdgeKind, NodeId, NodeKind, WaitGraph};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};

/// Key for identifying a process: (process_name, pid).
type ProcessKey = (String, u32);

/// Shared dashboard state, holding the latest dump from each connected process.
pub struct DashboardState {
    dumps: Mutex<HashMap<ProcessKey, ProcessDump>>,
    notify: broadcast::Sender<()>,
}

impl DashboardState {
    pub fn new() -> Self {
        let (notify, _) = broadcast::channel(16);
        Self {
            dumps: Mutex::new(HashMap::new()),
            notify,
        }
    }

    /// Insert or update a dump. Notifies subscribers.
    pub async fn upsert_dump(&self, dump: ProcessDump) {
        let key = (dump.process_name.clone(), dump.pid);
        self.dumps.lock().await.insert(key, dump);
        let _ = self.notify.send(());
    }

    /// Get all current dumps as a sorted vec.
    pub async fn all_dumps(&self) -> Vec<ProcessDump> {
        let map = self.dumps.lock().await;
        let mut dumps: Vec<ProcessDump> = map.values().cloned().collect();
        dumps.sort_by(|a, b| a.process_name.cmp(&b.process_name));
        dumps
    }

    /// Build the full dashboard payload with dumps and deadlock candidates.
    pub async fn dashboard_payload(&self) -> DashboardPayload {
        let dumps = self.all_dumps().await;
        let graph = WaitGraph::build(&dumps);
        let raw_candidates = detect::find_deadlock_candidates(&graph);
        let deadlock_candidates = raw_candidates
            .into_iter()
            .enumerate()
            .map(|(i, c)| convert_candidate(i as u32, &c, &graph, &dumps))
            .collect();
        DashboardPayload {
            dumps,
            deadlock_candidates,
        }
    }

    /// Subscribe to change notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.notify.subscribe()
    }
}

// ── Candidate conversion ─────────────────────────────────────────

fn convert_candidate(
    id: u32,
    candidate: &detect::DeadlockCandidate,
    graph: &WaitGraph,
    dumps: &[ProcessDump],
) -> peeps_types::DeadlockCandidate {
    let pid_to_name: HashMap<u32, &str> = dumps
        .iter()
        .map(|d| (d.pid, d.process_name.as_str()))
        .collect();

    // Build cycle_path nodes from the candidate's cycle_path (which closes: first == last).
    // We skip the closing duplicate node.
    let path_nodes: Vec<&NodeId> = if candidate.cycle_path.len() > 1 {
        candidate.cycle_path[..candidate.cycle_path.len() - 1]
            .iter()
            .collect()
    } else {
        candidate.cycle_path.iter().collect()
    };

    let cycle_path: Vec<peeps_types::CycleNode> = path_nodes
        .iter()
        .map(|node_id| node_id_to_cycle_node(node_id, graph, &pid_to_name))
        .collect();

    // Build edges between consecutive path nodes (and wrap around)
    let cycle_edges: Vec<peeps_types::CycleEdge> = if path_nodes.len() >= 2 {
        (0..path_nodes.len())
            .map(|i| {
                let from = i as u32;
                let to = ((i + 1) % path_nodes.len()) as u32;
                let from_id = path_nodes[i];
                let to_id = path_nodes[(i + 1) % path_nodes.len()];
                let (explanation, wait_secs) =
                    edge_explanation(from_id, to_id, graph, &cycle_path);
                peeps_types::CycleEdge {
                    from_node: from,
                    to_node: to,
                    explanation,
                    wait_secs,
                }
            })
            .collect()
    } else {
        vec![]
    };

    let severity = match candidate.severity {
        Severity::Danger => peeps_types::DeadlockSeverity::Danger,
        Severity::Warn | Severity::Info => peeps_types::DeadlockSeverity::Warn,
    };

    let cross_process = {
        let mut pids = std::collections::BTreeSet::new();
        for node in &candidate.cycle_path {
            if let Some(pid) = node_pid(node) {
                pids.insert(pid);
            }
        }
        pids.len() > 1
    };

    let worst_wait_secs = candidate
        .edges
        .iter()
        .filter_map(|e| {
            match graph.nodes.get(&e.from) {
                Some(NodeKind::Task { age_secs, .. }) => Some(*age_secs),
                Some(NodeKind::RpcRequest { elapsed_secs, .. }) => Some(*elapsed_secs),
                _ => None,
            }
        })
        .fold(0.0_f64, f64::max);

    let title = build_title(&cycle_path, cross_process);

    peeps_types::DeadlockCandidate {
        id,
        severity,
        score: candidate.severity_score as f64,
        title,
        cycle_path,
        cycle_edges,
        rationale: candidate.rationale.clone(),
        cross_process,
        worst_wait_secs,
        blocked_task_count: count_blocked_outside(candidate, graph),
    }
}

fn node_pid(node: &NodeId) -> Option<u32> {
    match node {
        NodeId::Task { pid, .. }
        | NodeId::Future { pid, .. }
        | NodeId::Lock { pid, .. }
        | NodeId::MpscChannel { pid, .. }
        | NodeId::OneshotChannel { pid, .. }
        | NodeId::WatchChannel { pid, .. }
        | NodeId::OnceCell { pid, .. }
        | NodeId::RpcRequest { pid, .. }
        | NodeId::Process { pid } => Some(*pid),
    }
}

fn node_id_to_cycle_node(
    node_id: &NodeId,
    graph: &WaitGraph,
    pid_to_name: &HashMap<u32, &str>,
) -> peeps_types::CycleNode {
    let pid = node_pid(node_id).unwrap_or(0);
    let process = pid_to_name
        .get(&pid)
        .unwrap_or(&"unknown")
        .to_string();

    match graph.nodes.get(node_id) {
        Some(NodeKind::Task {
            name, state: _, age_secs: _,
        }) => peeps_types::CycleNode {
            label: name.clone(),
            kind: "task".to_string(),
            process,
            task_id: match node_id {
                NodeId::Task { task_id, .. } => Some(*task_id),
                _ => None,
            },
        },
        Some(NodeKind::Lock { name, .. }) => peeps_types::CycleNode {
            label: name.clone(),
            kind: "lock".to_string(),
            process,
            task_id: None,
        },
        Some(NodeKind::RpcRequest {
            method_name,
            direction: _,
            elapsed_secs: _,
        }) => peeps_types::CycleNode {
            label: method_name
                .clone()
                .unwrap_or_else(|| "rpc".to_string()),
            kind: "rpc".to_string(),
            process,
            task_id: None,
        },
        Some(NodeKind::MpscChannel { name, .. }) => peeps_types::CycleNode {
            label: name.clone(),
            kind: "channel".to_string(),
            process,
            task_id: None,
        },
        Some(NodeKind::OneshotChannel { name, .. }) => peeps_types::CycleNode {
            label: name.clone(),
            kind: "channel".to_string(),
            process,
            task_id: None,
        },
        Some(NodeKind::WatchChannel { name, .. }) => peeps_types::CycleNode {
            label: name.clone(),
            kind: "channel".to_string(),
            process,
            task_id: None,
        },
        Some(NodeKind::Future { resource }) => peeps_types::CycleNode {
            label: resource.clone(),
            kind: "future".to_string(),
            process,
            task_id: None,
        },
        Some(NodeKind::OnceCell { name, .. }) => peeps_types::CycleNode {
            label: name.clone(),
            kind: "oncecell".to_string(),
            process,
            task_id: None,
        },
        Some(NodeKind::Process { name, .. }) => peeps_types::CycleNode {
            label: name.clone(),
            kind: "process".to_string(),
            process,
            task_id: None,
        },
        None => peeps_types::CycleNode {
            label: "unknown".to_string(),
            kind: "unknown".to_string(),
            process,
            task_id: None,
        },
    }
}

fn edge_explanation(
    from: &NodeId,
    to: &NodeId,
    graph: &WaitGraph,
    cycle_nodes: &[peeps_types::CycleNode],
) -> (String, f64) {
    // Find the matching edge in the graph
    for edge in &graph.edges {
        if edge.from == *from && edge.to == *to {
            let from_label = node_label(from, graph);
            let to_label = node_label(to, graph);
            let wait_secs = match &edge.kind {
                EdgeKind::TaskWaitsOnResource => match graph.nodes.get(from) {
                    Some(NodeKind::Task { age_secs, .. }) => *age_secs,
                    _ => 0.0,
                },
                EdgeKind::RpcClientToRequest => match graph.nodes.get(to) {
                    Some(NodeKind::RpcRequest { elapsed_secs, .. }) => *elapsed_secs,
                    _ => 0.0,
                },
                _ => 0.0,
            };
            let explanation = match &edge.kind {
                EdgeKind::TaskWaitsOnResource => {
                    format!("{from_label} waits on {to_label}")
                }
                EdgeKind::ResourceOwnedByTask => {
                    format!("{from_label} held by {to_label}")
                }
                EdgeKind::RpcClientToRequest => {
                    format!("{from_label} waiting on RPC {to_label}")
                }
                EdgeKind::RpcRequestToServerTask => {
                    format!("RPC {from_label} handled by {to_label}")
                }
                _ => format!("{from_label} -> {to_label}"),
            };
            return (explanation, wait_secs);
        }
    }
    let _ = cycle_nodes; // used for context in the signature
    ("unknown relationship".to_string(), 0.0)
}

fn node_label(node: &NodeId, graph: &WaitGraph) -> String {
    match graph.nodes.get(node) {
        Some(NodeKind::Task { name, .. }) => format!("task \"{name}\""),
        Some(NodeKind::Lock { name, .. }) => format!("lock \"{name}\""),
        Some(NodeKind::RpcRequest { method_name, .. }) => {
            format!("RPC \"{}\"", method_name.as_deref().unwrap_or("unknown"))
        }
        Some(NodeKind::MpscChannel { name, .. }) => format!("channel \"{name}\""),
        Some(NodeKind::OneshotChannel { name, .. }) => format!("oneshot \"{name}\""),
        Some(NodeKind::WatchChannel { name, .. }) => format!("watch \"{name}\""),
        Some(NodeKind::Future { resource }) => format!("future \"{resource}\""),
        _ => "unknown".to_string(),
    }
}

fn build_title(cycle_nodes: &[peeps_types::CycleNode], cross_process: bool) -> String {
    let task_names: Vec<&str> = cycle_nodes
        .iter()
        .filter(|n| n.kind == "task")
        .map(|n| n.label.as_str())
        .collect();

    let prefix = if cross_process {
        "Cross-process deadlock"
    } else {
        "Deadlock"
    };

    match task_names.len() {
        0 => format!("{prefix} involving {} nodes", cycle_nodes.len()),
        1 => format!("{prefix}: {}", task_names[0]),
        2 => format!("{prefix}: {} <-> {}", task_names[0], task_names[1]),
        n => format!(
            "{prefix}: {}, {}, and {} more",
            task_names[0],
            task_names[1],
            n - 2
        ),
    }
}

fn count_blocked_outside(
    candidate: &detect::DeadlockCandidate,
    graph: &WaitGraph,
) -> u32 {
    let mut blocked = std::collections::BTreeSet::new();
    for edge in &graph.edges {
        if edge.kind == EdgeKind::TaskWaitsOnResource && candidate.nodes.contains(&edge.to) {
            if !candidate.nodes.contains(&edge.from) {
                if matches!(edge.from, NodeId::Task { .. }) {
                    blocked.insert(edge.from.clone());
                }
            }
        }
    }
    blocked.len() as u32
}

/// Accept TCP connections and spawn a reader task for each.
pub async fn run_tcp_acceptor(listener: TcpListener, state: Arc<DashboardState>) {
    let max_frame_bytes = max_frame_bytes_from_env();
    eprintln!("[peeps] max frame size set to {max_frame_bytes} bytes");
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                eprintln!("[peeps] TCP connection from {addr}");
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = handle_tcp_connection(stream, &state, max_frame_bytes).await {
                        eprintln!("[peeps] connection from {addr} closed: {e}");
                    } else {
                        eprintln!("[peeps] connection from {addr} closed");
                    }
                });
            }
            Err(e) => {
                eprintln!("[peeps] TCP accept error: {e}");
            }
        }
    }
}

/// Read length-prefixed JSON frames from a single TCP connection.
///
/// Wire format: `[u32 big-endian length][UTF-8 JSON ProcessDump]`
async fn handle_tcp_connection(
    mut stream: TcpStream,
    state: &DashboardState,
    max_frame_bytes: usize,
) -> std::io::Result<()> {
    loop {
        // Read 4-byte length prefix (big-endian u32).
        let len = stream.read_u32().await?;

        if len == 0 {
            continue;
        }

        // Sanity limit to avoid unbounded memory growth on malformed clients.
        if (len as usize) > max_frame_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("frame too large: {len} bytes (max {max_frame_bytes})"),
            ));
        }

        let mut buf = vec![0u8; len as usize];
        stream.read_exact(&mut buf).await?;

        let json = match std::str::from_utf8(&buf) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[peeps] invalid UTF-8 in frame: {e}");
                continue;
            }
        };

        match facet_json::from_str::<ProcessDump>(json) {
            Ok(dump) => {
                eprintln!(
                    "[peeps] dump from {} (pid {}): {} tasks, {} threads",
                    dump.process_name,
                    dump.pid,
                    dump.tasks.len(),
                    dump.threads.len()
                );
                state.upsert_dump(dump).await;
            }
            Err(e) => {
                eprintln!("[peeps] failed to parse dump frame: {e}");
            }
        }
    }
}

fn max_frame_bytes_from_env() -> usize {
    const DEFAULT_MAX_FRAME_BYTES: usize = 128 * 1024 * 1024;
    match std::env::var("PEEPS_MAX_FRAME_BYTES") {
        Ok(raw) => match raw.parse::<usize>() {
            Ok(v) if v > 0 => v,
            _ => {
                eprintln!(
                    "[peeps] invalid PEEPS_MAX_FRAME_BYTES={raw:?}, using default {}",
                    DEFAULT_MAX_FRAME_BYTES
                );
                DEFAULT_MAX_FRAME_BYTES
            }
        },
        Err(_) => DEFAULT_MAX_FRAME_BYTES,
    }
}
