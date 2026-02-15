//! Unified wait graph model and ingestion pipeline for peeps.
//!
//! Converts raw `ProcessDump` snapshots into a normalized directed graph
//! of blocking relationships. The graph is the single source of truth for
//! cycle detection, severity ranking, and dashboard explanations.

use std::collections::BTreeMap;

use facet::Facet;

pub mod detect;

use peeps_types::{
    ConnectionSnapshot, Direction, FutureWaitSnapshot, FutureWakeEdgeSnapshot, LockAcquireKind,
    LockInfoSnapshot, LockSnapshot, MpscChannelSnapshot, OnceCellSnapshot, OnceCellState,
    OneshotChannelSnapshot, OneshotState, ProcessDump, SessionSnapshot, SyncSnapshot,
    TaskSnapshot, TaskState, WakeEdgeSnapshot, WatchChannelSnapshot,
};

// ── Stable node identity ────────────────────────────────────────

/// Stable identifier for a graph node across snapshots.
///
/// Uses `BTreeMap`-friendly `Ord` so the graph has deterministic iteration order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
#[repr(u8)]
pub enum NodeId {
    /// A tokio task within a process.
    Task { pid: u32, task_id: u64 },
    /// An instrumented future within a process.
    Future { pid: u32, future_id: u64 },
    /// A named lock within a process.
    Lock { pid: u32, name: String },
    /// An mpsc channel within a process.
    MpscChannel { pid: u32, name: String },
    /// A oneshot channel within a process.
    OneshotChannel { pid: u32, name: String },
    /// A watch channel within a process.
    WatchChannel { pid: u32, name: String },
    /// A OnceCell within a process.
    OnceCell { pid: u32, name: String },
    /// An RPC request (connection + request id, scoped to process).
    RpcRequest {
        pid: u32,
        connection: String,
        request_id: u64,
    },
    /// A whole process.
    Process { pid: u32 },
}

// ── Node kinds ──────────────────────────────────────────────────

/// What kind of resource a node represents.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum NodeKind {
    Task {
        name: String,
        state: TaskState,
        age_secs: f64,
    },
    Future {
        resource: String,
    },
    Lock {
        name: String,
        acquires: u64,
        releases: u64,
    },
    MpscChannel {
        name: String,
        bounded: bool,
        capacity: Option<u64>,
        pending: u64,
    },
    OneshotChannel {
        name: String,
        state: OneshotState,
    },
    WatchChannel {
        name: String,
        changes: u64,
    },
    OnceCell {
        name: String,
        state: OnceCellState,
    },
    RpcRequest {
        method_name: Option<String>,
        direction: Direction,
        elapsed_secs: f64,
    },
    Process {
        name: String,
        pid: u32,
    },
}

// ── Edge kinds ──────────────────────────────────────────────────

/// The nature of a blocking/dependency relationship.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum EdgeKind {
    /// A task is waiting on a resource (lock, channel, future, RPC response).
    TaskWaitsOnResource,
    /// A resource is currently owned/held by a task.
    ResourceOwnedByTask,
    /// A task wakes a future (or another task through a future).
    TaskWakesFuture,
    /// A future, once ready, resumes a task.
    FutureResumesTask,
    /// A task is the client side of an outgoing RPC request.
    RpcClientToRequest,
    /// An incoming RPC request is being handled by a server-side task.
    RpcRequestToServerTask,
    /// Parent-child spawn relationship between tasks.
    TaskSpawnedTask,
    /// Cross-process RPC stitch: outgoing request in one process links to
    /// the corresponding incoming request in another process.
    RpcCrossProcessStitch,
}

/// Metadata attached to every edge.
#[derive(Debug, Clone, Facet)]
pub struct EdgeMeta {
    /// Which snapshot source produced this edge.
    pub source_snapshot: SnapshotSource,
    /// Number of times this relationship has been observed.
    pub count: u64,
    /// Optional severity hint for ranking (higher = more suspicious).
    pub severity_hint: u8,
}

/// Which snapshot source an edge was derived from.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum SnapshotSource {
    Tasks,
    WakeEdges,
    FutureWakeEdges,
    FutureWaits,
    Locks,
    Sync,
    Roam,
}

// ── Graph edge ──────────────────────────────────────────────────

/// A directed edge in the wait graph.
#[derive(Debug, Clone, Facet)]
pub struct WaitEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    pub meta: EdgeMeta,
}

// ── The graph itself ────────────────────────────────────────────

/// The normalized wait graph built from one or more process dumps.
#[derive(Debug, Clone)]
pub struct WaitGraph {
    pub nodes: BTreeMap<NodeId, NodeKind>,
    pub edges: Vec<WaitEdge>,
}

impl WaitGraph {
    /// Build a wait graph from one or more process dumps.
    pub fn build(dumps: &[ProcessDump]) -> Self {
        let mut graph = WaitGraph {
            nodes: BTreeMap::new(),
            edges: Vec::new(),
        };
        for dump in dumps {
            graph.ingest_dump(dump);
        }
        graph.stitch_cross_process_rpc();
        graph
    }

    fn ingest_dump(&mut self, dump: &ProcessDump) {
        let pid = dump.pid;

        // Process node
        self.nodes.insert(
            NodeId::Process { pid },
            NodeKind::Process {
                name: dump.process_name.clone(),
                pid,
            },
        );

        self.ingest_tasks(pid, &dump.tasks);
        self.ingest_wake_edges(pid, &dump.wake_edges);
        self.ingest_future_wake_edges(pid, &dump.future_wake_edges);
        self.ingest_future_waits(pid, &dump.future_waits);
        if let Some(ref locks) = dump.locks {
            self.ingest_locks(pid, locks);
        }
        if let Some(ref sync) = dump.sync {
            self.ingest_sync(pid, sync);
        }
        if let Some(ref roam) = dump.roam {
            self.ingest_roam(pid, roam);
        }
    }

    fn ingest_tasks(&mut self, pid: u32, tasks: &[TaskSnapshot]) {
        for task in tasks {
            let node_id = NodeId::Task {
                pid,
                task_id: task.id,
            };
            self.nodes.insert(
                node_id.clone(),
                NodeKind::Task {
                    name: task.name.clone(),
                    state: task.state,
                    age_secs: task.age_secs,
                },
            );

            // Parent-child spawn edge
            if let Some(parent_id) = task.parent_task_id {
                let parent_node = NodeId::Task {
                    pid,
                    task_id: parent_id,
                };
                self.edges.push(WaitEdge {
                    from: parent_node,
                    to: node_id,
                    kind: EdgeKind::TaskSpawnedTask,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::Tasks,
                        count: 1,
                        severity_hint: 0,
                    },
                });
            }
        }
    }

    fn ingest_wake_edges(&mut self, pid: u32, wake_edges: &[WakeEdgeSnapshot]) {
        for edge in wake_edges {
            let target = NodeId::Task {
                pid,
                task_id: edge.target_task_id,
            };
            if let Some(source_id) = edge.source_task_id {
                let source = NodeId::Task {
                    pid,
                    task_id: source_id,
                };
                // source task wakes target task — model as: target was waiting,
                // source provides the wake. This is a "future resumes task" in
                // the abstract (the waker fires on the target).
                self.edges.push(WaitEdge {
                    from: source.clone(),
                    to: target.clone(),
                    kind: EdgeKind::TaskWakesFuture,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::WakeEdges,
                        count: edge.wake_count,
                        severity_hint: 0,
                    },
                });
            }
        }
    }

    fn ingest_future_wake_edges(&mut self, pid: u32, edges: &[FutureWakeEdgeSnapshot]) {
        for edge in edges {
            let future_node = NodeId::Future {
                pid,
                future_id: edge.future_id,
            };

            // Ensure future node exists
            self.nodes
                .entry(future_node.clone())
                .or_insert(NodeKind::Future {
                    resource: edge.future_resource.clone(),
                });

            // source task -> wakes future
            if let Some(source_id) = edge.source_task_id {
                let source = NodeId::Task {
                    pid,
                    task_id: source_id,
                };
                self.edges.push(WaitEdge {
                    from: source,
                    to: future_node.clone(),
                    kind: EdgeKind::TaskWakesFuture,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::FutureWakeEdges,
                        count: edge.wake_count,
                        severity_hint: 0,
                    },
                });
            }

            // future -> resumes target task
            if let Some(target_id) = edge.target_task_id {
                let target = NodeId::Task {
                    pid,
                    task_id: target_id,
                };
                self.edges.push(WaitEdge {
                    from: future_node,
                    to: target,
                    kind: EdgeKind::FutureResumesTask,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::FutureWakeEdges,
                        count: edge.wake_count,
                        severity_hint: 0,
                    },
                });
            }
        }
    }

    fn ingest_future_waits(&mut self, pid: u32, waits: &[FutureWaitSnapshot]) {
        for wait in waits {
            let future_node = NodeId::Future {
                pid,
                future_id: wait.future_id,
            };

            self.nodes
                .entry(future_node.clone())
                .or_insert(NodeKind::Future {
                    resource: wait.resource.clone(),
                });

            // task -> waits on future
            let task_node = NodeId::Task {
                pid,
                task_id: wait.task_id,
            };
            let severity = if wait.pending_count > 0 && wait.ready_count == 0 {
                2 // never been ready — suspicious
            } else {
                0
            };
            self.edges.push(WaitEdge {
                from: task_node,
                to: future_node.clone(),
                kind: EdgeKind::TaskWaitsOnResource,
                meta: EdgeMeta {
                    source_snapshot: SnapshotSource::FutureWaits,
                    count: wait.pending_count + wait.ready_count,
                    severity_hint: severity,
                },
            });

            // future -> created by task (ownership)
            if let Some(creator_id) = wait.created_by_task_id {
                let creator = NodeId::Task {
                    pid,
                    task_id: creator_id,
                };
                self.edges.push(WaitEdge {
                    from: future_node,
                    to: creator,
                    kind: EdgeKind::ResourceOwnedByTask,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::FutureWaits,
                        count: 1,
                        severity_hint: 0,
                    },
                });
            }
        }
    }

    fn ingest_locks(&mut self, pid: u32, lock_snap: &LockSnapshot) {
        for lock in &lock_snap.locks {
            self.ingest_single_lock(pid, lock);
        }
    }

    fn ingest_single_lock(&mut self, pid: u32, lock: &LockInfoSnapshot) {
        let lock_node = NodeId::Lock {
            pid,
            name: lock.name.clone(),
        };
        self.nodes.insert(
            lock_node.clone(),
            NodeKind::Lock {
                name: lock.name.clone(),
                acquires: lock.acquires,
                releases: lock.releases,
            },
        );

        // lock -> owned by holder task
        for holder in &lock.holders {
            if let Some(task_id) = holder.task_id {
                let task_node = NodeId::Task { pid, task_id };
                let severity = match holder.kind {
                    LockAcquireKind::Write | LockAcquireKind::Mutex => {
                        if holder.held_secs > 1.0 {
                            3
                        } else {
                            1
                        }
                    }
                    LockAcquireKind::Read => 0,
                };
                self.edges.push(WaitEdge {
                    from: lock_node.clone(),
                    to: task_node,
                    kind: EdgeKind::ResourceOwnedByTask,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::Locks,
                        count: 1,
                        severity_hint: severity,
                    },
                });
            }
        }

        // waiter task -> waits on lock
        for waiter in &lock.waiters {
            if let Some(task_id) = waiter.task_id {
                let task_node = NodeId::Task { pid, task_id };
                let severity = if waiter.waiting_secs > 1.0 { 3 } else { 1 };
                self.edges.push(WaitEdge {
                    from: task_node,
                    to: lock_node.clone(),
                    kind: EdgeKind::TaskWaitsOnResource,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::Locks,
                        count: 1,
                        severity_hint: severity,
                    },
                });
            }
        }
    }

    fn ingest_sync(&mut self, pid: u32, sync: &SyncSnapshot) {
        for ch in &sync.mpsc_channels {
            self.ingest_mpsc(pid, ch);
        }
        for ch in &sync.oneshot_channels {
            self.ingest_oneshot(pid, ch);
        }
        for ch in &sync.watch_channels {
            self.ingest_watch(pid, ch);
        }
        for cell in &sync.once_cells {
            self.ingest_once_cell(pid, cell);
        }
    }

    fn ingest_mpsc(&mut self, pid: u32, ch: &MpscChannelSnapshot) {
        let node_id = NodeId::MpscChannel {
            pid,
            name: ch.name.clone(),
        };
        let pending = ch.sent.saturating_sub(ch.received);
        self.nodes.insert(
            node_id.clone(),
            NodeKind::MpscChannel {
                name: ch.name.clone(),
                bounded: ch.bounded,
                capacity: ch.capacity,
                pending,
            },
        );

        // channel -> owned by creator task
        if let Some(creator_id) = ch.creator_task_id {
            let creator = NodeId::Task {
                pid,
                task_id: creator_id,
            };
            self.edges.push(WaitEdge {
                from: node_id.clone(),
                to: creator,
                kind: EdgeKind::ResourceOwnedByTask,
                meta: EdgeMeta {
                    source_snapshot: SnapshotSource::Sync,
                    count: 1,
                    severity_hint: 0,
                },
            });
        }

        // If senders are blocked, the channel is a bottleneck
        if ch.send_waiters > 0 {
            if let Some(creator_id) = ch.creator_task_id {
                let creator = NodeId::Task {
                    pid,
                    task_id: creator_id,
                };
                self.edges.push(WaitEdge {
                    from: creator,
                    to: node_id,
                    kind: EdgeKind::TaskWaitsOnResource,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::Sync,
                        count: ch.send_waiters,
                        severity_hint: 2,
                    },
                });
            }
        }
    }

    fn ingest_oneshot(&mut self, pid: u32, ch: &OneshotChannelSnapshot) {
        let node_id = NodeId::OneshotChannel {
            pid,
            name: ch.name.clone(),
        };
        self.nodes.insert(
            node_id.clone(),
            NodeKind::OneshotChannel {
                name: ch.name.clone(),
                state: ch.state,
            },
        );
        if let Some(creator_id) = ch.creator_task_id {
            let creator = NodeId::Task {
                pid,
                task_id: creator_id,
            };
            self.edges.push(WaitEdge {
                from: node_id,
                to: creator,
                kind: EdgeKind::ResourceOwnedByTask,
                meta: EdgeMeta {
                    source_snapshot: SnapshotSource::Sync,
                    count: 1,
                    severity_hint: 0,
                },
            });
        }
    }

    fn ingest_watch(&mut self, pid: u32, ch: &WatchChannelSnapshot) {
        let node_id = NodeId::WatchChannel {
            pid,
            name: ch.name.clone(),
        };
        self.nodes.insert(
            node_id.clone(),
            NodeKind::WatchChannel {
                name: ch.name.clone(),
                changes: ch.changes,
            },
        );
        if let Some(creator_id) = ch.creator_task_id {
            let creator = NodeId::Task {
                pid,
                task_id: creator_id,
            };
            self.edges.push(WaitEdge {
                from: node_id,
                to: creator,
                kind: EdgeKind::ResourceOwnedByTask,
                meta: EdgeMeta {
                    source_snapshot: SnapshotSource::Sync,
                    count: 1,
                    severity_hint: 0,
                },
            });
        }
    }

    fn ingest_once_cell(&mut self, pid: u32, cell: &OnceCellSnapshot) {
        let node_id = NodeId::OnceCell {
            pid,
            name: cell.name.clone(),
        };
        self.nodes.insert(
            node_id,
            NodeKind::OnceCell {
                name: cell.name.clone(),
                state: cell.state,
            },
        );
    }

    fn ingest_roam(&mut self, pid: u32, session: &SessionSnapshot) {
        for conn in &session.connections {
            self.ingest_connection(pid, conn, &session.method_names);
        }
    }

    fn ingest_connection(
        &mut self,
        pid: u32,
        conn: &ConnectionSnapshot,
        method_names: &std::collections::HashMap<u64, String>,
    ) {
        for req in &conn.in_flight {
            let method_name = req
                .method_name
                .clone()
                .or_else(|| method_names.get(&req.method_id).cloned());

            let req_node = NodeId::RpcRequest {
                pid,
                connection: conn.name.clone(),
                request_id: req.request_id,
            };

            self.nodes.insert(
                req_node.clone(),
                NodeKind::RpcRequest {
                    method_name: method_name.clone(),
                    direction: req.direction.clone(),
                    elapsed_secs: req.elapsed_secs,
                },
            );

            if let Some(task_id) = req.task_id {
                let task_node = NodeId::Task { pid, task_id };
                let severity = if req.elapsed_secs > 5.0 { 3 } else { 1 };

                match req.direction {
                    Direction::Outgoing => {
                        // client task -> waits on RPC request
                        self.edges.push(WaitEdge {
                            from: task_node,
                            to: req_node.clone(),
                            kind: EdgeKind::RpcClientToRequest,
                            meta: EdgeMeta {
                                source_snapshot: SnapshotSource::Roam,
                                count: 1,
                                severity_hint: severity,
                            },
                        });
                    }
                    Direction::Incoming => {
                        // RPC request -> handled by server task
                        self.edges.push(WaitEdge {
                            from: req_node.clone(),
                            to: task_node,
                            kind: EdgeKind::RpcRequestToServerTask,
                            meta: EdgeMeta {
                                source_snapshot: SnapshotSource::Roam,
                                count: 1,
                                severity_hint: severity,
                            },
                        });
                    }
                }
            }
        }
    }

    /// After all dumps are ingested, stitch outgoing RPC requests in one process
    /// to matching incoming RPC requests in another process.
    ///
    /// Match criteria: same method_name + same request_id, one Outgoing and one
    /// Incoming, in different processes.
    fn stitch_cross_process_rpc(&mut self) {
        // Collect RPC request nodes with their metadata
        struct RpcInfo {
            node_id: NodeId,
            method_name: Option<String>,
            request_id: u64,
            pid: u32,
            direction: Direction,
            elapsed_secs: f64,
        }

        let rpc_nodes: Vec<RpcInfo> = self
            .nodes
            .iter()
            .filter_map(|(id, kind)| {
                if let (
                    NodeId::RpcRequest {
                        pid, request_id, ..
                    },
                    NodeKind::RpcRequest {
                        method_name,
                        direction,
                        elapsed_secs,
                    },
                ) = (id, kind)
                {
                    Some(RpcInfo {
                        node_id: id.clone(),
                        method_name: method_name.clone(),
                        request_id: *request_id,
                        pid: *pid,
                        direction: direction.clone(),
                        elapsed_secs: *elapsed_secs,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Find matching pairs: outgoing in one process, incoming in another,
        // same method_name + request_id
        let mut new_edges = Vec::new();

        for i in 0..rpc_nodes.len() {
            for j in (i + 1)..rpc_nodes.len() {
                let a = &rpc_nodes[i];
                let b = &rpc_nodes[j];

                if a.pid == b.pid {
                    continue;
                }
                if a.request_id != b.request_id {
                    continue;
                }
                if a.method_name != b.method_name {
                    continue;
                }

                // Determine which is outgoing and which is incoming
                let (outgoing, incoming) = match (&a.direction, &b.direction) {
                    (Direction::Outgoing, Direction::Incoming) => (a, b),
                    (Direction::Incoming, Direction::Outgoing) => (b, a),
                    _ => continue, // same direction, not a stitch
                };

                let severity = if outgoing.elapsed_secs > 5.0 { 3 } else { 1 };

                new_edges.push(WaitEdge {
                    from: outgoing.node_id.clone(),
                    to: incoming.node_id.clone(),
                    kind: EdgeKind::RpcCrossProcessStitch,
                    meta: EdgeMeta {
                        source_snapshot: SnapshotSource::Roam,
                        count: 1,
                        severity_hint: severity,
                    },
                });
            }
        }

        self.edges.extend(new_edges);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn empty_dump(pid: u32, name: &str) -> ProcessDump {
        ProcessDump {
            process_name: name.to_string(),
            pid,
            timestamp: "2026-02-15T00:00:00Z".to_string(),
            tasks: vec![],
            wake_edges: vec![],
            future_wake_edges: vec![],
            future_waits: vec![],
            threads: vec![],
            locks: None,
            sync: None,
            roam: None,
            shm: None,
            custom: HashMap::new(),
        }
    }

    #[test]
    fn empty_dump_produces_process_node() {
        let graph = WaitGraph::build(&[empty_dump(1, "myapp")]);
        assert_eq!(graph.nodes.len(), 1);
        assert!(graph.edges.is_empty());
        let (id, kind) = graph.nodes.iter().next().unwrap();
        assert_eq!(*id, NodeId::Process { pid: 1 });
        match kind {
            NodeKind::Process { name, pid } => {
                assert_eq!(name, "myapp");
                assert_eq!(*pid, 1);
            }
            _ => panic!("expected Process node"),
        }
    }

    #[test]
    fn tasks_with_parent_produce_spawn_edges() {
        let mut dump = empty_dump(1, "app");
        dump.tasks = vec![
            TaskSnapshot {
                id: 1,
                name: "root".to_string(),
                state: TaskState::Polling,
                spawned_at_secs: 0.0,
                age_secs: 10.0,
                spawn_backtrace: String::new(),
                poll_events: vec![],
                parent_task_id: None,
                parent_task_name: None,
            },
            TaskSnapshot {
                id: 2,
                name: "child".to_string(),
                state: TaskState::Pending,
                spawned_at_secs: 1.0,
                age_secs: 9.0,
                spawn_backtrace: String::new(),
                poll_events: vec![],
                parent_task_id: Some(1),
                parent_task_name: Some("root".to_string()),
            },
        ];
        let graph = WaitGraph::build(&[dump]);
        // process + 2 tasks = 3 nodes
        assert_eq!(graph.nodes.len(), 3);
        // 1 spawn edge
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].kind, EdgeKind::TaskSpawnedTask);
        assert_eq!(
            graph.edges[0].from,
            NodeId::Task {
                pid: 1,
                task_id: 1
            }
        );
        assert_eq!(
            graph.edges[0].to,
            NodeId::Task {
                pid: 1,
                task_id: 2
            }
        );
    }

    #[test]
    fn lock_contention_produces_wait_and_ownership_edges() {
        let mut dump = empty_dump(1, "app");
        dump.tasks = vec![
            TaskSnapshot {
                id: 10,
                name: "holder".to_string(),
                state: TaskState::Polling,
                spawned_at_secs: 0.0,
                age_secs: 5.0,
                spawn_backtrace: String::new(),
                poll_events: vec![],
                parent_task_id: None,
                parent_task_name: None,
            },
            TaskSnapshot {
                id: 20,
                name: "waiter".to_string(),
                state: TaskState::Pending,
                spawned_at_secs: 1.0,
                age_secs: 4.0,
                spawn_backtrace: String::new(),
                poll_events: vec![],
                parent_task_id: None,
                parent_task_name: None,
            },
        ];
        dump.locks = Some(peeps_types::LockSnapshot {
            locks: vec![peeps_types::LockInfoSnapshot {
                name: "db_pool".to_string(),
                acquires: 100,
                releases: 99,
                holders: vec![peeps_types::LockHolderSnapshot {
                    kind: LockAcquireKind::Mutex,
                    held_secs: 2.0,
                    backtrace: None,
                    task_id: Some(10),
                    task_name: Some("holder".to_string()),
                }],
                waiters: vec![peeps_types::LockWaiterSnapshot {
                    kind: LockAcquireKind::Mutex,
                    waiting_secs: 0.5,
                    backtrace: None,
                    task_id: Some(20),
                    task_name: Some("waiter".to_string()),
                }],
            }],
        });

        let graph = WaitGraph::build(&[dump]);

        // process + 2 tasks + 1 lock = 4 nodes
        assert_eq!(graph.nodes.len(), 4);

        let ownership_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::ResourceOwnedByTask)
            .collect();
        assert_eq!(ownership_edges.len(), 1);
        assert_eq!(
            ownership_edges[0].from,
            NodeId::Lock {
                pid: 1,
                name: "db_pool".to_string()
            }
        );
        assert_eq!(
            ownership_edges[0].to,
            NodeId::Task {
                pid: 1,
                task_id: 10
            }
        );

        let wait_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::TaskWaitsOnResource)
            .collect();
        assert_eq!(wait_edges.len(), 1);
        assert_eq!(
            wait_edges[0].from,
            NodeId::Task {
                pid: 1,
                task_id: 20
            }
        );
        assert_eq!(
            wait_edges[0].to,
            NodeId::Lock {
                pid: 1,
                name: "db_pool".to_string()
            }
        );
    }

    #[test]
    fn future_wait_produces_edges() {
        let mut dump = empty_dump(1, "app");
        dump.tasks = vec![TaskSnapshot {
            id: 5,
            name: "poller".to_string(),
            state: TaskState::Pending,
            spawned_at_secs: 0.0,
            age_secs: 3.0,
            spawn_backtrace: String::new(),
            poll_events: vec![],
            parent_task_id: None,
            parent_task_name: None,
        }];
        dump.future_waits = vec![FutureWaitSnapshot {
            future_id: 42,
            task_id: 5,
            task_name: Some("poller".to_string()),
            resource: "timeout".to_string(),
            created_by_task_id: Some(5),
            created_by_task_name: Some("poller".to_string()),
            created_age_secs: 3.0,
            last_polled_by_task_id: Some(5),
            last_polled_by_task_name: Some("poller".to_string()),
            pending_count: 3,
            ready_count: 0,
            total_pending_secs: 2.0,
            last_seen_age_secs: 0.1,
        }];
        let graph = WaitGraph::build(&[dump]);

        // process + task + future = 3 nodes
        assert_eq!(graph.nodes.len(), 3);

        // task -> waits on future, future -> owned by task
        assert_eq!(graph.edges.len(), 2);

        let wait = graph
            .edges
            .iter()
            .find(|e| e.kind == EdgeKind::TaskWaitsOnResource)
            .unwrap();
        // never been ready => severity 2
        assert_eq!(wait.meta.severity_hint, 2);
    }

    #[test]
    fn rpc_in_flight_produces_edges() {
        let mut dump = empty_dump(1, "app");
        dump.tasks = vec![TaskSnapshot {
            id: 7,
            name: "rpc-caller".to_string(),
            state: TaskState::Pending,
            spawned_at_secs: 0.0,
            age_secs: 2.0,
            spawn_backtrace: String::new(),
            poll_events: vec![],
            parent_task_id: None,
            parent_task_name: None,
        }];
        dump.roam = Some(SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn-1".to_string(),
                peer_name: Some("backend".to_string()),
                age_secs: 60.0,
                total_completed: 100,
                max_concurrent_requests: 8,
                initial_credit: 8,
                in_flight: vec![peeps_types::RequestSnapshot {
                    request_id: 99,
                    method_name: Some("get_user".to_string()),
                    method_id: 1,
                    direction: Direction::Outgoing,
                    elapsed_secs: 6.0,
                    task_id: Some(7),
                    task_name: Some("rpc-caller".to_string()),
                    metadata: None,
                    args: None,
                    backtrace: None,
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: peeps_types::TransportStats {
                    frames_sent: 200,
                    frames_received: 200,
                    bytes_sent: 10000,
                    bytes_received: 10000,
                    last_sent_ago_secs: Some(0.1),
                    last_recv_ago_secs: Some(0.1),
                },
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
        });

        let graph = WaitGraph::build(&[dump]);

        let rpc_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::RpcClientToRequest)
            .collect();
        assert_eq!(rpc_edges.len(), 1);
        // >5s elapsed => severity 3
        assert_eq!(rpc_edges[0].meta.severity_hint, 3);
    }

    #[test]
    fn multiple_dumps_merge() {
        let dump1 = empty_dump(1, "frontend");
        let dump2 = empty_dump(2, "backend");
        let graph = WaitGraph::build(&[dump1, dump2]);
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.nodes.contains_key(&NodeId::Process { pid: 1 }));
        assert!(graph.nodes.contains_key(&NodeId::Process { pid: 2 }));
    }

    // ════════════════════════════════════════════════════════════════
    // Fixture corpus: realistic ProcessDump scenarios for end-to-end
    // validation of graph normalization, cycle detection, and severity
    // ranking. Each fixture builds ProcessDump(s) and asserts through
    // the full pipeline: build → detect → rank.
    // ════════════════════════════════════════════════════════════════

    fn make_task(id: u64, name: &str, state: TaskState, age: f64) -> TaskSnapshot {
        TaskSnapshot {
            id,
            name: name.to_string(),
            state,
            spawned_at_secs: 0.0,
            age_secs: age,
            spawn_backtrace: String::new(),
            poll_events: vec![],
            parent_task_id: None,
            parent_task_name: None,
        }
    }

    fn make_lock(
        name: &str,
        holders: Vec<(u64, &str, f64)>,
        waiters: Vec<(u64, &str, f64)>,
    ) -> peeps_types::LockInfoSnapshot {
        peeps_types::LockInfoSnapshot {
            name: name.to_string(),
            acquires: 100,
            releases: 99,
            holders: holders
                .into_iter()
                .map(|(tid, tname, held)| peeps_types::LockHolderSnapshot {
                    kind: LockAcquireKind::Mutex,
                    held_secs: held,
                    backtrace: None,
                    task_id: Some(tid),
                    task_name: Some(tname.to_string()),
                })
                .collect(),
            waiters: waiters
                .into_iter()
                .map(|(tid, tname, wait)| peeps_types::LockWaiterSnapshot {
                    kind: LockAcquireKind::Mutex,
                    waiting_secs: wait,
                    backtrace: None,
                    task_id: Some(tid),
                    task_name: Some(tname.to_string()),
                })
                .collect(),
        }
    }

    fn make_connection(
        name: &str,
        requests: Vec<peeps_types::RequestSnapshot>,
    ) -> ConnectionSnapshot {
        ConnectionSnapshot {
            name: name.to_string(),
            peer_name: Some("peer".to_string()),
            age_secs: 60.0,
            total_completed: 50,
            max_concurrent_requests: 8,
            initial_credit: 8,
            in_flight: requests,
            recent_completions: vec![],
            channels: vec![],
            transport: peeps_types::TransportStats {
                frames_sent: 100,
                frames_received: 100,
                bytes_sent: 5000,
                bytes_received: 5000,
                last_sent_ago_secs: Some(0.1),
                last_recv_ago_secs: Some(0.1),
            },
            channel_credits: vec![],
        }
    }

    fn make_rpc_request(
        method: &str,
        request_id: u64,
        direction: Direction,
        elapsed: f64,
        task_id: Option<u64>,
        task_name: Option<&str>,
    ) -> peeps_types::RequestSnapshot {
        peeps_types::RequestSnapshot {
            request_id,
            method_name: Some(method.to_string()),
            method_id: 0,
            direction,
            elapsed_secs: elapsed,
            task_id,
            task_name: task_name.map(|s| s.to_string()),
            metadata: None,
            args: None,
            backtrace: None,
        }
    }

    // ── Fixture 1: True deadlock cycle ─────────────────────────────
    //
    // Task A (id=1) holds lock X, waits on lock Y.
    // Task B (id=2) holds lock Y, waits on lock X.
    //
    // Expected: cycle A → lock-Y → B → lock-X → A detected as Danger.

    fn fixture_true_deadlock() -> ProcessDump {
        let mut dump = empty_dump(1, "deadlock-app");
        dump.tasks = vec![
            make_task(1, "task-A", TaskState::Pending, 10.0),
            make_task(2, "task-B", TaskState::Pending, 10.0),
        ];
        dump.locks = Some(peeps_types::LockSnapshot {
            locks: vec![
                make_lock("lock-X", vec![(1, "task-A", 5.0)], vec![(2, "task-B", 4.0)]),
                make_lock("lock-Y", vec![(2, "task-B", 5.0)], vec![(1, "task-A", 4.0)]),
            ],
        });
        dump
    }

    #[test]
    fn fixture_true_deadlock_normalization() {
        let graph = WaitGraph::build(&[fixture_true_deadlock()]);

        // 1 process + 2 tasks + 2 locks = 5 nodes
        assert_eq!(graph.nodes.len(), 5);

        // Each lock: 1 ownership + 1 wait = 4 edges total
        assert_eq!(graph.edges.len(), 4);

        let wait_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::TaskWaitsOnResource)
            .collect();
        assert_eq!(wait_edges.len(), 2);

        let owns_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::ResourceOwnedByTask)
            .collect();
        assert_eq!(owns_edges.len(), 2);

        // task-A waits on lock-Y
        assert!(wait_edges.iter().any(|e| e.from
            == NodeId::Task {
                pid: 1,
                task_id: 1
            }
            && e.to
                == NodeId::Lock {
                    pid: 1,
                    name: "lock-Y".to_string()
                }));

        // task-B waits on lock-X
        assert!(wait_edges.iter().any(|e| e.from
            == NodeId::Task {
                pid: 1,
                task_id: 2
            }
            && e.to
                == NodeId::Lock {
                    pid: 1,
                    name: "lock-X".to_string()
                }));

        // lock-X owned by task-A
        assert!(owns_edges.iter().any(|e| e.from
            == NodeId::Lock {
                pid: 1,
                name: "lock-X".to_string()
            }
            && e.to
                == NodeId::Task {
                    pid: 1,
                    task_id: 1
                }));

        // lock-Y owned by task-B
        assert!(owns_edges.iter().any(|e| e.from
            == NodeId::Lock {
                pid: 1,
                name: "lock-Y".to_string()
            }
            && e.to
                == NodeId::Task {
                    pid: 1,
                    task_id: 2
                }));
    }

    #[test]
    fn fixture_true_deadlock_cycle_detection() {
        let graph = WaitGraph::build(&[fixture_true_deadlock()]);
        let candidates = detect::find_deadlock_candidates(&graph);

        assert_eq!(candidates.len(), 1);
        let c = &candidates[0];

        // All four nodes in the cycle
        assert_eq!(c.nodes.len(), 4);
        assert!(c.nodes.contains(&NodeId::Task {
            pid: 1,
            task_id: 1
        }));
        assert!(c.nodes.contains(&NodeId::Task {
            pid: 1,
            task_id: 2
        }));
        assert!(c.nodes.contains(&NodeId::Lock {
            pid: 1,
            name: "lock-X".to_string()
        }));
        assert!(c.nodes.contains(&NodeId::Lock {
            pid: 1,
            name: "lock-Y".to_string()
        }));

        // Cycle path is closed
        assert_eq!(c.cycle_path.first(), c.cycle_path.last());
        assert!(c.cycle_path.len() >= 5); // 4 nodes + closing = 5

        // Severity: age 10s + cycle = at least warn
        assert!(c.severity >= detect::Severity::Warn);
        assert!(c.severity_score >= 20);
    }

    // ── Fixture 2: Long wait, no cycle ─────────────────────────────
    //
    // Task C calls an RPC that's been running for 30s.
    // No circular dependency — just slow. No deadlock candidate.

    fn fixture_long_wait_no_cycle() -> ProcessDump {
        let mut dump = empty_dump(2, "slow-rpc-app");
        dump.tasks = vec![make_task(3, "rpc-caller", TaskState::Pending, 30.0)];
        dump.roam = Some(SessionSnapshot {
            connections: vec![make_connection(
                "conn-slow",
                vec![make_rpc_request(
                    "heavy_query",
                    200,
                    Direction::Outgoing,
                    30.0,
                    Some(3),
                    Some("rpc-caller"),
                )],
            )],
            method_names: HashMap::new(),
        });
        dump
    }

    #[test]
    fn fixture_long_wait_no_cycle_normalization() {
        let graph = WaitGraph::build(&[fixture_long_wait_no_cycle()]);

        // 1 process + 1 task + 1 rpc_request = 3 nodes
        assert_eq!(graph.nodes.len(), 3);

        let rpc_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::RpcClientToRequest)
            .collect();
        assert_eq!(rpc_edges.len(), 1);
        assert_eq!(rpc_edges[0].meta.severity_hint, 3); // >5s

        // No wait-on-resource edges
        let wait_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::TaskWaitsOnResource)
            .collect();
        assert!(wait_edges.is_empty());
    }

    #[test]
    fn fixture_long_wait_no_cycle_detection() {
        let graph = WaitGraph::build(&[fixture_long_wait_no_cycle()]);
        let candidates = detect::find_deadlock_candidates(&graph);

        // No cycle — the RPC is just slow, not deadlocked
        assert!(candidates.is_empty());
    }

    // ── Fixture 3: Bursty transient waits ──────────────────────────
    //
    // Multiple tasks contending on a lock with very short wait times.
    // Lock holder is actively working, waiters waited < 5ms.
    // This is normal contention, NOT a deadlock.

    fn fixture_bursty_transient() -> ProcessDump {
        let mut dump = empty_dump(3, "bursty-app");
        dump.tasks = vec![
            make_task(10, "worker-1", TaskState::Polling, 2.0),
            make_task(11, "worker-2", TaskState::Pending, 2.0),
            make_task(12, "worker-3", TaskState::Pending, 2.0),
        ];
        dump.locks = Some(peeps_types::LockSnapshot {
            locks: vec![peeps_types::LockInfoSnapshot {
                name: "hot-mutex".to_string(),
                acquires: 10000,
                releases: 9999,
                holders: vec![peeps_types::LockHolderSnapshot {
                    kind: LockAcquireKind::Mutex,
                    held_secs: 0.001, // 1ms
                    backtrace: None,
                    task_id: Some(10),
                    task_name: Some("worker-1".to_string()),
                }],
                waiters: vec![
                    peeps_types::LockWaiterSnapshot {
                        kind: LockAcquireKind::Mutex,
                        waiting_secs: 0.002,
                        backtrace: None,
                        task_id: Some(11),
                        task_name: Some("worker-2".to_string()),
                    },
                    peeps_types::LockWaiterSnapshot {
                        kind: LockAcquireKind::Mutex,
                        waiting_secs: 0.001,
                        backtrace: None,
                        task_id: Some(12),
                        task_name: Some("worker-3".to_string()),
                    },
                ],
            }],
        });
        dump
    }

    #[test]
    fn fixture_bursty_transient_normalization() {
        let graph = WaitGraph::build(&[fixture_bursty_transient()]);

        // 1 process + 3 tasks + 1 lock = 5 nodes
        assert_eq!(graph.nodes.len(), 5);

        // 1 ownership edge + 2 wait edges = 3
        assert_eq!(graph.edges.len(), 3);

        // All severity hints should be low
        let owns = graph
            .edges
            .iter()
            .find(|e| e.kind == EdgeKind::ResourceOwnedByTask)
            .unwrap();
        assert_eq!(owns.meta.severity_hint, 1); // Mutex but held < 1s

        for wait in graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::TaskWaitsOnResource)
        {
            assert_eq!(wait.meta.severity_hint, 1); // waiting < 1s
        }
    }

    #[test]
    fn fixture_bursty_transient_no_deadlock() {
        let graph = WaitGraph::build(&[fixture_bursty_transient()]);
        let candidates = detect::find_deadlock_candidates(&graph);

        // No cycle: workers wait on lock, lock owned by one worker.
        // This is a DAG (waiter -> lock -> holder), not a cycle.
        assert!(candidates.is_empty());
    }

    // ── Fixture 4: Cross-process RPC chain cycle ───────────────────
    //
    // Process A (pid=100): task-1 sends RPC to B, waiting for response.
    // Process B (pid=200): task-2 handles that request, but sends an
    //   RPC back to process A and is waiting for response.
    // Process A (pid=100): task-3 handles incoming from B, but waits
    //   on a lock held by task-1.
    //
    // Cycle: A:task-1 --rpc--> B:task-2 --rpc--> A:task-3 --lock--> A:task-1

    fn fixture_cross_process_rpc_cycle() -> Vec<ProcessDump> {
        // Process A
        let mut dump_a = empty_dump(100, "process-A");
        dump_a.tasks = vec![
            make_task(1, "a-sender", TaskState::Pending, 15.0),
            make_task(3, "a-handler", TaskState::Pending, 10.0),
        ];
        dump_a.roam = Some(SessionSnapshot {
            connections: vec![
                make_connection(
                    "conn-to-B",
                    vec![make_rpc_request(
                        "do_work",
                        300,
                        Direction::Outgoing,
                        10.0,
                        Some(1),
                        Some("a-sender"),
                    )],
                ),
                make_connection(
                    "conn-from-B",
                    vec![make_rpc_request(
                        "callback",
                        400,
                        Direction::Incoming,
                        8.0,
                        Some(3),
                        Some("a-handler"),
                    )],
                ),
            ],
            method_names: HashMap::new(),
        });
        dump_a.locks = Some(peeps_types::LockSnapshot {
            locks: vec![make_lock(
                "shared-state",
                vec![(1, "a-sender", 10.0)],
                vec![(3, "a-handler", 8.0)],
            )],
        });

        // Process B
        let mut dump_b = empty_dump(200, "process-B");
        dump_b.tasks = vec![make_task(2, "b-handler", TaskState::Pending, 12.0)];
        dump_b.roam = Some(SessionSnapshot {
            connections: vec![
                make_connection(
                    "conn-from-A",
                    vec![make_rpc_request(
                        "do_work",
                        300,
                        Direction::Incoming,
                        10.0,
                        Some(2),
                        Some("b-handler"),
                    )],
                ),
                make_connection(
                    "conn-to-A",
                    vec![make_rpc_request(
                        "callback",
                        400,
                        Direction::Outgoing,
                        8.0,
                        Some(2),
                        Some("b-handler"),
                    )],
                ),
            ],
            method_names: HashMap::new(),
        });

        vec![dump_a, dump_b]
    }

    #[test]
    fn fixture_cross_process_normalization() {
        let dumps = fixture_cross_process_rpc_cycle();
        let graph = WaitGraph::build(&dumps);

        // Process A: process + 2 tasks + 2 rpc_requests + 1 lock = 6
        // Process B: process + 1 task + 2 rpc_requests = 4
        // Total = 10
        assert_eq!(graph.nodes.len(), 10);

        // Process A: outgoing RPC (1→req), incoming RPC (req→3), lock own (lock→1), lock wait (3→lock) = 4
        // Process B: incoming RPC (req→2), outgoing RPC (2→req) = 2
        // Cross-process stitch: do_work outgoing→incoming, callback outgoing→incoming = 2
        // Total = 8
        assert_eq!(graph.edges.len(), 8);

        let rpc_client_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::RpcClientToRequest)
            .collect();
        assert_eq!(rpc_client_edges.len(), 2);

        let rpc_server_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::RpcRequestToServerTask)
            .collect();
        assert_eq!(rpc_server_edges.len(), 2);

        // Lock contention: task-3 waits, task-1 holds
        let lock_wait = graph
            .edges
            .iter()
            .find(|e| e.kind == EdgeKind::TaskWaitsOnResource)
            .unwrap();
        assert_eq!(
            lock_wait.from,
            NodeId::Task {
                pid: 100,
                task_id: 3
            }
        );
        assert_eq!(lock_wait.meta.severity_hint, 3); // >1s

        let lock_own = graph
            .edges
            .iter()
            .find(|e| e.kind == EdgeKind::ResourceOwnedByTask)
            .unwrap();
        assert_eq!(
            lock_own.to,
            NodeId::Task {
                pid: 100,
                task_id: 1
            }
        );
        assert_eq!(lock_own.meta.severity_hint, 3); // >1s
    }

    #[test]
    fn fixture_cross_process_cycle_via_stitching() {
        // The ingestion pipeline stitches outgoing RPC nodes in one process
        // to incoming RPC nodes in another (matching by method_name + request_id).
        // This makes the cross-process cycle visible to the detector.
        let dumps = fixture_cross_process_rpc_cycle();
        let graph = WaitGraph::build(&dumps);
        let candidates = detect::find_deadlock_candidates(&graph);

        assert_eq!(
            candidates.len(),
            1,
            "cross-process stitching should reveal the RPC cycle"
        );
        let c = &candidates[0];
        assert!(c.rationale.iter().any(|r| r.contains("processes")));
        assert!(c.severity >= detect::Severity::Warn);
    }

    // ── Comparative severity ranking ───────────────────────────────

    #[test]
    fn cross_process_outranks_single_process() {
        let single_graph = WaitGraph::build(&[fixture_true_deadlock()]);
        let cross_graph = WaitGraph::build(&fixture_cross_process_rpc_cycle());

        let single_candidates = detect::find_deadlock_candidates(&single_graph);
        let cross_candidates = detect::find_deadlock_candidates(&cross_graph);

        assert_eq!(single_candidates.len(), 1);
        assert_eq!(cross_candidates.len(), 1);

        // Cross-process cycle should score higher (has cross-process bonus)
        assert!(
            cross_candidates[0].severity_score >= single_candidates[0].severity_score,
            "cross-process ({}) should score >= single-process ({})",
            cross_candidates[0].severity_score,
            single_candidates[0].severity_score
        );

        // Cross-process should be Danger
        assert_eq!(cross_candidates[0].severity, detect::Severity::Danger);

        // Single-process should be at least Warn
        assert!(single_candidates[0].severity >= detect::Severity::Warn);
    }

    // ── Sync channel fixture ───────────────────────────────────────

    #[test]
    fn mpsc_with_blocked_senders_produces_wait_edge() {
        let mut dump = empty_dump(1, "channel-app");
        dump.tasks = vec![make_task(5, "producer", TaskState::Pending, 3.0)];
        dump.sync = Some(SyncSnapshot {
            mpsc_channels: vec![MpscChannelSnapshot {
                name: "work-queue".to_string(),
                bounded: true,
                capacity: Some(10),
                sent: 1000,
                received: 990,
                send_waiters: 3,
                sender_count: 4,
                sender_closed: false,
                receiver_closed: false,
                age_secs: 60.0,
                creator_task_id: Some(5),
                creator_task_name: Some("producer".to_string()),
            }],
            oneshot_channels: vec![],
            watch_channels: vec![],
            once_cells: vec![],
        });

        let graph = WaitGraph::build(&[dump]);
        assert_eq!(graph.nodes.len(), 3); // process + task + channel

        let owns: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::ResourceOwnedByTask)
            .collect();
        assert_eq!(owns.len(), 1);

        let waits: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::TaskWaitsOnResource)
            .collect();
        assert_eq!(waits.len(), 1);
        assert_eq!(waits[0].meta.count, 3);
        assert_eq!(waits[0].meta.severity_hint, 2);
    }

    // ── Wake edge normalization ────────────────────────────────────

    #[test]
    fn wake_edges_produce_graph_edges() {
        let mut dump = empty_dump(1, "wake-app");
        dump.tasks = vec![
            make_task(1, "waker", TaskState::Polling, 5.0),
            make_task(2, "sleeper", TaskState::Pending, 5.0),
        ];
        dump.wake_edges = vec![WakeEdgeSnapshot {
            source_task_id: Some(1),
            source_task_name: Some("waker".to_string()),
            target_task_id: 2,
            target_task_name: Some("sleeper".to_string()),
            wake_count: 42,
            last_wake_age_secs: 0.1,
        }];

        let graph = WaitGraph::build(&[dump]);
        assert_eq!(graph.nodes.len(), 3); // process + 2 tasks

        let wake_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::TaskWakesFuture)
            .collect();
        assert_eq!(wake_edges.len(), 1);
        assert_eq!(wake_edges[0].meta.count, 42);
    }

    // ── Future wake edge normalization ─────────────────────────────

    #[test]
    fn future_wake_edges_produce_graph_edges() {
        let mut dump = empty_dump(1, "fut-wake-app");
        dump.tasks = vec![
            make_task(1, "producer", TaskState::Polling, 5.0),
            make_task(2, "consumer", TaskState::Pending, 5.0),
        ];
        dump.future_wake_edges = vec![FutureWakeEdgeSnapshot {
            source_task_id: Some(1),
            source_task_name: Some("producer".to_string()),
            future_id: 99,
            future_resource: "notify".to_string(),
            target_task_id: Some(2),
            target_task_name: Some("consumer".to_string()),
            wake_count: 10,
            last_wake_age_secs: 0.05,
        }];

        let graph = WaitGraph::build(&[dump]);
        assert_eq!(graph.nodes.len(), 4); // process + 2 tasks + 1 future

        let wakes: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::TaskWakesFuture)
            .collect();
        assert_eq!(wakes.len(), 1);

        let resumes: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::FutureResumesTask)
            .collect();
        assert_eq!(resumes.len(), 1);
        assert_eq!(
            resumes[0].to,
            NodeId::Task {
                pid: 1,
                task_id: 2
            }
        );
    }

    // ── Oneshot channel normalization ──────────────────────────────

    #[test]
    fn oneshot_channel_produces_ownership_edge() {
        let mut dump = empty_dump(1, "oneshot-app");
        dump.tasks = vec![make_task(7, "requester", TaskState::Pending, 1.0)];
        dump.sync = Some(SyncSnapshot {
            mpsc_channels: vec![],
            oneshot_channels: vec![OneshotChannelSnapshot {
                name: "response-ch".to_string(),
                state: OneshotState::Pending,
                age_secs: 1.0,
                creator_task_id: Some(7),
                creator_task_name: Some("requester".to_string()),
            }],
            watch_channels: vec![],
            once_cells: vec![],
        });

        let graph = WaitGraph::build(&[dump]);
        assert_eq!(graph.nodes.len(), 3); // process + task + oneshot

        let owns: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::ResourceOwnedByTask)
            .collect();
        assert_eq!(owns.len(), 1);
        assert_eq!(
            owns[0].from,
            NodeId::OneshotChannel {
                pid: 1,
                name: "response-ch".to_string()
            }
        );
    }

    // ── OnceCell normalization ─────────────────────────────────────

    #[test]
    fn once_cell_creates_node() {
        let mut dump = empty_dump(1, "cell-app");
        dump.sync = Some(SyncSnapshot {
            mpsc_channels: vec![],
            oneshot_channels: vec![],
            watch_channels: vec![],
            once_cells: vec![OnceCellSnapshot {
                name: "config".to_string(),
                state: OnceCellState::Initializing,
                age_secs: 0.5,
                init_duration_secs: None,
            }],
        });

        let graph = WaitGraph::build(&[dump]);
        assert_eq!(graph.nodes.len(), 2); // process + once_cell
        assert!(graph.nodes.contains_key(&NodeId::OnceCell {
            pid: 1,
            name: "config".to_string()
        }));
    }
}
