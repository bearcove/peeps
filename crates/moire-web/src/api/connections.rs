use std::collections::BTreeSet;

use axum::extract::{Path as AxumPath, State};
use axum::response::IntoResponse;
use moire_types::{
    ConnectedProcessInfo, ConnectionsResponse, CutId, CutStatusResponse, TriggerCutResponse,
};
use moire_wire::{ServerMessage, encode_server_message_default};
use tracing::{error, info, warn};

use crate::app::{AppState, CutState};
use crate::db::persist_cut_request;
use crate::util::http::json_ok;
use crate::util::time::now_nanos;

pub async fn api_connections(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let mut processes: Vec<ConnectedProcessInfo> = guard
        .connections
        .iter()
        .map(|(conn_id, conn)| ConnectedProcessInfo {
            conn_id: conn_id.get(),
            process_name: conn.process_name.clone(),
            pid: conn.pid,
        })
        .collect();
    processes.sort_by(|a, b| {
        a.process_name
            .cmp(&b.process_name)
            .then_with(|| a.pid.cmp(&b.pid))
            .then_with(|| a.conn_id.cmp(&b.conn_id))
    });

    json_ok(&ConnectionsResponse {
        connected_processes: processes.len(),
        processes,
    })
}

pub async fn api_trigger_cut(State(state): State<AppState>) -> impl IntoResponse {
    let (cut_id, cut_id_string, now_ns, requested_connections, outbound) = {
        let mut guard = state.inner.lock().await;
        let cut_num = guard.next_cut_id;
        guard.next_cut_id = guard.next_cut_id.next();
        let cut_id = cut_num.to_cut_id();
        let cut_id_string = cut_id.0.clone();
        let now_ns = now_nanos();
        let mut pending_conn_ids = BTreeSet::new();
        let mut outbound = Vec::new();
        for (conn_id, conn) in &guard.connections {
            pending_conn_ids.insert(*conn_id);
            outbound.push((*conn_id, conn.tx.clone()));
        }

        guard.cuts.insert(
            cut_id.clone(),
            CutState {
                requested_at_ns: now_ns,
                pending_conn_ids,
                acks: std::collections::BTreeMap::new(),
            },
        );

        (cut_id, cut_id_string, now_ns, outbound.len(), outbound)
    };

    let request = ServerMessage::CutRequest(moire_types::CutRequest {
        cut_id: cut_id.clone(),
    });
    info!(
        cut_id = %cut_id_string,
        requested_connections,
        "cut requested via API"
    );
    if let Err(e) = persist_cut_request(state.db.clone(), cut_id_string.clone(), now_ns).await {
        error!(%e, cut_id = %cut_id_string, "failed to persist cut request");
    }
    let payload = match encode_server_message_default(&request) {
        Ok(payload) => payload,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to encode cut request: {e}"),
            )
                .into_response();
        }
    };

    for (conn_id, tx) in outbound {
        if let Err(e) = tx.try_send(payload.clone()) {
            warn!(conn_id = conn_id.get(), %e, "failed to enqueue cut request");
        }
    }

    json_ok(&TriggerCutResponse {
        cut_id,
        requested_at_ns: now_ns,
        requested_connections,
    })
}

pub async fn api_cut_status(
    State(state): State<AppState>,
    AxumPath(cut_id): AxumPath<String>,
) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let cut_id = CutId(cut_id);
    let Some(cut) = guard.cuts.get(&cut_id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            format!("unknown cut id: {}", cut_id.0),
        )
            .into_response();
    };

    let pending_conn_ids: Vec<u64> = cut.pending_conn_ids.iter().map(|id| id.get()).collect();
    info!(
        cut_id = %cut_id.0,
        pending_connections = cut.pending_conn_ids.len(),
        acked_connections = cut.acks.len(),
        "cut status requested"
    );
    json_ok(&CutStatusResponse {
        cut_id: cut_id.clone(),
        requested_at_ns: cut.requested_at_ns,
        pending_connections: cut.pending_conn_ids.len(),
        acked_connections: cut.acks.len(),
        pending_conn_ids,
    })
}
