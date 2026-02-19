use facet::Facet;

/// API response for connected processes.
#[derive(Facet)]
pub struct ConnectionsResponse {
    pub connected_processes: usize,
    pub processes: Vec<ConnectedProcessInfo>,
}

#[derive(Facet)]
pub struct ConnectedProcessInfo {
    pub conn_id: u64,
    pub process_name: String,
    pub pid: u32,
}

#[derive(Facet)]
pub struct TriggerCutResponse {
    pub cut_id: String,
    pub requested_at_ns: i64,
    pub requested_connections: usize,
}

#[derive(Facet)]
pub struct CutStatusResponse {
    pub cut_id: String,
    pub requested_at_ns: i64,
    pub pending_connections: usize,
    pub acked_connections: usize,
    pub pending_conn_ids: Vec<u64>,
}

#[derive(Facet)]
pub struct ApiError {
    pub error: String,
}

#[derive(Facet)]
pub struct SqlRequest {
    pub sql: String,
}

#[derive(Facet)]
pub struct QueryRequest {
    pub name: String,
    #[facet(skip_unless_truthy)]
    pub limit: Option<u32>,
}

#[derive(Facet)]
pub struct SqlResponse {
    pub columns: Vec<String>,
    pub rows: Vec<facet_value::Value>,
    pub row_count: u32,
}

/// Top-level response for `/api/snapshot`.
#[derive(Facet)]
pub struct SnapshotCutResponse {
    /// Wall-clock milliseconds (Unix epoch) when this cut was assembled server-side.
    pub captured_at_unix_ms: i64,
    /// Processes that replied within the timeout window.
    pub processes: Vec<ProcessSnapshotView>,
    /// Processes connected at request time but timed out before response.
    pub timed_out_processes: Vec<TimedOutProcess>,
}

/// Per-process envelope inside a snapshot cut.
#[derive(Facet)]
pub struct ProcessSnapshotView {
    pub process_id: u64,
    pub process_name: String,
    pub pid: u32,
    pub ptime_now_ms: u64,
    pub snapshot: crate::Snapshot,
    #[facet(default)]
    pub scope_entity_links: Vec<ScopeEntityLink>,
}

#[derive(Facet)]
pub struct ScopeEntityLink {
    pub scope_id: String,
    pub entity_id: String,
}

#[derive(Facet)]
pub struct TimedOutProcess {
    pub process_id: u64,
    pub process_name: String,
    pub pid: u32,
}

#[derive(Facet)]
pub struct RecordStartRequest {
    pub interval_ms: Option<u32>,
    pub max_frames: Option<u32>,
    pub max_memory_bytes: Option<u64>,
}

#[derive(Facet)]
pub struct RecordCurrentResponse {
    pub session: Option<RecordingSessionInfo>,
}

#[derive(Facet)]
pub struct RecordingSessionInfo {
    pub session_id: String,
    pub status: String,
    pub interval_ms: u32,
    pub started_at_unix_ms: i64,
    pub stopped_at_unix_ms: Option<i64>,
    pub frame_count: u32,
    pub max_frames: u32,
    pub max_memory_bytes: u64,
    pub overflowed: bool,
    pub approx_memory_bytes: u64,
    pub avg_capture_ms: f64,
    pub max_capture_ms: f64,
    pub total_capture_ms: f64,
    pub frames: Vec<FrameSummary>,
}

#[derive(Facet)]
pub struct FrameSummary {
    pub frame_index: u32,
    pub captured_at_unix_ms: i64,
    pub process_count: u32,
    pub capture_duration_ms: f64,
}

#[derive(Facet)]
pub struct RecordingImportFrame {
    pub frame_index: u32,
    pub snapshot: facet_value::Value,
}

#[derive(Facet)]
pub struct RecordingImportBody {
    pub version: u32,
    pub session: RecordingSessionInfo,
    pub frames: Vec<RecordingImportFrame>,
}
