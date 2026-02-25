use crate::{ConnectionId, CutId, ProcessId, SessionId};
use facet::Facet;
use moire_trace_types::{BacktraceId, FrameId, RelPc};

/// API response for connected processes.
#[derive(Facet)]
pub struct ConnectionsResponse {
    pub connected_processes: usize,
    pub processes: Vec<ConnectedProcessInfo>,
}

#[derive(Facet)]
pub struct ConnectedProcessInfo {
    pub conn_id: ConnectionId,
    pub process_id: ProcessId,
    pub process_name: String,
    pub pid: u32,
}

#[derive(Facet)]
pub struct TriggerCutResponse {
    pub cut_id: CutId,
    pub requested_at_ns: i64,
    pub requested_connections: usize,
}

#[derive(Facet)]
pub struct CutStatusResponse {
    pub cut_id: CutId,
    pub requested_at_ns: i64,
    pub pending_connections: usize,
    pub acked_connections: usize,
    pub pending_conn_ids: Vec<ConnectionId>,
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
    /// Monotonic server-side snapshot id for correlating stream updates.
    pub snapshot_id: i64,
    /// Wall-clock milliseconds (Unix epoch) when this cut was assembled server-side.
    pub captured_at_unix_ms: i64,
    /// Processes that replied within the timeout window.
    pub processes: Vec<ProcessSnapshotView>,
    /// Processes connected at request time but timed out before response.
    pub timed_out_processes: Vec<TimedOutProcess>,
    /// Backtraces referenced by entities/scopes/edges/events in this snapshot.
    pub backtraces: Vec<SnapshotBacktrace>,
    /// Deduplicated frame catalog keyed by frame_id.
    pub frames: Vec<SnapshotFrameRecord>,
}

#[derive(Facet, Clone, Debug)]
pub struct SnapshotBacktrace {
    pub backtrace_id: BacktraceId,
    pub frame_ids: Vec<FrameId>,
}

#[derive(Facet, Clone, Debug)]
pub struct SnapshotFrameRecord {
    pub frame_id: FrameId,
    pub frame: SnapshotBacktraceFrame,
}

#[derive(Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum SnapshotBacktraceFrame {
    Resolved(BacktraceFrameResolved),
    Unresolved(BacktraceFrameUnresolved),
}

#[derive(Facet, Clone, Debug, PartialEq, Eq)]
pub struct BacktraceFrameResolved {
    pub module_path: String,
    pub function_name: String,
    pub source_file: String,
    pub line: Option<u32>,
}

#[derive(Facet, Clone, Debug, PartialEq, Eq)]
pub struct BacktraceFrameUnresolved {
    pub module_path: String,
    pub rel_pc: RelPc,
    pub reason: String,
}

#[derive(Facet, Clone, Debug)]
pub struct SnapshotSymbolicationUpdate {
    pub snapshot_id: i64,
    pub total_frames: u32,
    pub completed_frames: u32,
    pub done: bool,
    pub updated_frames: Vec<SnapshotFrameRecord>,
}

/// Per-process envelope inside a snapshot cut.
#[derive(Facet)]
pub struct ProcessSnapshotView {
    pub process_id: ProcessId,
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
    pub process_id: ProcessId,
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
    pub session_id: SessionId,
    pub status: RecordingSessionStatus,
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

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum RecordingSessionStatus {
    Recording,
    Stopped,
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

/// Response for `GET /api/source/preview`.
// r[impl api.source.preview]
#[derive(Facet)]
pub struct SourcePreviewResponse {
    pub frame_id: FrameId,
    pub source_file: String,
    pub target_line: u32,
    #[facet(skip_unless_truthy)]
    pub target_col: Option<u32>,
    pub total_lines: u32,
    /// Full arborium-highlighted HTML for the entire file.
    /// The frontend splits this into per-line strings using splitHighlightedHtml.
    pub html: String,
    /// Highlighted HTML for the cut scope excerpt (function/impl with cuts).
    /// When present, the frontend should prefer this over windowing into `html`.
    #[facet(skip_unless_truthy)]
    pub context_html: Option<String>,
    /// 1-based inclusive line range of the scope in the original file.
    /// Line 1 of context_html = line context_range.start in the original.
    #[facet(skip_unless_truthy)]
    pub context_range: Option<LineRange>,
    /// Single-line highlighted HTML of the target statement, whitespace-collapsed.
    /// Used for compact collapsed-frame display.
    #[facet(skip_unless_truthy)]
    pub context_line: Option<String>,
    /// Plain-text collapsed signature of the enclosing function/method.
    /// e.g. `"run()"` or `"SomeType::run(&self, config, handle)"`.
    /// Currently only populated for Rust source files.
    #[facet(skip_unless_truthy)]
    pub enclosing_fn: Option<String>,
}

/// Request body for `POST /api/source/previews`.
#[derive(Facet)]
pub struct SourcePreviewBatchRequest {
    pub frame_ids: Vec<FrameId>,
}

/// Response for `POST /api/source/previews`.
#[derive(Facet)]
pub struct SourcePreviewBatchResponse {
    pub previews: Vec<SourcePreviewResponse>,
    pub unavailable_frame_ids: Vec<FrameId>,
}

/// A 1-based inclusive line range within a source file.
#[derive(Facet, Debug)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Facet)]
pub struct RecordingImportBody {
    pub version: u32,
    pub session: RecordingSessionInfo,
    pub frames: Vec<RecordingImportFrame>,
}
