//! Shared diagnostic snapshot types for peeps.
//!
//! All snapshot types live here so they can be used for both serialization
//! (producing dumps) and deserialization (reading dumps) without circular
//! dependencies between peeps subcrates and instrumented libraries.

use std::collections::HashMap;

use facet::Facet;

// ── Task snapshot types ──────────────────────────────────────────

/// Unique task ID.
pub type TaskId = u64;

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
}

/// A lock waiter.
#[derive(Debug, Clone, Facet)]
pub struct LockWaiterSnapshot {
    pub kind: LockAcquireKind,
    pub waiting_secs: f64,
    pub backtrace: Option<String>,
}

// ── Channel & sync snapshot types ───────────────────────────────

/// Snapshot of all tracked channels and sync primitives.
#[derive(Debug, Clone, Facet)]
pub struct SyncSnapshot {
    pub mpsc_channels: Vec<MpscChannelSnapshot>,
    pub oneshot_channels: Vec<OneshotChannelSnapshot>,
    pub watch_channels: Vec<WatchChannelSnapshot>,
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
}

/// Snapshot of a tracked oneshot channel.
#[derive(Debug, Clone, Facet)]
pub struct OneshotChannelSnapshot {
    pub name: String,
    pub state: OneshotState,
    pub age_secs: f64,
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

/// Snapshot of all roam-session diagnostic state.
#[derive(Debug, Clone, Facet)]
pub struct SessionSnapshot {
    pub connections: Vec<ConnectionSnapshot>,
    pub method_names: HashMap<u64, String>,
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
    pub args: Option<HashMap<String, String>>,
    pub backtrace: Option<String>,
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

// ── Process dump ─────────────────────────────────────────────────

/// Per-process diagnostic dump.
#[derive(Debug, Clone, Facet)]
pub struct ProcessDump {
    pub process_name: String,
    pub pid: u32,
    pub timestamp: String,
    pub tasks: Vec<TaskSnapshot>,
    pub threads: Vec<ThreadStackSnapshot>,
    pub locks: Option<LockSnapshot>,
    pub sync: Option<SyncSnapshot>,
    pub roam: Option<SessionSnapshot>,
    pub shm: Option<ShmSnapshot>,
    pub custom: HashMap<String, String>,
}
