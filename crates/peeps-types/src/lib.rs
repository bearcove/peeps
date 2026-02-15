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
    Lock {
        process: String,
        name: String,
    },
    Mpsc {
        process: String,
        name: String,
    },
    Oneshot {
        process: String,
        name: String,
    },
    Watch {
        process: String,
        name: String,
    },
    Semaphore {
        process: String,
        name: String,
    },
    OnceCell {
        process: String,
        name: String,
    },
    RoamChannel {
        process: String,
        channel_id: u64,
    },
    Socket {
        process: String,
        fd: u64,
        label: Option<String>,
        direction: Option<SocketWaitDirection>,
        peer: Option<String>,
    },
    Unknown {
        label: String,
    },
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

// ── Canonical graph emission API (wrapper crates) ───────────────

/// Canonical node row emitted by instrumentation wrappers.
///
/// Common contract for all resources (`task`, `future`, `lock`,
/// `mpsc_tx`, `semaphore`, `oncecell`, `request`, `response`, etc.).
/// Type-specific fields belong in `attrs_json`.
/// Shared cross-resource context belongs in `attrs_json.meta`.
#[derive(Debug, Clone, Facet)]
pub struct GraphNodeSnapshot {
    /// Globally unique node ID within a snapshot.
    /// Format: `{kind}:{proc_key}:{resource_id_parts...}`
    pub id: String,
    /// Node kind (e.g. `task`, `future`, `lock`, `mpsc_tx`, `request`).
    pub kind: String,
    /// Human-readable process name.
    pub process: String,
    /// Opaque process key: `{process_slug}-{pid}`, charset `[a-z0-9._-]+`, no `:`.
    pub proc_key: String,
    /// Optional human-readable label.
    pub label: Option<String>,
    /// JSON-encoded type-specific attributes. Contains a `meta` sub-object
    /// for shared cross-resource metadata.
    pub attrs_json: String,
}

/// Edge provenance. Deliberately restricted to explicit measured data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum GraphEdgeOrigin {
    Explicit,
}

/// Canonical edge row emitted by instrumentation wrappers.
///
/// All edges use kind `"needs"`. No inferred/derived/heuristic edges.
#[derive(Debug, Clone, Facet)]
pub struct GraphEdgeSnapshot {
    /// Source node ID.
    pub src_id: String,
    /// Destination node ID.
    pub dst_id: String,
    /// Edge kind — always `"needs"` in v1.
    pub kind: String,
    /// Optional observation timestamp (nanos since process start or epoch).
    pub observed_at_ns: Option<u64>,
    /// JSON-encoded edge attributes (reserved for future use).
    pub attrs_json: String,
    /// Provenance marker.
    pub origin: GraphEdgeOrigin,
}

/// Per-process canonical graph snapshot envelope.
#[derive(Debug, Clone, Facet)]
pub struct GraphSnapshot {
    pub nodes: Vec<GraphNodeSnapshot>,
    pub edges: Vec<GraphEdgeSnapshot>,
}

impl GraphSnapshot {
    pub fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

/// Shared helper used by wrapper crates to emit canonical rows.
pub struct GraphSnapshotBuilder {
    graph: GraphSnapshot,
}

impl GraphSnapshotBuilder {
    pub fn new() -> Self {
        Self {
            graph: GraphSnapshot::empty(),
        }
    }

    pub fn push_node(&mut self, node: GraphNodeSnapshot) {
        self.graph.nodes.push(node);
    }

    pub fn push_edge(&mut self, edge: GraphEdgeSnapshot) {
        self.graph.edges.push(edge);
    }

    pub fn finish(self) -> GraphSnapshot {
        self.graph
    }
}

// ── Shared metadata system ──────────────────────────────────────

/// Maximum number of metadata pairs per node.
pub const META_MAX_PAIRS: usize = 16;
/// Maximum key length in bytes.
pub const META_MAX_KEY_LEN: usize = 48;
/// Maximum value length in bytes.
pub const META_MAX_VALUE_LEN: usize = 256;

/// Metadata value for the graph metadata system.
///
/// All variants serialize as strings in `attrs_json.meta`.
pub enum MetaValue<'a> {
    Static(&'static str),
    Str(&'a str),
    U64(u64),
    I64(i64),
    Bool(bool),
}

pub trait IntoMetaValue<'a> {
    fn into_meta_value(self) -> MetaValue<'a>;
}

impl<'a> IntoMetaValue<'a> for &'a str {
    fn into_meta_value(self) -> MetaValue<'a> {
        MetaValue::Str(self)
    }
}

impl IntoMetaValue<'_> for u64 {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::U64(self)
    }
}

impl IntoMetaValue<'_> for i64 {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::I64(self)
    }
}

impl IntoMetaValue<'_> for u32 {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::U64(self as u64)
    }
}

impl IntoMetaValue<'_> for usize {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::U64(self as u64)
    }
}

impl IntoMetaValue<'_> for bool {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::Bool(self)
    }
}

impl<'a> IntoMetaValue<'a> for MetaValue<'a> {
    fn into_meta_value(self) -> MetaValue<'a> {
        self
    }
}

impl MetaValue<'_> {
    /// Write this value as a string into the provided buffer.
    /// Returns the number of bytes written, or None if the buffer is too small.
    fn write_to(&self, buf: &mut [u8]) -> Option<usize> {
        use std::io::Write;
        match self {
            MetaValue::Static(s) | MetaValue::Str(s) => {
                let bytes = s.as_bytes();
                if bytes.len() > buf.len() {
                    return None;
                }
                buf[..bytes.len()].copy_from_slice(bytes);
                Some(bytes.len())
            }
            MetaValue::U64(v) => {
                let mut cursor = std::io::Cursor::new(&mut buf[..]);
                write!(cursor, "{v}").ok()?;
                Some(cursor.position() as usize)
            }
            MetaValue::I64(v) => {
                let mut cursor = std::io::Cursor::new(&mut buf[..]);
                write!(cursor, "{v}").ok()?;
                Some(cursor.position() as usize)
            }
            MetaValue::Bool(v) => {
                let s = if *v { "true" } else { "false" };
                let bytes = s.as_bytes();
                if bytes.len() > buf.len() {
                    return None;
                }
                buf[..bytes.len()].copy_from_slice(bytes);
                Some(bytes.len())
            }
        }
    }
}

/// Validate a metadata key: `[a-z0-9_.-]+`, max 48 bytes.
fn is_valid_meta_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= META_MAX_KEY_LEN
        && bytes.iter().all(|&b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.' || b == b'-'
        })
}

/// A validated metadata entry stored on the stack.
struct MetaEntry<'a> {
    key: &'a str,
    /// Value rendered as a string, stored in a stack buffer.
    value_buf: [u8; META_MAX_VALUE_LEN],
    value_len: usize,
}

/// Stack-based metadata builder for canonical graph nodes.
///
/// Validates keys/values per the spec and drops invalid pairs silently.
/// No heap allocation until `to_json_object()` is called.
pub struct MetaBuilder<'a, const N: usize = META_MAX_PAIRS> {
    entries: [std::mem::MaybeUninit<MetaEntry<'a>>; N],
    len: usize,
}

impl<'a, const N: usize> MetaBuilder<'a, N> {
    /// Create an empty metadata builder.
    pub fn new() -> Self {
        Self {
            // SAFETY: MaybeUninit doesn't require initialization
            entries: unsafe { std::mem::MaybeUninit::uninit().assume_init() },
            len: 0,
        }
    }

    /// Push a key-value pair. Invalid keys/values are silently dropped.
    pub fn push(&mut self, key: &'a str, value: MetaValue<'_>) -> &mut Self {
        if self.len >= N {
            return self;
        }
        if !is_valid_meta_key(key) {
            return self;
        }
        let mut value_buf = [0u8; META_MAX_VALUE_LEN];
        let Some(value_len) = value.write_to(&mut value_buf) else {
            return self;
        };
        if value_len > META_MAX_VALUE_LEN {
            return self;
        }
        self.entries[self.len] = std::mem::MaybeUninit::new(MetaEntry {
            key,
            value_buf,
            value_len,
        });
        self.len += 1;
        self
    }

    /// Serialize the metadata as a JSON object string: `{"key":"value",...}`.
    ///
    /// Returns an empty string if no entries are present.
    pub fn to_json_object(&self) -> String {
        if self.len == 0 {
            return String::new();
        }
        let mut out = String::with_capacity(self.len * 32);
        out.push('{');
        for i in 0..self.len {
            // SAFETY: entries[0..self.len] are initialized
            let entry = unsafe { self.entries[i].assume_init_ref() };
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            json_escape_into(&mut out, entry.key);
            out.push_str("\":\"");
            let value_str = std::str::from_utf8(&entry.value_buf[..entry.value_len]).unwrap_or("");
            json_escape_into(&mut out, value_str);
            out.push('"');
        }
        out.push('}');
        out
    }
}

/// Escape a string for JSON (handles `"`, `\`, and control chars).
pub fn json_escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

/// Build a [`MetaBuilder`] on the stack from key-value literal pairs.
///
/// When the `diagnostics` feature is disabled in wrapper crates, the
/// calling macro (`peepable_with_meta!`) should compile this away entirely.
///
/// ```ignore
/// use peeps_types::{peep_meta, MetaValue};
/// let meta = peep_meta! {
///     "request.id" => MetaValue::U64(42),
///     "request.method" => MetaValue::Static("GetUser"),
/// };
/// ```
#[macro_export]
macro_rules! peep_meta {
    ($($k:literal => $v:expr),* $(,)?) => {{
        const _COUNT: usize = $crate::peep_meta!(@count $($k),*);
        let mut builder = $crate::MetaBuilder::<_COUNT>::new();
        $(builder.push($k, $v);)*
        builder
    }};
    (@count $($k:literal),*) => {
        0usize $(+ { let _ = $k; 1usize })*
    };
}

// ── Canonical ID construction ───────────────────────────────────

/// Sanitize a string segment for use in canonical IDs.
///
/// Replaces any character not in `[a-z0-9._-]` with `-`.
/// Colons are forbidden in proc_key segments.
pub fn sanitize_id_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-' {
                c
            } else if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Construct a canonical `proc_key` from process name and PID.
///
/// Format: `{sanitized_process_name}-{pid}`
pub fn make_proc_key(process_name: &str, pid: u32) -> String {
    let slug = sanitize_id_segment(process_name);
    format!("{slug}-{pid}")
}

/// Canonical ID constructors for each node kind.
pub mod canonical_id {
    use super::sanitize_id_segment;

    pub fn task(proc_key: &str, task_id: u64) -> String {
        format!("task:{proc_key}:{task_id}")
    }

    pub fn future(proc_key: &str, future_id: u64) -> String {
        format!("future:{proc_key}:{future_id}")
    }

    pub fn request(proc_key: &str, connection: &str, request_id: u64) -> String {
        format!("request:{proc_key}:{connection}:{request_id}")
    }

    pub fn response(proc_key: &str, connection: &str, request_id: u64) -> String {
        format!("response:{proc_key}:{connection}:{request_id}")
    }

    pub fn lock(proc_key: &str, name: &str) -> String {
        let name = sanitize_id_segment(name);
        format!("lock:{proc_key}:{name}")
    }

    pub fn semaphore(proc_key: &str, name: &str) -> String {
        let name = sanitize_id_segment(name);
        format!("semaphore:{proc_key}:{name}")
    }

    pub fn mpsc(proc_key: &str, name: &str, endpoint: &str) -> String {
        let name = sanitize_id_segment(name);
        format!("mpsc:{proc_key}:{name}:{endpoint}")
    }

    pub fn oneshot(proc_key: &str, name: &str, endpoint: &str) -> String {
        let name = sanitize_id_segment(name);
        format!("oneshot:{proc_key}:{name}:{endpoint}")
    }

    pub fn watch(proc_key: &str, name: &str, endpoint: &str) -> String {
        let name = sanitize_id_segment(name);
        format!("watch:{proc_key}:{name}:{endpoint}")
    }

    pub fn roam_channel(proc_key: &str, channel_id: u64, endpoint: &str) -> String {
        format!("roam-channel:{proc_key}:{channel_id}:{endpoint}")
    }

    pub fn oncecell(proc_key: &str, name: &str) -> String {
        let name = sanitize_id_segment(name);
        format!("oncecell:{proc_key}:{name}")
    }

    /// Construct a sanitized connection token: `conn_{id}`.
    pub fn connection(id: u64) -> String {
        format!("conn_{id}")
    }

    /// Construct a correlation key for request/response pairing.
    pub fn correlation_key(connection: &str, request_id: u64) -> String {
        format!("{connection}:{request_id}")
    }
}

// ── Canonical metadata keys ─────────────────────────────────────

/// Well-known metadata keys for `attrs_json.meta`.
pub mod meta_key {
    pub const REQUEST_ID: &str = "request.id";
    pub const REQUEST_METHOD: &str = "request.method";
    pub const REQUEST_CORRELATION_KEY: &str = "request.correlation_key";
    pub const RPC_CONNECTION: &str = "rpc.connection";
    pub const RPC_PEER: &str = "rpc.peer";
    pub const TASK_ID: &str = "task.id";
    pub const FUTURE_ID: &str = "future.id";
    pub const CHANNEL_ID: &str = "channel.id";
    pub const RESOURCE_PATH: &str = "resource.path";
    pub const CTX_MODULE_PATH: &str = "ctx.module_path";
    pub const CTX_FILE: &str = "ctx.file";
    pub const CTX_LINE: &str = "ctx.line";
    pub const CTX_CRATE_NAME: &str = "ctx.crate_name";
    pub const CTX_CRATE_VERSION: &str = "ctx.crate_version";
    pub const CTX_CALLSITE: &str = "ctx.callsite";
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

// ── Snapshot protocol types ──────────────────────────────────────

/// Server-to-client: request a snapshot.
#[derive(Debug, Clone, Facet)]
pub struct SnapshotRequest {
    pub r#type: String,
    pub snapshot_id: i64,
    pub timeout_ms: i64,
}

/// Client-to-server: snapshot reply envelope.
#[derive(Debug, Clone, Facet)]
pub struct SnapshotReply {
    pub r#type: String,
    pub snapshot_id: i64,
    pub process: String,
    pub pid: u32,
    pub dump: ProcessDump,
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
    pub graph: Option<GraphSnapshot>,
    pub custom: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_builder_basic() {
        let mut mb = MetaBuilder::<16>::new();
        mb.push("task.id", MetaValue::U64(42));
        mb.push("request.method", MetaValue::Static("get_page"));
        mb.push("rpc.peer", MetaValue::Str("backend-1"));
        let json = mb.to_json_object();
        assert!(json.contains("\"task.id\":\"42\""));
        assert!(json.contains("\"request.method\":\"get_page\""));
        assert!(json.contains("\"rpc.peer\":\"backend-1\""));
    }

    #[test]
    fn meta_builder_empty() {
        let mb = MetaBuilder::<16>::new();
        assert_eq!(mb.to_json_object(), "");
    }

    #[test]
    fn meta_builder_drops_invalid_key() {
        let mut mb = MetaBuilder::<16>::new();
        mb.push("UPPER_CASE", MetaValue::Static("nope"));
        mb.push("has space", MetaValue::Static("nope"));
        mb.push("has:colon", MetaValue::Static("nope"));
        mb.push("", MetaValue::Static("nope"));
        assert_eq!(mb.to_json_object(), "");
    }

    #[test]
    fn meta_builder_drops_overflow() {
        let mut mb = MetaBuilder::<2>::new();
        mb.push("a", MetaValue::Static("1"));
        mb.push("b", MetaValue::Static("2"));
        mb.push("c", MetaValue::Static("3")); // dropped
        let json = mb.to_json_object();
        assert!(json.contains("\"a\":\"1\""));
        assert!(json.contains("\"b\":\"2\""));
        assert!(!json.contains("\"c\""));
    }

    #[test]
    fn meta_builder_bool_and_i64() {
        let mut mb = MetaBuilder::<16>::new();
        mb.push("flag", MetaValue::Bool(true));
        mb.push("offset", MetaValue::I64(-7));
        let json = mb.to_json_object();
        assert!(json.contains("\"flag\":\"true\""));
        assert!(json.contains("\"offset\":\"-7\""));
    }

    #[test]
    fn meta_builder_escapes_json() {
        let mut mb = MetaBuilder::<16>::new();
        mb.push("path", MetaValue::Str("a\"b\\c"));
        let json = mb.to_json_object();
        assert!(json.contains(r#""path":"a\"b\\c""#));
    }

    #[test]
    fn is_valid_meta_key_cases() {
        assert!(is_valid_meta_key("task.id"));
        assert!(is_valid_meta_key("request.correlation_key"));
        assert!(is_valid_meta_key("a-b"));
        assert!(is_valid_meta_key("abc123"));
        assert!(!is_valid_meta_key(""));
        assert!(!is_valid_meta_key("ABC"));
        assert!(!is_valid_meta_key("has space"));
        assert!(!is_valid_meta_key("has:colon"));
        assert!(!is_valid_meta_key(&"a".repeat(49)));
        assert!(is_valid_meta_key(&"a".repeat(48)));
    }

    #[test]
    fn into_meta_value_str() {
        let s = "hello";
        let mv: MetaValue = s.into_meta_value();
        let mut buf = [0u8; 256];
        let n = mv.write_to(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello");
    }

    #[test]
    fn into_meta_value_u64() {
        let mv: MetaValue = 42u64.into_meta_value();
        let mut buf = [0u8; 256];
        let n = mv.write_to(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"42");
    }

    #[test]
    fn into_meta_value_i64() {
        let mv: MetaValue = (-10i64).into_meta_value();
        let mut buf = [0u8; 256];
        let n = mv.write_to(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"-10");
    }

    #[test]
    fn into_meta_value_u32() {
        let mv: MetaValue = 99u32.into_meta_value();
        match mv {
            MetaValue::U64(v) => assert_eq!(v, 99),
            _ => panic!("expected U64"),
        }
    }

    #[test]
    fn into_meta_value_usize() {
        let mv: MetaValue = 7usize.into_meta_value();
        match mv {
            MetaValue::U64(v) => assert_eq!(v, 7),
            _ => panic!("expected U64"),
        }
    }

    #[test]
    fn into_meta_value_bool() {
        let mv: MetaValue = true.into_meta_value();
        match mv {
            MetaValue::Bool(v) => assert!(v),
            _ => panic!("expected Bool"),
        }
    }

    #[test]
    fn into_meta_value_passthrough() {
        let original = MetaValue::Static("pass");
        let mv = original.into_meta_value();
        match mv {
            MetaValue::Static(s) => assert_eq!(s, "pass"),
            _ => panic!("expected Static"),
        }
    }

    #[test]
    fn sanitize_id_segment_cases() {
        assert_eq!(sanitize_id_segment("Hello World!"), "hello-world-");
        assert_eq!(sanitize_id_segment("my-app_v2.0"), "my-app_v2.0");
        assert_eq!(sanitize_id_segment("foo:bar"), "foo-bar");
    }

    #[test]
    fn make_proc_key_formats() {
        assert_eq!(make_proc_key("MyApp", 1234), "myapp-1234");
        assert_eq!(make_proc_key("web-server", 42), "web-server-42");
    }

    #[test]
    fn canonical_ids() {
        let pk = "myapp-1234";
        assert_eq!(canonical_id::task(pk, 5), "task:myapp-1234:5");
        assert_eq!(canonical_id::future(pk, 10), "future:myapp-1234:10");
        assert_eq!(
            canonical_id::request(pk, "conn_3", 7),
            "request:myapp-1234:conn_3:7"
        );
        assert_eq!(
            canonical_id::response(pk, "conn_3", 7),
            "response:myapp-1234:conn_3:7"
        );
        assert_eq!(canonical_id::lock(pk, "my_lock"), "lock:myapp-1234:my_lock");
        assert_eq!(
            canonical_id::mpsc(pk, "queue", "tx"),
            "mpsc:myapp-1234:queue:tx"
        );
        assert_eq!(canonical_id::connection(99), "conn_99");
        assert_eq!(canonical_id::correlation_key("conn_3", 7), "conn_3:7");
    }
}
