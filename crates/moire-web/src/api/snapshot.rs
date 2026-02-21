use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use moire_trace_types::FrameId;
use moire_types::{
    BacktraceFrameUnresolved, ProcessSnapshotView, SnapshotBacktraceFrame, SnapshotCutResponse,
    SnapshotFrameRecord, SnapshotSymbolicationUpdate, TimedOutProcess,
};
use moire_wire::{ServerMessage, SnapshotRequest, encode_server_message_default};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::app::{AppState, ConnectionId, SnapshotPending, SnapshotStreamState, remember_snapshot};
use crate::db::fetch_scope_entity_links_blocking;
use crate::snapshot::table::{
    collect_snapshot_backtrace_pairs, is_pending_frame, load_snapshot_backtrace_table,
};
use crate::symbolication::symbolicate_pending_frames_for_pairs;
use crate::util::http::{json_error, json_ok};
use crate::util::time::now_ms;

const SYMBOLICATION_STREAM_STALL_TICKS_LIMIT: u32 = 100;
const SYMBOLICATION_UNRESOLVED_STALLED: &str =
    "symbolication stalled: no progress before stream timeout";

pub async fn api_snapshot(State(state): State<AppState>) -> impl IntoResponse {
    info!("snapshot requested via API");
    json_ok(&take_snapshot_internal(&state).await)
}

pub async fn api_snapshot_symbolication_ws(
    State(state): State<AppState>,
    AxumPath(snapshot_id): AxumPath<i64>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| snapshot_symbolication_ws_task(state, snapshot_id, socket))
}

pub async fn api_snapshot_current(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot_json = {
        let guard = state.inner.lock().await;
        guard.last_snapshot_json.clone()
    };
    match snapshot_json {
        Some(body) => {
            info!("snapshot current requested: cache hit");
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
                body,
            )
                .into_response()
        }
        None => {
            info!("snapshot current requested: cache miss");
            json_error(StatusCode::NOT_FOUND, "no snapshot available")
        }
    }
}

async fn snapshot_symbolication_ws_task(state: AppState, snapshot_id: i64, mut socket: WebSocket) {
    let pairs = {
        let guard = state.inner.lock().await;
        guard
            .snapshot_streams
            .get(&snapshot_id)
            .map(|entry| entry.pairs.clone())
    };
    let Some(pairs) = pairs else {
        warn!(
            snapshot_id,
            "symbolication stream requested for unknown snapshot id"
        );
        let update = SnapshotSymbolicationUpdate {
            snapshot_id,
            total_frames: 0,
            completed_frames: 0,
            done: true,
            updated_frames: vec![],
        };
        if let Ok(payload) = facet_json::to_string(&update) {
            let _ = socket.send(Message::Text(payload.into())).await;
        }
        return;
    };

    info!(
        snapshot_id,
        backtrace_pairs = pairs.len(),
        "symbolication stream opened"
    );
    let mut previous_frames: BTreeMap<FrameId, SnapshotBacktraceFrame> = BTreeMap::new();
    let mut previous_completed = 0usize;
    let mut unchanged_ticks = 0u32;

    loop {
        if let Err(e) = symbolicate_pending_frames_for_pairs(state.db.clone(), &pairs).await {
            warn!(snapshot_id, %e, "symbolication pass failed");
        }
        let table = load_snapshot_backtrace_table(state.db.clone(), &pairs).await;
        let completed = table
            .frames
            .iter()
            .filter(|record| !is_pending_frame(&record.frame))
            .count();
        let total = table.frames.len();

        let mut updated_frames = Vec::new();
        for record in &table.frames {
            match previous_frames.get(&record.frame_id) {
                Some(previous) if previous == &record.frame => {}
                _ => updated_frames.push(record.clone()),
            }
        }

        if !updated_frames.is_empty() || completed != previous_completed {
            unchanged_ticks = 0;
            let update = SnapshotSymbolicationUpdate {
                snapshot_id,
                total_frames: total as u32,
                completed_frames: completed as u32,
                done: completed == total,
                updated_frames,
            };
            let payload = match facet_json::to_string(&update) {
                Ok(payload) => payload,
                Err(e) => {
                    warn!(snapshot_id, %e, "failed to encode symbolication update");
                    break;
                }
            };
            if socket.send(Message::Text(payload.into())).await.is_err() {
                info!(snapshot_id, "symbolication stream client disconnected");
                break;
            }
            info!(
                snapshot_id,
                completed_frames = completed,
                total_frames = total,
                updated_frame_count = update.updated_frames.len(),
                done = update.done,
                "symbolication stream update sent"
            );
        } else {
            unchanged_ticks = unchanged_ticks.saturating_add(1);
            if unchanged_ticks.is_multiple_of(30) {
                info!(
                    snapshot_id,
                    completed_frames = completed,
                    total_frames = total,
                    unchanged_ticks,
                    "symbolication stream stalled waiting for more frame updates"
                );
            }
            if unchanged_ticks >= SYMBOLICATION_STREAM_STALL_TICKS_LIMIT {
                let forced_updates: Vec<SnapshotFrameRecord> = table
                    .frames
                    .iter()
                    .filter_map(|record| match &record.frame {
                        SnapshotBacktraceFrame::Unresolved(BacktraceFrameUnresolved {
                            module_path,
                            rel_pc,
                            reason,
                        }) if reason == "symbolication pending" => Some(SnapshotFrameRecord {
                            frame_id: record.frame_id,
                            frame: SnapshotBacktraceFrame::Unresolved(BacktraceFrameUnresolved {
                                module_path: module_path.clone(),
                                rel_pc: *rel_pc,
                                reason: String::from(SYMBOLICATION_UNRESOLVED_STALLED),
                            }),
                        }),
                        _ => None,
                    })
                    .collect();
                warn!(
                    snapshot_id,
                    total_frames = total,
                    completed_frames = completed,
                    forced_unresolved_frames = forced_updates.len(),
                    unchanged_ticks,
                    "symbolication stream forcing completion after prolonged stall"
                );
                let update = SnapshotSymbolicationUpdate {
                    snapshot_id,
                    total_frames: total as u32,
                    completed_frames: total as u32,
                    done: true,
                    updated_frames: forced_updates,
                };
                match facet_json::to_string(&update) {
                    Ok(payload) => {
                        let _ = socket.send(Message::Text(payload.into())).await;
                    }
                    Err(e) => {
                        warn!(
                            snapshot_id,
                            %e,
                            "failed to encode forced symbolication completion update"
                        );
                    }
                }
                break;
            }
        }

        previous_frames = table
            .frames
            .into_iter()
            .map(|record| (record.frame_id, record.frame))
            .collect();
        previous_completed = completed;

        if completed == total {
            info!(
                snapshot_id,
                total_frames = total,
                "symbolication stream complete"
            );
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub async fn take_snapshot_internal(state: &AppState) -> SnapshotCutResponse {
    const SNAPSHOT_TIMEOUT_MS: u64 = 5000;

    let snapshot_id;
    let notify;
    let txs: Vec<(ConnectionId, mpsc::Sender<Vec<u8>>)>;
    {
        let mut guard = state.inner.lock().await;
        snapshot_id = guard.next_snapshot_id;
        guard.next_snapshot_id += 1;

        txs = guard
            .connections
            .iter()
            .map(|(id, conn)| (*id, conn.tx.clone()))
            .collect();

        notify = Arc::new(tokio::sync::Notify::new());
        if !txs.is_empty() {
            let pending_conn_ids: BTreeSet<ConnectionId> = txs.iter().map(|(id, _)| *id).collect();
            guard.pending_snapshots.insert(
                snapshot_id,
                SnapshotPending {
                    pending_conn_ids,
                    replies: HashMap::new(),
                    notify: notify.clone(),
                },
            );
        }
    }
    info!(
        snapshot_id,
        requested_connections = txs.len(),
        "snapshot request fanout started"
    );

    if txs.is_empty() {
        let response = SnapshotCutResponse {
            snapshot_id,
            captured_at_unix_ms: now_ms(),
            processes: vec![],
            timed_out_processes: vec![],
            backtraces: vec![],
            frames: vec![],
        };
        let mut guard = state.inner.lock().await;
        guard
            .snapshot_streams
            .insert(snapshot_id, SnapshotStreamState { pairs: vec![] });
        drop(guard);
        remember_snapshot(state, &response).await;
        return response;
    }

    let request_frame =
        match encode_server_message_default(&ServerMessage::SnapshotRequest(SnapshotRequest {
            snapshot_id,
            timeout_ms: SNAPSHOT_TIMEOUT_MS as i64,
        })) {
            Ok(frame) => frame,
            Err(e) => {
                error!(%e, "encode snapshot request");
                state
                    .inner
                    .lock()
                    .await
                    .pending_snapshots
                    .remove(&snapshot_id);
                let response = SnapshotCutResponse {
                    snapshot_id,
                    captured_at_unix_ms: now_ms(),
                    processes: vec![],
                    timed_out_processes: vec![],
                    backtraces: vec![],
                    frames: vec![],
                };
                let mut guard = state.inner.lock().await;
                guard
                    .snapshot_streams
                    .insert(snapshot_id, SnapshotStreamState { pairs: vec![] });
                drop(guard);
                remember_snapshot(state, &response).await;
                return response;
            }
        };

    for (_, tx) in &txs {
        if let Err(e) = tx.try_send(request_frame.clone()) {
            tracing::debug!(%e, "failed to send snapshot request to connection");
        }
    }

    let _ = tokio::time::timeout(
        Duration::from_millis(SNAPSHOT_TIMEOUT_MS),
        notify.notified(),
    )
    .await;

    let captured_at_unix_ms = now_ms();
    let (pending, conn_info) = {
        let mut guard = state.inner.lock().await;
        let pending = guard.pending_snapshots.remove(&snapshot_id);
        let conn_info: HashMap<ConnectionId, (String, u32)> = guard
            .connections
            .iter()
            .map(|(id, conn)| (*id, (conn.process_name.clone(), conn.pid)))
            .collect();
        (pending, conn_info)
    };

    let (processes, timed_out_processes) = match pending {
        None => (vec![], vec![]),
        Some(p) => {
            let partial: Vec<(ConnectionId, String, u32, u64, moire_types::Snapshot)> = p
                .replies
                .into_iter()
                .filter_map(|(conn_id, reply)| {
                    let snapshot = reply.snapshot?;
                    let (process_name, pid) = conn_info
                        .get(&conn_id)
                        .map(|(name, pid)| (name.clone(), *pid))
                        .unwrap_or_else(|| (format!("unknown-{}", conn_id.get()), 0));
                    Some((conn_id, process_name, pid, reply.ptime_now_ms, snapshot))
                })
                .collect();

            let mut processes = Vec::with_capacity(partial.len());
            for (conn_id, process_name, pid, ptime_now_ms, snapshot) in partial {
                let db = state.db.clone();
                let scope_entity_links = tokio::task::spawn_blocking(move || {
                    fetch_scope_entity_links_blocking(&db, conn_id.get())
                })
                .await
                .unwrap_or_else(|e| {
                    warn!(%e, "scope_entity_links join error");
                    Ok(vec![])
                })
                .unwrap_or_else(|e| {
                    warn!(%e, "scope_entity_links query error");
                    vec![]
                });
                processes.push(ProcessSnapshotView {
                    process_id: conn_id,
                    process_name,
                    pid,
                    ptime_now_ms,
                    snapshot,
                    scope_entity_links,
                });
            }
            let processes = processes;

            let timed_out_processes = p
                .pending_conn_ids
                .into_iter()
                .map(|conn_id| {
                    let (process_name, pid) = conn_info
                        .get(&conn_id)
                        .map(|(name, pid)| (name.clone(), *pid))
                        .unwrap_or_else(|| (format!("unknown-{}", conn_id.get()), 0));
                    TimedOutProcess {
                        process_id: conn_id,
                        process_name,
                        pid,
                    }
                })
                .collect();

            (processes, timed_out_processes)
        }
    };

    let response = SnapshotCutResponse {
        snapshot_id,
        captured_at_unix_ms,
        processes,
        timed_out_processes,
        backtraces: vec![],
        frames: vec![],
    };
    info!(
        snapshot_id,
        process_count = response.processes.len(),
        timed_out_count = response.timed_out_processes.len(),
        "snapshot request completed"
    );
    let pairs = collect_snapshot_backtrace_pairs(&response);
    {
        let mut guard = state.inner.lock().await;
        guard.snapshot_streams.insert(
            snapshot_id,
            SnapshotStreamState {
                pairs: pairs.clone(),
            },
        );
    }
    info!(
        snapshot_id,
        backtrace_pairs = pairs.len(),
        "snapshot queued for symbolication stream"
    );
    remember_snapshot(state, &response).await;
    response
}
