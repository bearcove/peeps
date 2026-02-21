use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use axum::Router;
use axum::routing::{any, get, post};

use crate::api::connections::{api_connections, api_cut_status, api_trigger_cut};
use crate::api::recording::{
    api_record_current, api_record_export, api_record_frame, api_record_import, api_record_start,
    api_record_stop,
};
use crate::api::snapshot::{api_snapshot, api_snapshot_current, api_snapshot_symbolication_ws};
use crate::api::sql::{api_query, api_sql};
use crate::db::{Db, StoredModuleManifestEntry};
use crate::proxy::proxy_vite;
use crate::recording::session::RecordingState;
use moire_trace_types::BacktraceId;
use moire_types::SnapshotCutResponse;
use moire_wire::SnapshotReply;
use tokio::sync::{Mutex, Notify, mpsc};

pub mod ids;
pub use ids::{ConnectionId, CutOrdinal, SessionOrdinal};

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Mutex<ServerState>>,
    pub db: Arc<Db>,
    pub dev_proxy: Option<DevProxyState>,
}

#[derive(Clone)]
pub struct DevProxyState {
    pub base_url: Arc<String>,
}

pub struct ServerState {
    pub next_conn_id: ConnectionId,
    pub next_cut_id: CutOrdinal,
    pub next_snapshot_id: i64,
    pub next_session_id: SessionOrdinal,
    pub connections: HashMap<ConnectionId, ConnectedProcess>,
    pub cuts: BTreeMap<moire_types::CutId, CutState>,
    pub pending_snapshots: HashMap<i64, SnapshotPending>,
    pub snapshot_streams: HashMap<i64, SnapshotStreamState>,
    pub last_snapshot_json: Option<String>,
    pub recording: Option<RecordingState>,
}

pub struct ConnectedProcess {
    pub process_name: String,
    pub pid: u32,
    pub handshake_received: bool,
    pub module_manifest: Vec<StoredModuleManifestEntry>,
    pub tx: mpsc::Sender<Vec<u8>>,
}

pub struct CutState {
    pub requested_at_ns: i64,
    pub pending_conn_ids: BTreeSet<ConnectionId>,
    pub acks: BTreeMap<ConnectionId, moire_types::CutAck>,
}

pub struct SnapshotPending {
    pub pending_conn_ids: BTreeSet<ConnectionId>,
    pub replies: HashMap<ConnectionId, SnapshotReply>,
    pub notify: Arc<Notify>,
}

pub struct SnapshotStreamState {
    pub pairs: Vec<(ConnectionId, BacktraceId)>,
}

impl ServerState {
    pub fn new(next_conn_id: ConnectionId) -> Self {
        Self {
            next_conn_id,
            next_cut_id: CutOrdinal::ONE,
            next_snapshot_id: 1,
            next_session_id: SessionOrdinal::ONE,
            connections: HashMap::new(),
            cuts: BTreeMap::new(),
            pending_snapshots: HashMap::new(),
            snapshot_streams: HashMap::new(),
            last_snapshot_json: None,
            recording: None,
        }
    }
}

impl AppState {
    pub fn new(db: Db, next_conn_id: ConnectionId, dev_proxy: Option<DevProxyState>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ServerState::new(next_conn_id))),
            db: Arc::new(db),
            dev_proxy,
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    let mut app = Router::new()
        .route("/health", get(health))
        .route("/api/connections", get(api_connections))
        .route("/api/cuts", post(api_trigger_cut))
        .route("/api/cuts/{cut_id}", get(api_cut_status))
        .route("/api/sql", post(api_sql))
        .route("/api/query", post(api_query))
        .route("/api/snapshot", post(api_snapshot))
        .route("/api/snapshot/current", get(api_snapshot_current))
        .route(
            "/api/snapshot/{snapshot_id}/symbolication/ws",
            get(api_snapshot_symbolication_ws),
        )
        .route("/api/record/start", post(api_record_start))
        .route("/api/record/stop", post(api_record_stop))
        .route("/api/record/current", get(api_record_current))
        .route(
            "/api/record/current/frame/{frame_index}",
            get(api_record_frame),
        )
        .route("/api/record/current/export", get(api_record_export))
        .route("/api/record/import", post(api_record_import));
    if state.dev_proxy.is_some() {
        app = app.fallback(any(proxy_vite));
    }
    app.with_state(state)
}

pub async fn health() -> &'static str {
    "ok"
}

pub async fn remember_snapshot(state: &AppState, snapshot: &SnapshotCutResponse) {
    let Ok(json) = facet_json::to_string(snapshot) else {
        tracing::warn!("failed to serialize snapshot for cache");
        return;
    };
    let mut guard = state.inner.lock().await;
    guard.last_snapshot_json = Some(json);
}
