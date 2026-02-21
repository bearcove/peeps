use std::sync::Arc;

use moire_types::{
    FrameSummary, RecordingImportBody, RecordingSessionInfo, RecordingSessionStatus, SessionId,
};
use tokio::sync::Notify;

#[derive(Clone)]
pub struct StoredFrame {
    pub frame_index: u32,
    pub captured_at_unix_ms: i64,
    pub process_count: u32,
    pub capture_duration_ms: f64,
    pub json: String,
}

pub struct RecordingState {
    pub session_id: SessionId,
    pub interval_ms: u32,
    pub started_at_unix_ms: i64,
    pub stopped_at_unix_ms: Option<i64>,
    pub frames: Vec<StoredFrame>,
    pub max_frames: u32,
    pub max_memory_bytes: u64,
    pub overflowed: bool,
    pub total_frames_captured: u32,
    pub approx_memory_bytes: u64,
    pub total_capture_ms: f64,
    pub max_capture_ms: f64,
    pub stop_signal: Arc<Notify>,
}

pub fn push_frame(
    recording: &mut RecordingState,
    captured_at_unix_ms: i64,
    process_count: u32,
    capture_duration_ms: f64,
    json: String,
) {
    if recording.frames.len() as u32 >= recording.max_frames {
        recording.overflowed = true;
        let dropped = recording.frames.remove(0);
        recording.approx_memory_bytes = recording
            .approx_memory_bytes
            .saturating_sub(dropped.json.len() as u64);
    }
    let frame_index = recording.total_frames_captured;
    recording.total_frames_captured += 1;
    recording.total_capture_ms += capture_duration_ms;
    if capture_duration_ms > recording.max_capture_ms {
        recording.max_capture_ms = capture_duration_ms;
    }
    let json_len = json.len() as u64;
    recording.frames.push(StoredFrame {
        frame_index,
        captured_at_unix_ms,
        process_count,
        capture_duration_ms,
        json,
    });
    recording.approx_memory_bytes += json_len;
    while recording.approx_memory_bytes > recording.max_memory_bytes && !recording.frames.is_empty()
    {
        recording.overflowed = true;
        let dropped = recording.frames.remove(0);
        recording.approx_memory_bytes = recording
            .approx_memory_bytes
            .saturating_sub(dropped.json.len() as u64);
    }
}

pub fn frame_json_by_index(recording: &RecordingState, frame_index: u32) -> Option<&str> {
    if recording.frames.is_empty() {
        return None;
    }
    let first_index = recording.frames[0].frame_index;
    if frame_index < first_index {
        return None;
    }
    let vec_index = (frame_index - first_index) as usize;
    recording
        .frames
        .get(vec_index)
        .map(|frame| frame.json.as_str())
}

pub fn recording_session_info(rec: &RecordingState) -> RecordingSessionInfo {
    let status = if rec.stopped_at_unix_ms.is_none() {
        RecordingSessionStatus::Recording
    } else {
        RecordingSessionStatus::Stopped
    };
    let avg_capture_ms = if rec.total_frames_captured > 0 {
        rec.total_capture_ms / rec.total_frames_captured as f64
    } else {
        0.0
    };
    let frames = rec
        .frames
        .iter()
        .map(|frame| FrameSummary {
            frame_index: frame.frame_index,
            captured_at_unix_ms: frame.captured_at_unix_ms,
            process_count: frame.process_count,
            capture_duration_ms: frame.capture_duration_ms,
        })
        .collect();
    RecordingSessionInfo {
        session_id: rec.session_id.clone(),
        status,
        interval_ms: rec.interval_ms,
        started_at_unix_ms: rec.started_at_unix_ms,
        stopped_at_unix_ms: rec.stopped_at_unix_ms,
        frame_count: rec.frames.len() as u32,
        max_frames: rec.max_frames,
        max_memory_bytes: rec.max_memory_bytes,
        overflowed: rec.overflowed,
        approx_memory_bytes: rec.approx_memory_bytes,
        avg_capture_ms,
        max_capture_ms: rec.max_capture_ms,
        total_capture_ms: rec.total_capture_ms,
        frames,
    }
}

pub fn build_imported_frames(import: &RecordingImportBody) -> Result<Vec<StoredFrame>, String> {
    let summary_by_index: std::collections::HashMap<u32, &FrameSummary> = import
        .session
        .frames
        .iter()
        .map(|frame| (frame.frame_index, frame))
        .collect();

    let mut frames: Vec<StoredFrame> = Vec::with_capacity(import.frames.len());
    for frame in &import.frames {
        let json = facet_json::to_string(&frame.snapshot).map_err(|error| {
            format!(
                "failed to re-serialize frame {}: {error}",
                frame.frame_index
            )
        })?;
        let summary = summary_by_index.get(&frame.frame_index);
        let captured_at_unix_ms = summary.map_or(0, |entry| entry.captured_at_unix_ms);
        let process_count = summary.map_or(0, |entry| entry.process_count);
        let capture_duration_ms = summary.map_or(0.0, |entry| entry.capture_duration_ms);
        frames.push(StoredFrame {
            frame_index: frame.frame_index,
            captured_at_unix_ms,
            process_count,
            capture_duration_ms,
            json,
        });
    }
    frames.sort_by_key(|frame| frame.frame_index);
    Ok(frames)
}

pub fn export_frame_rows(frames: &[StoredFrame]) -> Vec<String> {
    frames
        .iter()
        .map(|frame| {
            format!(
                r#"{{"frame_index":{},"snapshot":{}}}"#,
                frame.frame_index, frame.json
            )
        })
        .collect()
}
