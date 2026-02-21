use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use moire_types::{RecordCurrentResponse, RecordStartRequest, RecordingImportBody};
use tokio::sync::Notify;
use tracing::warn;

use crate::api::snapshot::take_snapshot_internal;
use crate::app::AppState;
use crate::recording::session::{
    RecordingState, build_imported_frames, export_frame_rows, frame_json_by_index, push_frame,
    recording_session_info,
};
use crate::util::http::{json_error, json_ok};
use crate::util::time::now_ms;

pub async fn api_record_start(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let req: RecordStartRequest = if body.is_empty() {
        RecordStartRequest {
            interval_ms: None,
            max_frames: None,
            max_memory_bytes: None,
        }
    } else {
        match facet_json::from_slice(&body) {
            Ok(req) => req,
            Err(e) => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    format!("invalid request json: {e}"),
                );
            }
        }
    };

    let (session_id, stop_signal) = {
        let mut guard = state.inner.lock().await;
        if guard
            .recording
            .as_ref()
            .is_some_and(|r| r.stopped_at_unix_ms.is_none())
        {
            return json_error(StatusCode::CONFLICT, "recording already in progress");
        }

        let session_num = guard.next_session_id;
        guard.next_session_id = guard.next_session_id.next();
        let session_id = session_num.to_session_id();
        let interval_ms = req.interval_ms.unwrap_or(500);
        let max_frames = req.max_frames.unwrap_or(1000);
        let max_memory_bytes = req.max_memory_bytes.unwrap_or(256 * 1024 * 1024);
        let stop_signal = Arc::new(Notify::new());

        guard.recording = Some(RecordingState {
            session_id: session_id.clone(),
            interval_ms,
            started_at_unix_ms: now_ms(),
            stopped_at_unix_ms: None,
            frames: Vec::new(),
            max_frames,
            max_memory_bytes,
            overflowed: false,
            total_frames_captured: 0,
            approx_memory_bytes: 0,
            total_capture_ms: 0.0,
            max_capture_ms: 0.0,
            stop_signal: stop_signal.clone(),
        });

        (session_id, stop_signal)
    };

    let loop_state = state.clone();
    let loop_session_id = session_id.clone();
    tokio::spawn(async move {
        let interval_ms = {
            let guard = loop_state.inner.lock().await;
            guard.recording.as_ref().map_or(500, |r| r.interval_ms)
        };
        loop {
            tokio::select! {
                _ = stop_signal.notified() => break,
                _ = tokio::time::sleep(Duration::from_millis(interval_ms as u64)) => {
                    let capture_start = Instant::now();
                    let snapshot = take_snapshot_internal(&loop_state).await;
                    let json = match facet_json::to_string(&snapshot) {
                        Ok(json) => json,
                        Err(e) => {
                            warn!(%e, "failed to serialize recording frame");
                            continue;
                        }
                    };
                    let capture_duration_ms = capture_start.elapsed().as_secs_f64() * 1000.0;
                    let process_count = snapshot.processes.len() as u32;
                    let captured_at_unix_ms = snapshot.captured_at_unix_ms;
                    let mut guard = loop_state.inner.lock().await;
                    let Some(recording) = &mut guard.recording else { break };
                    if recording.session_id != loop_session_id || recording.stopped_at_unix_ms.is_some() {
                        break;
                    }
                    push_frame(
                        recording,
                        captured_at_unix_ms,
                        process_count,
                        capture_duration_ms,
                        json,
                    );
                }
            }
        }
    });

    let guard = state.inner.lock().await;
    let rec = guard.recording.as_ref().unwrap();
    json_ok(&RecordCurrentResponse {
        session: Some(recording_session_info(rec)),
    })
}

pub async fn api_record_stop(State(state): State<AppState>) -> impl IntoResponse {
    let stop_signal = {
        let mut guard = state.inner.lock().await;
        match &mut guard.recording {
            None => return json_error(StatusCode::NOT_FOUND, "no recording in progress"),
            Some(rec) if rec.stopped_at_unix_ms.is_some() => {
                return json_error(StatusCode::NOT_FOUND, "no recording in progress");
            }
            Some(rec) => {
                rec.stopped_at_unix_ms = Some(now_ms());
                rec.stop_signal.clone()
            }
        }
    };

    stop_signal.notify_one();

    let guard = state.inner.lock().await;
    let rec = guard.recording.as_ref().unwrap();
    json_ok(&RecordCurrentResponse {
        session: Some(recording_session_info(rec)),
    })
}

pub async fn api_record_current(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let session = guard.recording.as_ref().map(recording_session_info);
    json_ok(&RecordCurrentResponse { session })
}

pub async fn api_record_frame(
    State(state): State<AppState>,
    AxumPath(frame_index): AxumPath<u32>,
) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let Some(recording) = &guard.recording else {
        return json_error(StatusCode::NOT_FOUND, "no recording");
    };
    let Some(frame_json) = frame_json_by_index(recording, frame_index) else {
        return json_error(StatusCode::NOT_FOUND, "frame not found");
    };
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        frame_json.to_string(),
    )
        .into_response()
}

pub async fn api_record_export(State(state): State<AppState>) -> impl IntoResponse {
    let (session_info, frames_json) = {
        let guard = state.inner.lock().await;
        let Some(recording) = &guard.recording else {
            return json_error(StatusCode::NOT_FOUND, "no recording");
        };
        if recording.stopped_at_unix_ms.is_none() {
            return json_error(StatusCode::CONFLICT, "recording is still in progress");
        }
        let session_info = recording_session_info(recording);
        let frames_json = export_frame_rows(&recording.frames);
        (session_info, frames_json)
    };

    let session_json = match facet_json::to_string(&session_info) {
        Ok(s) => s,
        Err(e) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to serialize session: {e}"),
            );
        }
    };

    let export_json = format!(
        r#"{{"version":1,"session":{},"frames":[{}]}}"#,
        session_json,
        frames_json.join(",")
    );

    let filename = format!(
        "recording-{}.json",
        session_info.session_id.as_str().replace(':', "_")
    );
    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        export_json,
    )
        .into_response();
    if let Ok(value) =
        header::HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
    {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

pub async fn api_record_import(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let import: RecordingImportBody = match facet_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, format!("invalid import json: {e}")),
    };

    if import.version != 1 {
        return json_error(
            StatusCode::BAD_REQUEST,
            format!("unsupported export version: {}", import.version),
        );
    }

    let frames = match build_imported_frames(&import) {
        Ok(frames) => frames,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
    };

    let approx_memory_bytes: u64 = frames.iter().map(|f| f.json.len() as u64).sum();
    let total_frames_captured = frames.len() as u32;

    let existing_stop_signal = {
        let mut guard = state.inner.lock().await;
        let existing_stop_signal = guard
            .recording
            .as_ref()
            .filter(|r| r.stopped_at_unix_ms.is_none())
            .map(|r| r.stop_signal.clone());

        guard.recording = Some(RecordingState {
            session_id: import.session.session_id.clone(),
            interval_ms: import.session.interval_ms,
            started_at_unix_ms: import.session.started_at_unix_ms,
            stopped_at_unix_ms: Some(import.session.stopped_at_unix_ms.unwrap_or_else(now_ms)),
            frames,
            max_frames: import.session.max_frames,
            max_memory_bytes: import.session.max_memory_bytes,
            overflowed: import.session.overflowed,
            total_frames_captured,
            approx_memory_bytes,
            total_capture_ms: import.session.total_capture_ms,
            max_capture_ms: import.session.max_capture_ms,
            stop_signal: Arc::new(Notify::new()),
        });

        existing_stop_signal
    };

    if let Some(sig) = existing_stop_signal {
        sig.notify_one();
    }

    let guard = state.inner.lock().await;
    let rec = guard.recording.as_ref().unwrap();
    json_ok(&RecordCurrentResponse {
        session: Some(recording_session_info(rec)),
    })
}
