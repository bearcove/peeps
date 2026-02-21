use crate::{ConnectionId, SessionId};
use facet::Facet;

/// Status of a recording session.
#[derive(Facet)]
#[repr(u8)]
pub enum RecordingStatus {
    Recording,
    Stopped,
}

/// Metadata for a recording session.
#[derive(Facet)]
pub struct RecordingSession {
    pub session_id: SessionId,
    pub status: RecordingStatus,
    pub interval_ms: u32,
    pub started_at_unix_ms: i64,
    pub stopped_at_unix_ms: Option<i64>,
    pub frame_count: u32,
    pub max_frames: u32,
    pub overflowed: bool,
    pub approx_memory_bytes: u64,
}

/// A single recorded frame — a point-in-time snapshot of all processes.
#[derive(Facet)]
pub struct RecordingFrame {
    pub frame_index: u32,
    pub captured_at_unix_ms: i64,
    pub processes: Vec<ProcessFrameView>,
}

/// Per-process data within a recording frame.
#[derive(Facet)]
pub struct ProcessFrameView {
    pub process_id: ConnectionId,
    pub process_name: String,
    pub pid: u32,
    pub ptime_now_ms: u64,
    pub snapshot: crate::Snapshot,
}
