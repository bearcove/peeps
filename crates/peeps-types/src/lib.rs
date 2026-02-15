//! Shared diagnostic snapshot types for peeps.
//!
//! All snapshot types live here so they can be used for both serialization
//! (producing dumps) and deserialization (reading dumps) without circular
//! dependencies between peeps subcrates and instrumented libraries.

use std::collections::HashMap;
use std::sync::OnceLock;

use facet::Facet;

// ── Global process name ─────────────────────────────────────────

static PROCESS_NAME: OnceLock<String> = OnceLock::new();

/// Set the global process name for this process.
///
/// Should be called once at startup (e.g. from `peeps::init_named`).
/// Subsequent calls are ignored (first write wins).
pub fn set_process_name(name: impl Into<String>) {
    let _ = PROCESS_NAME.set(name.into());
}

/// Get the global process name, if set.
pub fn process_name() -> Option<&'static str> {
    PROCESS_NAME.get().map(|s| s.as_str())
}

// ── Reserved metadata keys for context propagation ──────────────

/// Metadata key for the caller's process name.
pub const PEEPS_CALLER_PROCESS_KEY: &str = "peeps.caller_process";
/// Metadata key for the caller's connection name.
pub const PEEPS_CALLER_CONNECTION_KEY: &str = "peeps.caller_connection";
/// Metadata key for the caller's request ID.
pub const PEEPS_CALLER_REQUEST_ID_KEY: &str = "peeps.caller_request_id";

// ── Task snapshot types ──────────────────────────────────────────

/// Unique task ID.
pub type TaskId = u64;
/// Unique instrumented future ID.
pub type FutureId = u64;

/// Snapshot of a tracked task for diagnostics.
#[derive(Debug, Clone, Facet)]
pub struct TaskSnapshot {
    pub id: TaskId,
    pub name: String,
    pub state: TaskState,
    pub spawned_at_secs: f64,
    pub age_secs: f64,
    pub spawn_backtrace: String,
    pub poll_events: Vec<PollEvent>,
    /// Which task spawned this one.
    pub parent_task_id: Option<TaskId>,
    /// Name of the parent task (resolved at snapshot time).
    pub parent_task_name: Option<String>,
}

/// Snapshot of a wake/dependency edge between tasks.
#[derive(Debug, Clone, Facet)]
pub struct WakeEdgeSnapshot {
    /// Task that triggered the wake, when known.
    pub source_task_id: Option<TaskId>,
    pub source_task_name: Option<String>,
    /// Task that was woken.
    pub target_task_id: TaskId,
    pub target_task_name: Option<String>,
    /// Number of observed wake calls for this edge.
    pub wake_count: u64,
    /// Age of the most recent wake event.
    pub last_wake_age_secs: f64,
}

/// Snapshot of a wake/dependency edge from a task to an instrumented future.
#[derive(Debug, Clone, Facet)]
pub struct FutureWakeEdgeSnapshot {
    /// Task that triggered the wake, when known.
    pub source_task_id: Option<TaskId>,
    pub source_task_name: Option<String>,
    /// Instrumented future that was woken.
    pub future_id: FutureId,
    pub future_resource: String,
    /// Last known task polling this future.
    pub target_task_id: Option<TaskId>,
    pub target_task_name: Option<String>,
    /// Number of observed wake calls for this edge.
    pub wake_count: u64,
    /// Age of the most recent wake event.
    pub last_wake_age_secs: f64,
}

/// Snapshot of a task waiting on an annotated future/resource.
#[derive(Debug, Clone, Facet)]
pub struct FutureWaitSnapshot {
    pub future_id: FutureId,
    pub task_id: TaskId,
    pub task_name: Option<String>,
    pub resource: String,
    pub created_by_task_id: Option<TaskId>,
    pub created_by_task_name: Option<String>,
    pub created_age_secs: f64,
    pub last_polled_by_task_id: Option<TaskId>,
    pub last_polled_by_task_name: Option<String>,
    pub pending_count: u64,
    pub ready_count: u64,
    pub total_pending_secs: f64,
    pub last_seen_age_secs: f64,
}

/// Task state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum TaskState {
    Pending,
    Polling,
    Completed,
}

/// A single poll event.
#[derive(Debug, Clone, Facet)]
pub struct PollEvent {
    pub started_at_secs: f64,
    pub duration_secs: Option<f64>,
    pub result: PollResult,
    pub backtrace: Option<String>,
}

/// Poll result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PollResult {
    Pending,
    Ready,
}

// ── Thread snapshot types ────────────────────────────────────────

/// A single thread's stack trace, with sampling data for stuck detection.
#[derive(Debug, Clone, Facet)]
pub struct ThreadStackSnapshot {
    pub name: String,
    pub backtrace: Option<String>,
    pub samples: u32,
    pub responded: u32,
    pub same_location_count: u32,
    pub dominant_frame: Option<String>,
}

// ── Lock snapshot types ──────────────────────────────────────────

/// Snapshot of all tracked locks.
#[derive(Debug, Clone, Facet)]
pub struct LockSnapshot {
    pub locks: Vec<LockInfoSnapshot>,
}

/// Snapshot of a single tracked lock.
#[derive(Debug, Clone, Facet)]
pub struct LockInfoSnapshot {
    pub name: String,
    pub acquires: u64,
    pub releases: u64,
    pub holders: Vec<LockHolderSnapshot>,
    pub waiters: Vec<LockWaiterSnapshot>,
}

/// Kind of lock acquisition.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum LockAcquireKind {
    Read,
    Write,
    Mutex,
}

/// A current lock holder.
#[derive(Debug, Clone, Facet)]
pub struct LockHolderSnapshot {
    pub kind: LockAcquireKind,
    pub held_secs: f64,
    pub backtrace: Option<String>,
    /// Which peeps task holds this lock.
    pub task_id: Option<u64>,
    pub task_name: Option<String>,
}

/// A lock waiter.
#[derive(Debug, Clone, Facet)]
pub struct LockWaiterSnapshot {
    pub kind: LockAcquireKind,
    pub waiting_secs: f64,
    pub backtrace: Option<String>,
    /// Which peeps task is waiting for this lock.
    pub task_id: Option<u64>,
    pub task_name: Option<String>,
}

// ── Channel & sync snapshot types ───────────────────────────────

/// Snapshot of all tracked channels and sync primitives.
#[derive(Debug, Clone, Facet)]
pub struct SyncSnapshot {
    pub mpsc_channels: Vec<MpscChannelSnapshot>,
    pub oneshot_channels: Vec<OneshotChannelSnapshot>,
    pub watch_channels: Vec<WatchChannelSnapshot>,
    pub semaphores: Vec<SemaphoreSnapshot>,
    pub once_cells: Vec<OnceCellSnapshot>,
}

/// Snapshot of a tracked mpsc channel.
#[derive(Debug, Clone, Facet)]
pub struct MpscChannelSnapshot {
    pub name: String,
    pub bounded: bool,
    /// Max capacity (None for unbounded).
    pub capacity: Option<u64>,
    pub sent: u64,
    pub received: u64,
    /// Number of senders currently blocked waiting to send (bounded only).
    pub send_waiters: u64,
    pub sender_count: u64,
    pub sender_closed: bool,
    pub receiver_closed: bool,
    pub age_secs: f64,
    /// Which task created this channel.
    pub creator_task_id: Option<u64>,
    pub creator_task_name: Option<String>,
}

/// Snapshot of a tracked oneshot channel.
#[derive(Debug, Clone, Facet)]
pub struct OneshotChannelSnapshot {
    pub name: String,
    pub state: OneshotState,
    pub age_secs: f64,
    /// Which task created this channel.
    pub creator_task_id: Option<u64>,
    pub creator_task_name: Option<String>,
}

/// State of a oneshot channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum OneshotState {
    Pending,
    Sent,
    Received,
    SenderDropped,
    ReceiverDropped,
}

/// Snapshot of a tracked watch channel.
#[derive(Debug, Clone, Facet)]
pub struct WatchChannelSnapshot {
    pub name: String,
    pub changes: u64,
    pub receiver_count: u64,
    pub age_secs: f64,
    /// Which task created this channel.
    pub creator_task_id: Option<u64>,
    pub creator_task_name: Option<String>,
}

/// Snapshot of a tracked semaphore.
#[derive(Debug, Clone, Facet)]
pub struct SemaphoreSnapshot {
    pub name: String,
    pub permits_total: u64,
    pub permits_available: u64,
    pub waiters: u64,
    pub acquires: u64,
    pub avg_wait_secs: f64,
    pub max_wait_secs: f64,
    pub age_secs: f64,
    /// Which task created this semaphore.
    pub creator_task_id: Option<u64>,
    pub creator_task_name: Option<String>,
    /// Task IDs of the top waiters (those currently waiting for a permit).
    pub top_waiter_task_ids: Vec<u64>,
    /// How long the oldest current waiter has been waiting.
    pub oldest_wait_secs: f64,
}

/// Snapshot of a tracked OnceCell.
#[derive(Debug, Clone, Facet)]
pub struct OnceCellSnapshot {
    pub name: String,
    pub state: OnceCellState,
    pub age_secs: f64,
    pub init_duration_secs: Option<f64>,
}

/// State of a OnceCell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum OnceCellState {
    Empty,
    Initializing,
    Initialized,
}

// ── Roam session snapshot types ──────────────────────────────────

/// Direction of an RPC request (serializable).
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum Direction {
    /// We sent the request, waiting for response.
    Outgoing,
    /// We received the request, processing it.
    Incoming,
}

/// Direction of a channel (serializable).
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ChannelDir {
    Tx,
    Rx,
}

/// Snapshot of a roam channel with enriched diagnostic data.
#[derive(Debug, Clone, Facet)]
pub struct RoamChannelSnapshot {
    pub channel_id: u64,
    pub name: String,
    pub direction: ChannelDir,
    pub age_secs: f64,
    pub request_id: Option<u64>,
    pub task_id: Option<u64>,
    pub task_name: Option<String>,
    pub queue_depth: Option<u64>,
    pub closed: bool,
}

/// Snapshot of all roam-session diagnostic state.
#[derive(Debug, Clone, Facet)]
pub struct SessionSnapshot {
    pub connections: Vec<ConnectionSnapshot>,
    pub method_names: HashMap<u64, String>,
    /// Enriched channel snapshots with task association and queue depth.
    pub channel_details: Vec<RoamChannelSnapshot>,
}

/// Snapshot of a single connection's diagnostic state.
#[derive(Debug, Clone, Facet)]
pub struct ConnectionSnapshot {
    pub name: String,
    pub peer_name: Option<String>,
    pub age_secs: f64,
    pub total_completed: u64,
    pub max_concurrent_requests: u32,
    pub initial_credit: u32,
    pub in_flight: Vec<RequestSnapshot>,
    pub recent_completions: Vec<CompletionSnapshot>,
    pub channels: Vec<ChannelSnapshot>,
    pub transport: TransportStats,
    pub channel_credits: Vec<ChannelCreditSnapshot>,
}

/// Snapshot of an in-flight RPC request.
#[derive(Debug, Clone, Facet)]
pub struct RequestSnapshot {
    pub request_id: u64,
    pub method_name: Option<String>,
    pub method_id: u64,
    pub direction: Direction,
    pub elapsed_secs: f64,
    pub task_id: Option<u64>,
    pub task_name: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    pub args: Option<HashMap<String, String>>,
    pub backtrace: Option<String>,
    pub server_task_id: Option<u64>,
    pub server_task_name: Option<String>,
}

/// Snapshot of a recently completed RPC request.
#[derive(Debug, Clone, Facet)]
pub struct CompletionSnapshot {
    pub method_name: Option<String>,
    pub method_id: u64,
    pub direction: Direction,
    pub duration_secs: f64,
    pub age_secs: f64,
}

/// Snapshot of an open channel.
#[derive(Debug, Clone, Facet)]
pub struct ChannelSnapshot {
    pub channel_id: u64,
    pub direction: ChannelDir,
    pub age_secs: f64,
    pub request_id: Option<u64>,
}

/// Transport-level statistics for a connection.
#[derive(Debug, Clone, Facet)]
pub struct TransportStats {
    pub frames_sent: u64,
    pub frames_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub last_sent_ago_secs: Option<f64>,
    pub last_recv_ago_secs: Option<f64>,
}

/// Per-channel flow control credit snapshot.
#[derive(Debug, Clone, Facet)]
pub struct ChannelCreditSnapshot {
    pub channel_id: u64,
    pub incoming_credit: u32,
    pub outgoing_credit: u32,
}

// ── Roam SHM snapshot types ─────────────────────────────────────

/// Snapshot of all SHM diagnostic state.
#[derive(Debug, Clone, Facet)]
pub struct ShmSnapshot {
    pub segments: Vec<ShmSegmentSnapshot>,
    pub channels: Vec<ChannelQueueSnapshot>,
}

/// Snapshot of a single SHM segment.
#[derive(Debug, Clone, Facet)]
pub struct ShmSegmentSnapshot {
    pub segment_path: Option<String>,
    pub total_size: u64,
    pub current_size: u64,
    pub max_peers: u32,
    pub host_goodbye: bool,
    pub peers: Vec<ShmPeerSnapshot>,
    pub var_pool: Vec<VarSlotClassSnapshot>,
}

/// Snapshot of a single SHM peer.
#[derive(Debug, Clone, Facet)]
pub struct ShmPeerSnapshot {
    pub peer_id: u32,
    pub state: ShmPeerState,
    pub name: Option<String>,
    pub bipbuf_capacity: u32,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub calls_sent: u64,
    pub calls_received: u64,
    pub time_since_heartbeat_ms: Option<u64>,
}

/// SHM peer state (serializable).
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ShmPeerState {
    Empty,
    Reserved,
    Attached,
    Goodbye,
    Unknown,
}

/// Snapshot of a var slot pool size class.
#[derive(Debug, Clone, Facet)]
pub struct VarSlotClassSnapshot {
    pub slot_size: u32,
    pub slots_per_extent: u32,
    pub extent_count: u32,
    pub free_slots_approx: u32,
    pub total_slots: u32,
}

/// Snapshot of an auditable channel queue.
#[derive(Debug, Clone, Facet)]
pub struct ChannelQueueSnapshot {
    pub name: String,
    pub len: u64,
    pub capacity: u64,
}

// ── Diagnostics enum + inventory ─────────────────────────────────

/// A diagnostic snapshot from any registered source.
pub enum Diagnostics {
    RoamSession(SessionSnapshot),
    RoamShm(ShmSnapshot),
}

/// A registered diagnostics source.
///
/// Libraries call `inventory::submit!` with one of these to register
/// their snapshot collection function.
pub struct DiagnosticsSource {
    pub collect: fn() -> Diagnostics,
}

inventory::collect!(DiagnosticsSource);

/// Collect diagnostics from all registered sources.
pub fn collect_all_diagnostics() -> Vec<Diagnostics> {
    inventory::iter::<DiagnosticsSource>
        .into_iter()
        .map(|source| (source.collect)())
        .collect()
}

// ── Future causality edge types ──────────────────────────────────

/// Future-to-future spawn/composition lineage.
#[derive(Debug, Clone, Facet)]
pub struct FutureSpawnEdgeSnapshot {
    pub parent_future_id: FutureId,
    pub parent_resource: String,
    pub child_future_id: FutureId,
    pub child_resource: String,
    pub created_by_task_id: Option<TaskId>,
    pub created_by_task_name: Option<String>,
    pub created_age_secs: f64,
}

/// Task polling a future (ownership over time).
#[derive(Debug, Clone, Facet)]
pub struct FuturePollEdgeSnapshot {
    pub task_id: TaskId,
    pub task_name: Option<String>,
    pub future_id: FutureId,
    pub future_resource: String,
    pub poll_count: u64,
    pub total_poll_secs: f64,
    pub last_poll_age_secs: f64,
}

/// Future explicitly resuming/waking a task.
#[derive(Debug, Clone, Facet)]
pub struct FutureResumeEdgeSnapshot {
    pub future_id: FutureId,
    pub future_resource: String,
    pub target_task_id: TaskId,
    pub target_task_name: Option<String>,
    pub resume_count: u64,
    pub last_resume_age_secs: f64,
}

/// Direction of a socket wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum SocketWaitDirection {
    Readable,
    Writable,
}

/// Structured resource identity for future waits.
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ResourceRefSnapshot {
    Lock { process: String, name: String },
    Mpsc { process: String, name: String },
    Oneshot { process: String, name: String },
    Watch { process: String, name: String },
    Semaphore { process: String, name: String },
    OnceCell { process: String, name: String },
    RoamChannel { process: String, channel_id: u64 },
    Socket {
        process: String,
        fd: u64,
        label: Option<String>,
        direction: Option<SocketWaitDirection>,
        peer: Option<String>,
    },
    Unknown { label: String },
}

/// Future waiting on a structured resource.
#[derive(Debug, Clone, Facet)]
pub struct FutureResourceEdgeSnapshot {
    pub future_id: FutureId,
    pub resource: ResourceRefSnapshot,
    pub wait_count: u64,
    pub total_wait_secs: f64,
    pub last_wait_age_secs: f64,
}

/// Explicit cross-process request parent edge.
#[derive(Debug, Clone, Facet)]
pub struct RequestParentSnapshot {
    pub child_process: String,
    pub child_connection: String,
    pub child_request_id: u64,
    pub parent_process: String,
    pub parent_connection: String,
    pub parent_request_id: u64,
}

// ── Deadlock candidate types ─────────────────────────────────────

/// Severity level for a deadlock candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum DeadlockSeverity {
    Warn,
    Danger,
}

/// A node in a deadlock cycle path.
#[derive(Debug, Clone, Facet)]
pub struct CycleNode {
    /// Display label for this node (task name, lock name, etc.).
    pub label: String,
    /// The kind of resource ("task", "lock", "channel", "rpc", "thread").
    pub kind: String,
    /// Process that owns this node.
    pub process: String,
    /// Task ID if this node is a task.
    pub task_id: Option<TaskId>,
}

/// Confidence level of a cycle edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CycleEdgeConfidence {
    /// Emitted directly by instrumentation.
    Explicit,
    /// Computed or inferred from other data.
    Derived,
    /// Guess-level correlation.
    Heuristic,
}

/// An edge in a deadlock cycle path.
#[derive(Debug, Clone, Facet)]
pub struct CycleEdge {
    /// Index into the candidate's `cycle_path` for the source node.
    pub from_node: u32,
    /// Index into the candidate's `cycle_path` for the target node.
    pub to_node: u32,
    /// Human-readable explanation ("task A waits on lock L held by task B").
    pub explanation: String,
    /// How long this edge has been waiting.
    pub wait_secs: f64,
    /// Confidence level of this edge.
    pub confidence: CycleEdgeConfidence,
}

/// A deadlock candidate: a cycle or near-cycle in the wait graph.
#[derive(Debug, Clone, Facet)]
pub struct DeadlockCandidate {
    /// Unique ID for this candidate within the snapshot.
    pub id: u32,
    /// Severity based on ranking signals.
    pub severity: DeadlockSeverity,
    /// Severity score (higher = worse).
    pub score: f64,
    /// Short title summarizing the deadlock.
    pub title: String,
    /// Nodes involved in the cycle.
    pub cycle_path: Vec<CycleNode>,
    /// Edges forming the cycle with per-edge explanations.
    pub cycle_edges: Vec<CycleEdge>,
    /// Human-readable rationale strings explaining why this was flagged.
    pub rationale: Vec<String>,
    /// Whether this cycle spans multiple processes.
    pub cross_process: bool,
    /// Worst wait duration among all edges.
    pub worst_wait_secs: f64,
    /// Number of tasks transitively blocked by this cycle.
    pub blocked_task_count: u32,
}

// ── Dashboard payload ────────────────────────────────────────────

/// Top-level payload sent to the dashboard.
#[derive(Debug, Clone, Facet)]
pub struct DashboardPayload {
    pub dumps: Vec<ProcessDump>,
    pub deadlock_candidates: Vec<DeadlockCandidate>,
}

// ── Process dump ─────────────────────────────────────────────────

/// Per-process diagnostic dump.
#[derive(Debug, Clone, Facet)]
pub struct ProcessDump {
    pub process_name: String,
    pub pid: u32,
    pub timestamp: String,
    pub tasks: Vec<TaskSnapshot>,
    pub wake_edges: Vec<WakeEdgeSnapshot>,
    pub future_wake_edges: Vec<FutureWakeEdgeSnapshot>,
    pub future_waits: Vec<FutureWaitSnapshot>,
    pub threads: Vec<ThreadStackSnapshot>,
    pub locks: Option<LockSnapshot>,
    pub sync: Option<SyncSnapshot>,
    pub roam: Option<SessionSnapshot>,
    pub shm: Option<ShmSnapshot>,
    pub future_spawn_edges: Vec<FutureSpawnEdgeSnapshot>,
    pub future_poll_edges: Vec<FuturePollEdgeSnapshot>,
    pub future_resume_edges: Vec<FutureResumeEdgeSnapshot>,
    pub future_resource_edges: Vec<FutureResourceEdgeSnapshot>,
    pub request_parents: Vec<RequestParentSnapshot>,
    pub custom: HashMap<String, String>,
}
