use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::{self, Body, Bytes};
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use axum::Router;
use facet::Facet;
use figue as args;
use peeps_types::{
    ApiError, Change, ConnectedProcessInfo, ConnectionsResponse, CutStatusResponse, FrameSummary,
    ProcessSnapshotView, QueryRequest, RecordCurrentResponse, RecordStartRequest,
    RecordingImportBody, RecordingSessionInfo, ScopeEntityLink, SnapshotCutResponse, SqlRequest,
    SqlResponse, TimedOutProcess, TriggerCutResponse,
};
use peeps_wire::{
    decode_client_message_default, encode_server_message_default, ClientMessage, ServerMessage,
    SnapshotRequest,
};
use rusqlite::{params, Connection};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Child;
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<ServerState>>,
    db_path: Arc<PathBuf>,
    dev_proxy: Option<DevProxyState>,
}

#[derive(Clone)]
struct DevProxyState {
    base_url: Arc<String>,
}

struct ProxiedResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

struct ServerState {
    next_conn_id: u64,
    next_cut_id: u64,
    next_snapshot_id: i64,
    next_session_id: u64,
    connections: HashMap<u64, ConnectedProcess>,
    cuts: BTreeMap<String, CutState>,
    pending_snapshots: HashMap<i64, SnapshotPending>,
    last_snapshot_json: Option<String>,
    recording: Option<RecordingState>,
}

struct ConnectedProcess {
    process_name: String,
    pid: u32,
    tx: mpsc::Sender<Vec<u8>>,
}

struct CutState {
    requested_at_ns: i64,
    pending_conn_ids: BTreeSet<u64>,
    acks: BTreeMap<u64, peeps_types::CutAck>,
}

struct SnapshotPending {
    pending_conn_ids: BTreeSet<u64>,
    replies: HashMap<u64, peeps_wire::SnapshotReply>,
    notify: Arc<Notify>,
}

struct RecordingState {
    session_id: String,
    interval_ms: u32,
    started_at_unix_ms: i64,
    stopped_at_unix_ms: Option<i64>,
    frames: Vec<StoredFrame>,
    max_frames: u32,
    max_memory_bytes: u64,
    overflowed: bool,
    total_frames_captured: u32,
    approx_memory_bytes: u64,
    total_capture_ms: f64,
    max_capture_ms: f64,
    stop_signal: Arc<Notify>,
}

struct StoredFrame {
    frame_index: u32,
    captured_at_unix_ms: i64,
    process_count: u32,
    capture_duration_ms: f64,
    json: String,
}

#[derive(Facet, Debug)]
struct Cli {
    #[facet(flatten)]
    builtins: args::FigueBuiltins,
    #[facet(args::named, default)]
    dev: bool,
}

const DB_SCHEMA_VERSION: i64 = 3;
const DEFAULT_VITE_ADDR: &str = "[::]:9131";
const PROXY_BODY_LIMIT_BYTES: usize = 8 * 1024 * 1024;

const REAPER_PIPE_FD_ENV: &str = "PEEPS_REAPER_PIPE_FD";
const REAPER_PGID_ENV: &str = "PEEPS_REAPER_PGID";

fn main() {
    // Reaper mode: watch the pipe, kill the process group when it closes.
    // Must NOT call die_with_parent() — we need to outlive the parent briefly.
    #[cfg(unix)]
    if let (Ok(fd_str), Ok(pgid_str)) = (
        std::env::var(REAPER_PIPE_FD_ENV),
        std::env::var(REAPER_PGID_ENV),
    ) {
        if let (Ok(fd), Ok(pgid)) = (
            fd_str.parse::<libc::c_int>(),
            pgid_str.parse::<libc::pid_t>(),
        ) {
            reaper_main(fd, pgid);
            return;
        }
    }

    ur_taking_me_with_you::die_with_parent();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
        .block_on(async {
            if let Err(err) = run().await {
                eprintln!("{err}");
                std::process::exit(1);
            }
        });
}

#[cfg(unix)]
fn reaper_main(pipe_fd: libc::c_int, pgid: libc::pid_t) {
    // Block until the parent closes the write end of the pipe (i.e. parent died).
    let mut buf = [0u8; 1];
    loop {
        let n = unsafe { libc::read(pipe_fd, buf.as_mut_ptr() as *mut _, 1) };
        if n <= 0 {
            break; // EOF or error — parent is gone
        }
    }
    // Kill the entire process group.
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

async fn run() -> Result<(), String> {
    let cli = parse_cli()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let tcp_addr = std::env::var("PEEPS_LISTEN").unwrap_or_else(|_| "127.0.0.1:9119".into());
    let http_addr = std::env::var("PEEPS_HTTP").unwrap_or_else(|_| "127.0.0.1:9130".into());
    let vite_addr = std::env::var("PEEPS_VITE_ADDR").unwrap_or_else(|_| DEFAULT_VITE_ADDR.into());
    let db_path =
        PathBuf::from(std::env::var("PEEPS_DB").unwrap_or_else(|_| "peeps-web.sqlite".into()));
    init_sqlite(&db_path).map_err(|e| format!("failed to init sqlite at {:?}: {e}", db_path))?;

    let mut dev_vite_child: Option<Child> = None;
    let dev_proxy = if cli.dev {
        let child = start_vite_dev_server(&vite_addr).await?;
        info!(vite_addr = %vite_addr, "peeps-web --dev launched Vite");
        dev_vite_child = Some(child);
        Some(DevProxyState {
            base_url: Arc::new(format!("http://{vite_addr}")),
        })
    } else {
        None
    };

    let state = AppState {
        inner: Arc::new(Mutex::new(ServerState {
            next_conn_id: 1,
            next_cut_id: 1,
            next_snapshot_id: 1,
            next_session_id: 1,
            connections: HashMap::new(),
            cuts: BTreeMap::new(),
            pending_snapshots: HashMap::new(),
            last_snapshot_json: None,
            recording: None,
        })),
        db_path: Arc::new(db_path),
        dev_proxy,
    };

    let tcp_listener = TcpListener::bind(&tcp_addr)
        .await
        .map_err(|e| format!("failed to bind TCP on {tcp_addr}: {e}"))?;
    info!(%tcp_addr, "peeps-web TCP ingest listener ready");

    let http_listener = TcpListener::bind(&http_addr)
        .await
        .map_err(|e| format!("failed to bind HTTP on {http_addr}: {e}"))?;
    if cli.dev {
        info!(%http_addr, vite_addr = %vite_addr, "peeps-web HTTP API + Vite proxy ready");
    } else {
        info!(%http_addr, "peeps-web HTTP API ready");
    }
    print_startup_hints(
        &http_addr,
        &tcp_addr,
        if cli.dev { Some(&vite_addr) } else { None },
    );

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/api/connections", get(api_connections))
        .route("/api/cuts", post(api_trigger_cut))
        .route("/api/cuts/{cut_id}", get(api_cut_status))
        .route("/api/sql", post(api_sql))
        .route("/api/query", post(api_query))
        .route("/api/snapshot", post(api_snapshot))
        .route("/api/snapshot/current", get(api_snapshot_current))
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
    let app = app.with_state(state.clone());

    let _dev_vite_child = dev_vite_child;
    tokio::select! {
        _ = run_tcp_acceptor(tcp_listener, state.clone()) => {}
        result = axum::serve(http_listener, app) => {
            if let Err(e) = result {
                error!(%e, "HTTP server error");
            }
        }
    }
    Ok(())
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn api_connections(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let mut processes: Vec<ConnectedProcessInfo> = guard
        .connections
        .iter()
        .map(|(conn_id, conn)| ConnectedProcessInfo {
            conn_id: *conn_id,
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

async fn api_trigger_cut(State(state): State<AppState>) -> impl IntoResponse {
    let (cut_id, cut_id_string, now_ns, requested_connections, outbound) = {
        let mut guard = state.inner.lock().await;
        let cut_num = guard.next_cut_id;
        guard.next_cut_id += 1;
        let cut_id_string = format!("cut:{cut_num}");
        let cut_id = peeps_types::CutId(String::from(cut_id_string.as_str()));
        let now_ns = now_nanos();
        let mut pending_conn_ids = BTreeSet::new();
        let mut outbound = Vec::new();
        for (conn_id, conn) in &guard.connections {
            pending_conn_ids.insert(*conn_id);
            outbound.push((*conn_id, conn.tx.clone()));
        }

        guard.cuts.insert(
            cut_id_string.clone(),
            CutState {
                requested_at_ns: now_ns,
                pending_conn_ids,
                acks: BTreeMap::new(),
            },
        );

        (cut_id, cut_id_string, now_ns, outbound.len(), outbound)
    };

    let request = ServerMessage::CutRequest(peeps_types::CutRequest { cut_id });
    if let Err(e) = persist_cut_request(state.db_path.clone(), cut_id_string.clone(), now_ns).await
    {
        error!(%e, cut_id = %cut_id_string, "failed to persist cut request");
    }
    let payload = match encode_server_message_default(&request) {
        Ok(payload) => payload,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to encode cut request: {e}"),
            )
                .into_response();
        }
    };

    for (conn_id, tx) in outbound {
        if let Err(e) = tx.try_send(payload.clone()) {
            warn!(conn_id, %e, "failed to enqueue cut request");
        }
    }

    json_ok(&TriggerCutResponse {
        cut_id: cut_id_string,
        requested_at_ns: now_ns,
        requested_connections,
    })
}

async fn api_cut_status(
    State(state): State<AppState>,
    Path(cut_id): Path<String>,
) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let Some(cut) = guard.cuts.get(&cut_id) else {
        return (StatusCode::NOT_FOUND, format!("unknown cut id: {cut_id}")).into_response();
    };

    let pending_conn_ids: Vec<u64> = cut.pending_conn_ids.iter().copied().collect();
    json_ok(&CutStatusResponse {
        cut_id,
        requested_at_ns: cut.requested_at_ns,
        pending_connections: cut.pending_conn_ids.len(),
        acked_connections: cut.acks.len(),
        pending_conn_ids,
    })
}

async fn api_sql(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let req: SqlRequest = match facet_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request json: {e}"),
            )
        }
    };

    let db_path = state.db_path.clone();
    match tokio::task::spawn_blocking(move || sql_query_blocking(&db_path, req.sql.as_str())).await
    {
        Ok(Ok(resp)) => json_ok(&resp),
        Ok(Err(err)) => json_error(StatusCode::BAD_REQUEST, err),
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("sql worker join error: {e}"),
        ),
    }
}

async fn api_query(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let req: QueryRequest = match facet_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request json: {e}"),
            )
        }
    };

    let db_path = state.db_path.clone();
    let name = req.name.to_string();
    let limit = req.limit.unwrap_or(50);
    match tokio::task::spawn_blocking(move || query_named_blocking(&db_path, &name, limit)).await {
        Ok(Ok(resp)) => json_ok(&resp),
        Ok(Err(err)) => json_error(StatusCode::BAD_REQUEST, err),
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("query worker join error: {e}"),
        ),
    }
}

async fn api_snapshot(State(state): State<AppState>) -> impl IntoResponse {
    json_ok(&take_snapshot_internal(&state).await)
}

async fn api_snapshot_current(State(state): State<AppState>) -> impl IntoResponse {
    let snapshot_json = {
        let guard = state.inner.lock().await;
        guard.last_snapshot_json.clone()
    };
    match snapshot_json {
        Some(body) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            body,
        )
            .into_response(),
        None => json_error(StatusCode::NOT_FOUND, "no snapshot available"),
    }
}

async fn remember_snapshot(state: &AppState, snapshot: &SnapshotCutResponse) {
    let Ok(json) = facet_json::to_string(snapshot) else {
        warn!("failed to serialize snapshot for cache");
        return;
    };
    let mut guard = state.inner.lock().await;
    guard.last_snapshot_json = Some(json);
}

async fn take_snapshot_internal(state: &AppState) -> SnapshotCutResponse {
    const SNAPSHOT_TIMEOUT_MS: u64 = 5000;

    // Assign a snapshot id and register a pending entry before sending requests,
    // so replies that arrive before we start waiting are not lost.
    let snapshot_id;
    let notify;
    let txs: Vec<(u64, mpsc::Sender<Vec<u8>>)>;
    {
        let mut guard = state.inner.lock().await;
        snapshot_id = guard.next_snapshot_id;
        guard.next_snapshot_id += 1;

        txs = guard
            .connections
            .iter()
            .map(|(id, conn)| (*id, conn.tx.clone()))
            .collect();

        notify = Arc::new(Notify::new());
        if !txs.is_empty() {
            let pending_conn_ids: BTreeSet<u64> = txs.iter().map(|(id, _)| *id).collect();
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

    if txs.is_empty() {
        let response = SnapshotCutResponse {
            captured_at_unix_ms: now_ms(),
            processes: vec![],
            timed_out_processes: vec![],
        };
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
                    captured_at_unix_ms: now_ms(),
                    processes: vec![],
                    timed_out_processes: vec![],
                };
                remember_snapshot(state, &response).await;
                return response;
            }
        };

    for (_, tx) in &txs {
        if let Err(e) = tx.try_send(request_frame.clone()) {
            debug!(%e, "failed to send snapshot request to connection");
        }
    }

    // Wait for all replies or timeout.
    let _ = tokio::time::timeout(
        Duration::from_millis(SNAPSHOT_TIMEOUT_MS),
        notify.notified(),
    )
    .await;

    // Collect whatever arrived.
    let captured_at_unix_ms = now_ms();
    let (pending, conn_info) = {
        let mut guard = state.inner.lock().await;
        let pending = guard.pending_snapshots.remove(&snapshot_id);
        let conn_info: HashMap<u64, (String, u32)> = guard
            .connections
            .iter()
            .map(|(id, conn)| (*id, (conn.process_name.clone(), conn.pid)))
            .collect();
        (pending, conn_info)
    };

    let (processes, timed_out_processes) = match pending {
        None => (vec![], vec![]),
        Some(p) => {
            // Collect partial process data first (synchronous), then fetch scope
            // entity links from the DB for each process (async, one per process).
            let partial: Vec<(u64, String, u32, u64, peeps_types::Snapshot)> = p
                .replies
                .into_iter()
                .filter_map(|(conn_id, reply)| {
                    let snapshot = reply.snapshot?;
                    let (process_name, pid) = conn_info
                        .get(&conn_id)
                        .map(|(name, pid)| (name.clone(), *pid))
                        .unwrap_or_else(|| (format!("unknown-{conn_id}"), 0));
                    Some((conn_id, process_name, pid, reply.ptime_now_ms, snapshot))
                })
                .collect();

            let mut processes = Vec::with_capacity(partial.len());
            for (conn_id, process_name, pid, ptime_now_ms, snapshot) in partial {
                let db_path = state.db_path.clone();
                let scope_entity_links = tokio::task::spawn_blocking(move || {
                    fetch_scope_entity_links_blocking(&db_path, conn_id)
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

            // Any conn_id still in pending_conn_ids did not reply in time.
            let timed_out_processes = p
                .pending_conn_ids
                .into_iter()
                .map(|conn_id| {
                    let (process_name, pid) = conn_info
                        .get(&conn_id)
                        .map(|(name, pid)| (name.clone(), *pid))
                        .unwrap_or_else(|| (format!("unknown-{conn_id}"), 0));
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
        captured_at_unix_ms,
        processes,
        timed_out_processes,
    };
    remember_snapshot(state, &response).await;
    response
}

async fn api_record_start(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
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
                )
            }
        }
    };

    let (session_id, stop_signal) = {
        let mut guard = state.inner.lock().await;
        if guard
            .recording
            .as_ref()
            .map_or(false, |r| r.stopped_at_unix_ms.is_none())
        {
            return json_error(StatusCode::CONFLICT, "recording already in progress");
        }

        let session_num = guard.next_session_id;
        guard.next_session_id += 1;
        let session_id = format!("session:{session_num}");
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
                    while recording.approx_memory_bytes > recording.max_memory_bytes
                        && !recording.frames.is_empty()
                    {
                        recording.overflowed = true;
                        let dropped = recording.frames.remove(0);
                        recording.approx_memory_bytes = recording
                            .approx_memory_bytes
                            .saturating_sub(dropped.json.len() as u64);
                    }
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

async fn api_record_stop(State(state): State<AppState>) -> impl IntoResponse {
    let stop_signal = {
        let mut guard = state.inner.lock().await;
        match &mut guard.recording {
            None => return json_error(StatusCode::NOT_FOUND, "no recording in progress"),
            Some(rec) if rec.stopped_at_unix_ms.is_some() => {
                return json_error(StatusCode::NOT_FOUND, "no recording in progress")
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

async fn api_record_current(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let session = guard.recording.as_ref().map(recording_session_info);
    json_ok(&RecordCurrentResponse { session })
}

async fn api_record_frame(
    State(state): State<AppState>,
    Path(frame_index): Path<u32>,
) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let Some(recording) = &guard.recording else {
        return json_error(StatusCode::NOT_FOUND, "no recording");
    };
    if recording.frames.is_empty() {
        return json_error(StatusCode::NOT_FOUND, "frame not found");
    }
    let first_index = recording.frames[0].frame_index;
    if frame_index < first_index {
        return json_error(StatusCode::NOT_FOUND, "frame not found (dropped)");
    }
    let vec_index = (frame_index - first_index) as usize;
    let Some(frame) = recording.frames.get(vec_index) else {
        return json_error(StatusCode::NOT_FOUND, "frame not found");
    };
    // Serve the pre-serialized JSON directly.
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        frame.json.clone(),
    )
        .into_response()
}

async fn api_record_export(State(state): State<AppState>) -> impl IntoResponse {
    let (session_info, frames_json) = {
        let guard = state.inner.lock().await;
        let Some(recording) = &guard.recording else {
            return json_error(StatusCode::NOT_FOUND, "no recording");
        };
        if recording.stopped_at_unix_ms.is_none() {
            return json_error(StatusCode::CONFLICT, "recording is still in progress");
        }
        let session_info = recording_session_info(recording);
        let frames_json: Vec<String> = recording
            .frames
            .iter()
            .map(|f| {
                format!(
                    r#"{{"frame_index":{},"snapshot":{}}}"#,
                    f.frame_index, f.json
                )
            })
            .collect();
        (session_info, frames_json)
    };

    let session_json = match facet_json::to_string(&session_info) {
        Ok(s) => s,
        Err(e) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to serialize session: {e}"),
            )
        }
    };

    let export_json = format!(
        r#"{{"version":1,"session":{},"frames":[{}]}}"#,
        session_json,
        frames_json.join(",")
    );

    let filename = format!(
        "recording-{}.json",
        session_info.session_id.replace(':', "_")
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

async fn api_record_import(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
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

    let summary_by_index: HashMap<u32, &FrameSummary> = import
        .session
        .frames
        .iter()
        .map(|f| (f.frame_index, f))
        .collect();

    let mut frames: Vec<StoredFrame> = Vec::with_capacity(import.frames.len());
    for f in &import.frames {
        let json = match facet_json::to_string(&f.snapshot) {
            Ok(j) => j,
            Err(e) => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    format!("failed to re-serialize frame {}: {e}", f.frame_index),
                )
            }
        };
        let summary = summary_by_index.get(&f.frame_index);
        let captured_at_unix_ms = summary.map_or(0, |s| s.captured_at_unix_ms);
        let process_count = summary.map_or(0, |s| s.process_count);
        let capture_duration_ms = summary.map_or(0.0, |s| s.capture_duration_ms);
        frames.push(StoredFrame {
            frame_index: f.frame_index,
            captured_at_unix_ms,
            process_count,
            capture_duration_ms,
            json,
        });
    }
    frames.sort_by_key(|f| f.frame_index);

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

fn recording_session_info(rec: &RecordingState) -> RecordingSessionInfo {
    let status = if rec.stopped_at_unix_ms.is_none() {
        "recording"
    } else {
        "stopped"
    };
    let avg_capture_ms = if rec.total_frames_captured > 0 {
        rec.total_capture_ms / rec.total_frames_captured as f64
    } else {
        0.0
    };
    let frames = rec
        .frames
        .iter()
        .map(|f| FrameSummary {
            frame_index: f.frame_index,
            captured_at_unix_ms: f.captured_at_unix_ms,
            process_count: f.process_count,
            capture_duration_ms: f.capture_duration_ms,
        })
        .collect();
    RecordingSessionInfo {
        session_id: rec.session_id.clone(),
        status: status.to_string(),
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

fn parse_cli() -> Result<Cli, String> {
    let figue_config = args::builder::<Cli>()
        .map_err(|e| format!("failed to build CLI schema: {e}"))?
        .cli(|cli| cli.strict())
        .help(|h| {
            h.program_name("peeps-web")
                .description("SQLite-backed peeps ingest + API server")
                .version(option_env!("CARGO_PKG_VERSION").unwrap_or("dev"))
        })
        .build();
    let cli = args::Driver::new(figue_config)
        .run()
        .into_result()
        .map_err(|e| e.to_string())?;
    Ok(cli.value)
}

fn print_startup_hints(http_addr: &str, tcp_addr: &str, vite_addr: Option<&str>) {
    let mode = if vite_addr.is_some() {
        "dev proxy"
    } else {
        "api only"
    };
    println!();
    println!();

    if let Some(vite_addr) = vite_addr {
        println!("  Vite dev server (managed): http://{vite_addr}");
        println!();
    }

    println!("  peeps-web ready ({mode})");
    println!();
    println!("  \x1b[32mOpen in browser: http://{http_addr}\x1b[0m");
    println!();
    println!("  Connect apps with:");
    println!("    \x1b[32mPEEPS_DASHBOARD={tcp_addr}\x1b[0m <your-binary>");
    println!();
    println!();
}

async fn start_vite_dev_server(vite_addr: &str) -> Result<Child, String> {
    let socket_addr = std::net::SocketAddr::from_str(vite_addr)
        .map_err(|e| format!("invalid PEEPS_VITE_ADDR '{vite_addr}': {e}"))?;
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let frontend_dir = workspace_root.join("frontend");
    if !frontend_dir.is_dir() {
        return Err(format!(
            "frontend directory not found at {}",
            frontend_dir.display()
        ));
    }

    ensure_frontend_deps(&workspace_root).await?;

    let mut command = tokio::process::Command::new("pnpm");
    command
        .arg("--filter")
        .arg("peeps-frontend")
        .arg("dev")
        .arg("--")
        .arg("--host")
        .arg(socket_addr.ip().to_string())
        .arg("--port")
        .arg(socket_addr.port().to_string())
        .arg("--strictPort")
        .current_dir(&workspace_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);

    #[cfg(unix)]
    command.process_group(0);

    let child = command.spawn().map_err(|e| {
        format!(
            "failed to launch Vite via pnpm in {}: {e}",
            workspace_root.display()
        )
    })?;

    #[cfg(unix)]
    {
        let vite_pgid = child.id().ok_or("Vite child has no PID")? as libc::pid_t;
        spawn_vite_reaper(vite_pgid)?;
    }

    wait_for_tcp_ready(vite_addr, Duration::from_secs(20)).await?;
    Ok(child)
}

#[cfg(unix)]
fn spawn_vite_reaper(vite_pgid: libc::pid_t) -> Result<(), String> {
    use std::os::fd::FromRawFd;

    // Create a pipe. We keep the write end; the reaper gets the read end.
    // When we (peeps-web) die, the write end closes and the reaper wakes up.
    let mut fds = [0 as libc::c_int; 2];
    let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if ret != 0 {
        return Err(format!(
            "failed to create reaper pipe: {}",
            std::io::Error::last_os_error()
        ));
    }
    let read_fd = fds[0];
    let write_fd = fds[1];

    // read_fd: clear FD_CLOEXEC so the reaper child inherits it
    unsafe {
        let flags = libc::fcntl(read_fd, libc::F_GETFD);
        libc::fcntl(read_fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
    }
    // write_fd: set FD_CLOEXEC so it closes on exec (stays only in this process)
    unsafe {
        let flags = libc::fcntl(write_fd, libc::F_GETFD);
        libc::fcntl(write_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
    }

    let exe = std::env::current_exe().map_err(|e| format!("failed to get current exe: {e}"))?;
    std::process::Command::new(exe)
        .env(REAPER_PIPE_FD_ENV, read_fd.to_string())
        .env(REAPER_PGID_ENV, vite_pgid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn vite reaper: {e}"))?;

    // Close read_fd in this process — the child has its own copy.
    unsafe { libc::close(read_fd) };

    // Leak the write_fd intentionally: it stays open as long as this process lives.
    // When we exit (for any reason), the OS closes it, which unblocks the reaper.
    std::mem::forget(unsafe { std::fs::File::from_raw_fd(write_fd) });

    Ok(())
}

async fn ensure_frontend_deps(workspace_root: &PathBuf) -> Result<(), String> {
    let vite_ready = tokio::process::Command::new("pnpm")
        .arg("--filter")
        .arg("peeps-frontend")
        .arg("exec")
        .arg("vite")
        .arg("--version")
        .current_dir(workspace_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false);
    if vite_ready {
        return Ok(());
    }

    info!(
        workspace = %workspace_root.display(),
        "frontend dependencies missing, running pnpm install"
    );

    let status = tokio::process::Command::new("pnpm")
        .arg("install")
        .current_dir(&workspace_root)
        .env("CI", "true")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .map_err(|e| {
            format!(
                "failed to run pnpm install in {}: {e}",
                workspace_root.display()
            )
        })?;

    if !status.success() {
        return Err(format!(
            "pnpm install failed in {} (status: {status})",
            workspace_root.display()
        ));
    }

    let vite_ready = tokio::process::Command::new("pnpm")
        .arg("--filter")
        .arg("peeps-frontend")
        .arg("exec")
        .arg("vite")
        .arg("--version")
        .current_dir(workspace_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false);
    if !vite_ready {
        return Err(
            "pnpm install succeeded but vite is still unavailable for peeps-frontend".to_string(),
        );
    }

    Ok(())
}

async fn wait_for_tcp_ready(addr: &str, timeout: Duration) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(stream) => {
                drop(stream);
                return Ok(());
            }
            Err(err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(format!("timed out waiting for Vite at {addr}: {err}"));
                }
            }
        }
        sleep(Duration::from_millis(150)).await;
    }
}

async fn proxy_vite(State(state): State<AppState>, request: Request) -> axum::response::Response {
    let Some(proxy) = state.dev_proxy.clone() else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let (parts, body) = request.into_parts();
    let method = parts.method.as_str().to_string();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let url = format!("{}{}", proxy.base_url, path_and_query);
    let headers = copy_request_headers(&parts.headers);
    let body = match body::to_bytes(body, PROXY_BODY_LIMIT_BYTES).await {
        Ok(body) => body.to_vec(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("failed to read request body: {e}"),
            )
                .into_response();
        }
    };

    let proxied = match tokio::task::spawn_blocking(move || {
        proxy_vite_blocking(&method, &url, headers, body)
    })
    .await
    {
        Ok(Ok(response)) => response,
        Ok(Err(err)) => return (StatusCode::BAD_GATEWAY, err).into_response(),
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("proxy worker join error: {err}"),
            )
                .into_response();
        }
    };

    build_proxy_response(proxied)
}

fn copy_request_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect()
}

fn proxy_vite_blocking(
    method: &str,
    url: &str,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
) -> Result<ProxiedResponse, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(30))
        .build();
    let mut req = agent.request(method, url);

    for (name, value) in headers {
        if skip_request_header(&name) {
            continue;
        }
        req = req.set(&name, &value);
    }

    let resp = if body.is_empty() && (method == "GET" || method == "HEAD") {
        match req.call() {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_, resp)) => resp,
            Err(ureq::Error::Transport(err)) => {
                return Err(format!("Vite proxy request failed for {url}: {err}"));
            }
        }
    } else {
        match req.send_bytes(&body) {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_, resp)) => resp,
            Err(ureq::Error::Transport(err)) => {
                return Err(format!("Vite proxy request failed for {url}: {err}"));
            }
        }
    };

    let status = resp.status();
    let mut response_headers = Vec::new();
    for name in resp.headers_names() {
        for value in resp.all(&name) {
            response_headers.push((name.clone(), value.to_string()));
        }
    }

    let mut response_body = Vec::new();
    resp.into_reader()
        .read_to_end(&mut response_body)
        .map_err(|e| format!("failed reading Vite proxy response body: {e}"))?;

    Ok(ProxiedResponse {
        status,
        headers: response_headers,
        body: response_body,
    })
}

fn build_proxy_response(proxied: ProxiedResponse) -> axum::response::Response {
    let status = StatusCode::from_u16(proxied.status).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut response = axum::response::Response::new(Body::from(proxied.body));
    *response.status_mut() = status;

    for (name, value) in proxied.headers {
        if skip_response_header(&name) {
            continue;
        }
        let Ok(header_name) = header::HeaderName::from_str(&name) else {
            continue;
        };
        let Ok(header_value) = header::HeaderValue::from_str(&value) else {
            continue;
        };
        response.headers_mut().append(header_name, header_value);
    }

    response
}

fn skip_request_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "host" || lower == "content-length" || is_hop_by_hop(&lower)
}

fn skip_response_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "content-length" || is_hop_by_hop(&lower)
}

fn is_hop_by_hop(lowercase_name: &str) -> bool {
    matches!(
        lowercase_name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

async fn run_tcp_acceptor(listener: TcpListener, state: AppState) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!(%addr, "TCP connection accepted");
                let st = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_conn(stream, st).await {
                        error!(%addr, %e, "connection error");
                    }
                });
            }
            Err(e) => error!(%e, "TCP accept failed"),
        }
    }
}

async fn handle_conn(stream: TcpStream, state: AppState) -> Result<(), String> {
    let (mut reader, mut writer) = stream.into_split();
    let (msg_tx, mut msg_rx) = mpsc::channel::<Vec<u8>>(32);

    let conn_id = {
        let mut guard = state.inner.lock().await;
        let conn_id = guard.next_conn_id;
        guard.next_conn_id += 1;
        guard.connections.insert(
            conn_id,
            ConnectedProcess {
                process_name: format!("unknown-{conn_id}"),
                pid: 0,
                tx: msg_tx,
            },
        );
        conn_id
    };
    if let Err(e) = persist_connection_upsert(
        state.db_path.clone(),
        conn_id,
        format!("unknown-{conn_id}"),
        0,
    )
    .await
    {
        warn!(conn_id, %e, "failed to persist connection row");
    }

    let writer_handle = tokio::spawn(async move {
        while let Some(frame) = msg_rx.recv().await {
            if writer.write_all(&frame).await.is_err() {
                break;
            }
        }
    });

    let read_result = read_messages(conn_id, &mut reader, &state).await;

    let to_notify: Vec<Arc<Notify>> = {
        let mut guard = state.inner.lock().await;
        guard.connections.remove(&conn_id);
        for cut in guard.cuts.values_mut() {
            cut.pending_conn_ids.remove(&conn_id);
            cut.acks.remove(&conn_id);
        }
        guard
            .pending_snapshots
            .values_mut()
            .filter_map(|pending| {
                if pending.pending_conn_ids.remove(&conn_id) && pending.pending_conn_ids.is_empty()
                {
                    Some(pending.notify.clone())
                } else {
                    None
                }
            })
            .collect()
    };
    for notify in to_notify {
        notify.notify_one();
    }
    if let Err(e) = persist_connection_closed(state.db_path.clone(), conn_id).await {
        warn!(conn_id, %e, "failed to persist connection close");
    }

    writer_handle.abort();
    read_result
}

async fn read_messages(
    conn_id: u64,
    reader: &mut tokio::net::tcp::OwnedReadHalf,
    state: &AppState,
) -> Result<(), String> {
    loop {
        let mut len_buf = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut len_buf).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                debug!(conn_id, "connection closed (EOF)");
                return Ok(());
            }
            return Err(format!("read frame len: {e}"));
        }

        let payload_len = u32::from_be_bytes(len_buf) as usize;
        if payload_len > peeps_wire::DEFAULT_MAX_FRAME_BYTES {
            return Err(format!("frame too large: {payload_len}"));
        }

        let mut payload = vec![0u8; payload_len];
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|e| format!("read frame payload: {e}"))?;

        let mut framed = Vec::with_capacity(4 + payload.len());
        framed.extend_from_slice(&len_buf);
        framed.extend_from_slice(&payload);
        let message = decode_client_message_default(&framed)
            .map_err(|e| format!("decode client message: {e}"))?;

        match message {
            ClientMessage::Handshake(handshake) => {
                let mut guard = state.inner.lock().await;
                if let Some(conn) = guard.connections.get_mut(&conn_id) {
                    conn.process_name = handshake.process_name.to_string();
                    conn.pid = handshake.pid;
                }
                if let Err(e) = persist_connection_upsert(
                    state.db_path.clone(),
                    conn_id,
                    handshake.process_name.to_string(),
                    handshake.pid,
                )
                .await
                {
                    warn!(conn_id, %e, "failed to persist handshake");
                }
            }
            ClientMessage::SnapshotReply(reply) => {
                debug!(
                    conn_id,
                    snapshot_id = reply.snapshot_id,
                    has_snapshot = reply.snapshot.is_some(),
                    "received snapshot reply"
                );
                let notify_opt = {
                    let mut guard = state.inner.lock().await;
                    if let Some(pending) = guard.pending_snapshots.get_mut(&reply.snapshot_id) {
                        pending.pending_conn_ids.remove(&conn_id);
                        pending.replies.insert(conn_id, reply);
                        if pending.pending_conn_ids.is_empty() {
                            Some(pending.notify.clone())
                        } else {
                            None
                        }
                    } else {
                        debug!(
                            conn_id,
                            snapshot_id = reply.snapshot_id,
                            "snapshot reply for unknown id"
                        );
                        None
                    }
                };
                if let Some(notify) = notify_opt {
                    notify.notify_one();
                }
            }
            ClientMessage::DeltaBatch(batch) => {
                if let Err(e) = persist_delta_batch(state.db_path.clone(), conn_id, batch).await {
                    warn!(conn_id, %e, "failed to persist delta batch");
                }
            }
            ClientMessage::CutAck(ack) => {
                let cut_id_text = ack.cut_id.0.to_string();
                let cursor_stream_id = ack.cursor.stream_id.0.to_string();
                let cursor_next_seq_no = ack.cursor.next_seq_no.0;
                let cut_id = ack.cut_id.0.to_string();
                let mut guard = state.inner.lock().await;
                if let Some(cut) = guard.cuts.get_mut(&cut_id) {
                    cut.pending_conn_ids.remove(&conn_id);
                    cut.acks.insert(conn_id, ack);
                } else {
                    warn!(conn_id, cut_id = %cut_id, "received cut ack for unknown cut");
                }
                drop(guard);
                if let Err(e) = persist_cut_ack(
                    state.db_path.clone(),
                    cut_id_text,
                    conn_id,
                    cursor_stream_id,
                    cursor_next_seq_no,
                )
                .await
                {
                    warn!(conn_id, %e, "failed to persist cut ack");
                }
            }
            ClientMessage::Error(msg) => {
                warn!(
                    conn_id,
                    process_name = %msg.process_name,
                    stage = %msg.stage,
                    error = %msg.error,
                    "client reported protocol/runtime error"
                );
            }
        }
    }
}

fn json_ok<T>(value: &T) -> axum::response::Response
where
    T: for<'facet> Facet<'facet>,
{
    match facet_json::to_string(value) {
        Ok(body) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("json encode error: {e}"),
        )
            .into_response(),
    }
}

fn json_error(status: StatusCode, message: impl Into<String>) -> axum::response::Response {
    json_with_status(
        status,
        &ApiError {
            error: message.into(),
        },
    )
}

fn json_with_status<T>(status: StatusCode, value: &T) -> axum::response::Response
where
    T: for<'facet> Facet<'facet>,
{
    match facet_json::to_string(value) {
        Ok(body) => (
            status,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("json encode error: {e}"),
        )
            .into_response(),
    }
}

fn now_nanos() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn to_i64_u64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn fetch_scope_entity_links_blocking(
    db_path: &PathBuf,
    conn_id: u64,
) -> Result<Vec<ScopeEntityLink>, String> {
    let conn = Connection::open(db_path).map_err(|e| format!("open sqlite: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT scope_id, entity_id FROM entity_scope_links WHERE conn_id = ?1")
        .map_err(|e| format!("prepare scope_entity_links: {e}"))?;
    let links = stmt
        .query_map(params![to_i64_u64(conn_id)], |row| {
            Ok(ScopeEntityLink {
                scope_id: row.get::<_, String>(0)?.into(),
                entity_id: row.get::<_, String>(1)?.into(),
            })
        })
        .map_err(|e| format!("query scope_entity_links: {e}"))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(links)
}

fn sql_query_blocking(db_path: &PathBuf, sql: &str) -> Result<SqlResponse, String> {
    let sql = sql.trim();
    if sql.is_empty() {
        return Err("empty SQL".to_string());
    }

    let conn = Connection::open(db_path).map_err(|e| format!("open sqlite: {e}"))?;
    let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare sql: {e}"))?;
    if !stmt.readonly() {
        return Err("only read-only statements are allowed".to_string());
    }

    let column_count = stmt.column_count();
    let columns: Vec<String> = (0..column_count)
        .map(|i| String::from(stmt.column_name(i).unwrap_or("?")))
        .collect();

    let mut rows = Vec::new();
    let mut raw_rows = stmt.raw_query();

    loop {
        let Some(row) = raw_rows.next().map_err(|e| format!("query row: {e}"))? else {
            break;
        };

        let mut row_values = Vec::with_capacity(column_count);
        for idx in 0..column_count {
            let value_ref = row
                .get_ref(idx)
                .map_err(|e| format!("read column {idx}: {e}"))?;
            row_values.push(peeps_sqlite_facet::sqlite_value_ref_to_facet(value_ref));
        }
        let row_value: facet_value::Value = row_values.into_iter().collect();
        rows.push(row_value);
    }

    Ok(SqlResponse {
        columns,
        row_count: rows.len() as u32,
        rows,
    })
}

fn query_named_blocking(db_path: &PathBuf, name: &str, limit: u32) -> Result<SqlResponse, String> {
    let sql = named_query_sql(name, limit)?;
    sql_query_blocking(db_path, &sql)
}

fn named_query_sql(name: &str, limit: u32) -> Result<String, String> {
    match name {
        "blockers" => Ok(format!(
            "select \
             e.src_id as waiter_id, \
             json_extract(src.entity_json, '$.name') as waiter_name, \
             e.dst_id as blocked_on_id, \
             json_extract(dst.entity_json, '$.name') as blocked_on_name, \
             e.kind_json \
             from edges e \
             left join entities src on src.conn_id = e.conn_id and src.stream_id = e.stream_id and src.entity_id = e.src_id \
             left join entities dst on dst.conn_id = e.conn_id and dst.stream_id = e.stream_id and dst.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
             order by e.updated_at_ns desc \
             limit {limit}"
        )),
        "blocked-senders" => Ok(format!(
            "select \
             f.entity_id as send_future_id, \
             json_extract(f.entity_json, '$.name') as send_name, \
             e.dst_id as waiting_on_entity_id, \
             json_extract(ch.entity_json, '$.name') as waiting_on_name, \
             e.updated_at_ns \
             from edges e \
             join entities f on f.conn_id = e.conn_id and f.stream_id = e.stream_id and f.entity_id = e.src_id \
             left join entities ch on ch.conn_id = e.conn_id and ch.stream_id = e.stream_id and ch.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
               and json_extract(f.entity_json, '$.body') = 'future' \
               and json_extract(f.entity_json, '$.name') like '%.send' \
             order by e.updated_at_ns desc \
             limit {limit}"
        )),
        "blocked-receivers" => Ok(format!(
            "select \
             f.entity_id as recv_future_id, \
             json_extract(f.entity_json, '$.name') as recv_name, \
             e.dst_id as waiting_on_entity_id, \
             json_extract(ch.entity_json, '$.name') as waiting_on_name, \
             e.updated_at_ns \
             from edges e \
             join entities f on f.conn_id = e.conn_id and f.stream_id = e.stream_id and f.entity_id = e.src_id \
             left join entities ch on ch.conn_id = e.conn_id and ch.stream_id = e.stream_id and ch.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
               and json_extract(f.entity_json, '$.body') = 'future' \
               and json_extract(f.entity_json, '$.name') like '%.recv' \
             order by e.updated_at_ns desc \
             limit {limit}"
        )),
        "stalled-sends" => Ok(format!(
            "select \
             f.entity_id as send_future_id, \
             json_extract(f.entity_json, '$.name') as send_name, \
             e.dst_id as waiting_on_entity_id, \
             json_extract(ch.entity_json, '$.name') as waiting_on_name \
             from edges e \
             join entities f on f.conn_id = e.conn_id and f.stream_id = e.stream_id and f.entity_id = e.src_id \
             left join entities ch on ch.conn_id = e.conn_id and ch.stream_id = e.stream_id and ch.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
               and json_extract(f.entity_json, '$.body') = 'future' \
               and json_extract(f.entity_json, '$.name') like '%.send' \
             order by e.updated_at_ns desc \
             limit {limit}"
        )),
        "channel-pressure" => Ok(format!(
            "select \
             entity_id, \
             json_extract(entity_json, '$.name') as name, \
             coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.capacity'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.capacity')) as capacity, \
             coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.occupancy'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.occupancy')) as occupancy, \
             case \
               when coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.capacity'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.capacity')) > 0 \
               then cast(coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.occupancy'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.occupancy')) as real) / \
                    cast(coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.capacity'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.capacity')) as real) \
               else null \
             end as utilization \
             from entities \
             where json_extract(entity_json, '$.body.channel_tx.details.mpsc') is not null \
                or json_extract(entity_json, '$.body.channel_rx.details.mpsc') is not null \
             order by utilization desc, name asc \
             limit {limit}"
        )),
        "channel-health" => Ok(format!(
            "select \
             entity_id, \
             json_extract(entity_json, '$.name') as name, \
             coalesce( \
               json_extract(entity_json, '$.body.channel_tx.lifecycle'), \
               json_extract(entity_json, '$.body.channel_rx.lifecycle') \
             ) as lifecycle, \
             coalesce( \
               json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.capacity'), \
               json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.capacity') \
             ) as capacity, \
             coalesce( \
               json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.occupancy'), \
               json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.occupancy') \
             ) as occupancy \
             from entities \
             where json_extract(entity_json, '$.body.channel_tx') is not null \
                or json_extract(entity_json, '$.body.channel_rx') is not null \
             order by name \
             limit {limit}"
        )),
        "scope-membership" => Ok(format!(
            "select \
             l.scope_id, \
             json_extract(s.scope_json, '$.name') as scope_name, \
             l.entity_id, \
             json_extract(e.entity_json, '$.name') as entity_name \
             from entity_scope_links l \
             left join scopes s on s.conn_id = l.conn_id and s.stream_id = l.stream_id and s.scope_id = l.scope_id \
             left join entities e on e.conn_id = l.conn_id and e.stream_id = l.stream_id and e.entity_id = l.entity_id \
             order by scope_name asc, entity_name asc \
             limit {limit}"
        )),
        "missing-scope-links" => Ok(format!(
            "select \
             e.conn_id as process_id, \
             c.process_name, \
             c.pid, \
             e.stream_id, \
             e.entity_id, \
             json_extract(e.entity_json, '$.name') as entity_name, \
             json_extract(e.entity_json, '$.body') as entity_body, \
             case \
               when p.process_scope_count is null then 1 \
               else 0 \
             end as missing_process_scope_link, \
             case \
               when json_extract(e.entity_json, '$.body') = 'future' and t.task_scope_count is null then 1 \
               else 0 \
             end as missing_task_scope_link \
             from entities e \
             left join connections c \
               on c.conn_id = e.conn_id \
             left join ( \
               select \
                 l.conn_id, \
                 l.stream_id, \
                 l.entity_id, \
                 count(*) as process_scope_count \
               from entity_scope_links l \
               join scopes s \
                 on s.conn_id = l.conn_id \
                and s.stream_id = l.stream_id \
                and s.scope_id = l.scope_id \
               where json_extract(s.scope_json, '$.body') = 'process' \
               group by l.conn_id, l.stream_id, l.entity_id \
             ) p \
               on p.conn_id = e.conn_id \
              and p.stream_id = e.stream_id \
              and p.entity_id = e.entity_id \
             left join ( \
               select \
                 l.conn_id, \
                 l.stream_id, \
                 l.entity_id, \
                 count(*) as task_scope_count \
               from entity_scope_links l \
               join scopes s \
                 on s.conn_id = l.conn_id \
                and s.stream_id = l.stream_id \
                and s.scope_id = l.scope_id \
               where json_extract(s.scope_json, '$.body') = 'task' \
               group by l.conn_id, l.stream_id, l.entity_id \
             ) t \
               on t.conn_id = e.conn_id \
              and t.stream_id = e.stream_id \
              and t.entity_id = e.entity_id \
             where p.process_scope_count is null \
                or (json_extract(e.entity_json, '$.body') = 'future' and t.task_scope_count is null) \
             order by c.process_name asc, entity_name asc, e.entity_id asc \
             limit {limit}"
        )),
        "stale-blockers" => Ok(format!(
            "select \
             e.src_id as waiter_id, \
             json_extract(src.entity_json, '$.name') as waiter_name, \
             e.dst_id as blocked_on_id, \
             json_extract(dst.entity_json, '$.name') as blocked_on_name, \
             e.updated_at_ns \
             from edges e \
             left join entities src on src.conn_id = e.conn_id and src.stream_id = e.stream_id and src.entity_id = e.src_id \
             left join entities dst on dst.conn_id = e.conn_id and dst.stream_id = e.stream_id and dst.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
             order by e.updated_at_ns asc \
             limit {limit}"
        )),
        _ => Err(format!(
            "unknown query pack: {name}. expected one of: blockers, blocked-senders, blocked-receivers, stalled-sends, channel-pressure, channel-health, scope-membership, missing-scope-links, stale-blockers"
        )),
    }
}

fn init_sqlite(db_path: &PathBuf) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| format!("open sqlite: {e}"))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
        .map_err(|e| format!("init sqlite pragmas: {e}"))?;

    let user_version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| format!("read sqlite user_version: {e}"))?;

    if user_version > DB_SCHEMA_VERSION {
        return Err(format!(
            "database schema version {} is newer than supported {}",
            user_version, DB_SCHEMA_VERSION
        ));
    }

    if user_version < DB_SCHEMA_VERSION {
        reset_managed_schema(&conn)?;
        conn.pragma_update(None, "user_version", DB_SCHEMA_VERSION)
            .map_err(|e| format!("set sqlite user_version: {e}"))?;
    }

    conn.execute_batch(managed_schema_sql())
        .map_err(|e| format!("ensure schema: {e}"))?;
    Ok(())
}

fn reset_managed_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS events;
        DROP TABLE IF EXISTS edges;
        DROP TABLE IF EXISTS entities;
        DROP TABLE IF EXISTS scopes;
        DROP TABLE IF EXISTS entity_scope_links;
        DROP TABLE IF EXISTS delta_batches;
        DROP TABLE IF EXISTS stream_cursors;
        DROP TABLE IF EXISTS cut_acks;
        DROP TABLE IF EXISTS cuts;
        DROP TABLE IF EXISTS connections;
        ",
    )
    .map_err(|e| format!("reset schema: {e}"))
}

fn managed_schema_sql() -> &'static str {
    "
    CREATE TABLE IF NOT EXISTS connections (
        conn_id INTEGER PRIMARY KEY,
        process_name TEXT NOT NULL,
        pid INTEGER NOT NULL,
        connected_at_ns INTEGER NOT NULL,
        disconnected_at_ns INTEGER
    );

    CREATE TABLE IF NOT EXISTS cuts (
        cut_id TEXT PRIMARY KEY,
        requested_at_ns INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS cut_acks (
        cut_id TEXT NOT NULL,
        conn_id INTEGER NOT NULL,
        stream_id TEXT NOT NULL,
        next_seq_no INTEGER NOT NULL,
        received_at_ns INTEGER NOT NULL,
        PRIMARY KEY (cut_id, conn_id)
    );

    CREATE TABLE IF NOT EXISTS stream_cursors (
        conn_id INTEGER NOT NULL,
        stream_id TEXT NOT NULL,
        next_seq_no INTEGER NOT NULL,
        updated_at_ns INTEGER NOT NULL,
        PRIMARY KEY (conn_id, stream_id)
    );

    CREATE TABLE IF NOT EXISTS delta_batches (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        conn_id INTEGER NOT NULL,
        stream_id TEXT NOT NULL,
        from_seq_no INTEGER NOT NULL,
        next_seq_no INTEGER NOT NULL,
        truncated INTEGER NOT NULL,
        compacted_before_seq_no INTEGER,
        change_count INTEGER NOT NULL,
        payload_json TEXT NOT NULL,
        received_at_ns INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS entities (
        conn_id INTEGER NOT NULL,
        stream_id TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        entity_json TEXT NOT NULL,
        updated_at_ns INTEGER NOT NULL,
        PRIMARY KEY (conn_id, stream_id, entity_id)
    );

    CREATE TABLE IF NOT EXISTS scopes (
        conn_id INTEGER NOT NULL,
        stream_id TEXT NOT NULL,
        scope_id TEXT NOT NULL,
        scope_json TEXT NOT NULL,
        updated_at_ns INTEGER NOT NULL,
        PRIMARY KEY (conn_id, stream_id, scope_id)
    );

    CREATE TABLE IF NOT EXISTS entity_scope_links (
        conn_id INTEGER NOT NULL,
        stream_id TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        scope_id TEXT NOT NULL,
        updated_at_ns INTEGER NOT NULL,
        PRIMARY KEY (conn_id, stream_id, entity_id, scope_id)
    );

    CREATE TABLE IF NOT EXISTS edges (
        conn_id INTEGER NOT NULL,
        stream_id TEXT NOT NULL,
        src_id TEXT NOT NULL,
        dst_id TEXT NOT NULL,
        kind_json TEXT NOT NULL,
        edge_json TEXT NOT NULL,
        updated_at_ns INTEGER NOT NULL,
        PRIMARY KEY (conn_id, stream_id, src_id, dst_id, kind_json)
    );

    CREATE TABLE IF NOT EXISTS events (
        conn_id INTEGER NOT NULL,
        stream_id TEXT NOT NULL,
        seq_no INTEGER NOT NULL,
        event_id TEXT NOT NULL,
        event_json TEXT NOT NULL,
        at_ms INTEGER NOT NULL,
        PRIMARY KEY (conn_id, stream_id, seq_no)
    );
    "
}

async fn persist_connection_upsert(
    db_path: Arc<PathBuf>,
    conn_id: u64,
    process_name: String,
    pid: u32,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*db_path).map_err(|e| format!("open sqlite: {e}"))?;
        conn.execute(
            "INSERT INTO connections (conn_id, process_name, pid, connected_at_ns, disconnected_at_ns)
             VALUES (?1, ?2, ?3, ?4, NULL)
             ON CONFLICT(conn_id) DO UPDATE SET
               process_name = excluded.process_name,
               pid = excluded.pid",
            params![
                to_i64_u64(conn_id),
                process_name,
                i64::from(pid),
                now_nanos()
            ],
        )
        .map_err(|e| format!("upsert connection: {e}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("join sqlite: {e}"))?
}

async fn persist_connection_closed(db_path: Arc<PathBuf>, conn_id: u64) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*db_path).map_err(|e| format!("open sqlite: {e}"))?;
        conn.execute(
            "UPDATE connections SET disconnected_at_ns = ?2 WHERE conn_id = ?1",
            params![to_i64_u64(conn_id), now_nanos()],
        )
        .map_err(|e| format!("close connection: {e}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("join sqlite: {e}"))?
}

async fn persist_cut_request(
    db_path: Arc<PathBuf>,
    cut_id: String,
    requested_at_ns: i64,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*db_path).map_err(|e| format!("open sqlite: {e}"))?;
        conn.execute(
            "INSERT INTO cuts (cut_id, requested_at_ns) VALUES (?1, ?2)
             ON CONFLICT(cut_id) DO UPDATE SET requested_at_ns = excluded.requested_at_ns",
            params![cut_id, requested_at_ns],
        )
        .map_err(|e| format!("upsert cut: {e}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("join sqlite: {e}"))?
}

async fn persist_cut_ack(
    db_path: Arc<PathBuf>,
    cut_id: String,
    conn_id: u64,
    stream_id: String,
    next_seq_no: u64,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*db_path).map_err(|e| format!("open sqlite: {e}"))?;
        conn.execute(
            "INSERT INTO cut_acks (cut_id, conn_id, stream_id, next_seq_no, received_at_ns)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(cut_id, conn_id) DO UPDATE SET
               stream_id = excluded.stream_id,
               next_seq_no = excluded.next_seq_no,
               received_at_ns = excluded.received_at_ns",
            params![
                cut_id,
                to_i64_u64(conn_id),
                stream_id,
                to_i64_u64(next_seq_no),
                now_nanos()
            ],
        )
        .map_err(|e| format!("upsert cut ack: {e}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("join sqlite: {e}"))?
}

async fn persist_delta_batch(
    db_path: Arc<PathBuf>,
    conn_id: u64,
    batch: peeps_types::PullChangesResponse,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || persist_delta_batch_blocking(&db_path, conn_id, &batch))
        .await
        .map_err(|e| format!("join sqlite: {e}"))?
}

fn persist_delta_batch_blocking(
    db_path: &PathBuf,
    conn_id: u64,
    batch: &peeps_types::PullChangesResponse,
) -> Result<(), String> {
    let mut conn = Connection::open(db_path).map_err(|e| format!("open sqlite: {e}"))?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("start transaction: {e}"))?;
    let stream_id = batch.stream_id.0.as_str().to_string();
    let received_at_ns = now_nanos();
    let payload_json = facet_json::to_string(batch).map_err(|e| format!("encode batch: {e}"))?;

    tx.execute(
        "INSERT INTO delta_batches (
            conn_id, stream_id, from_seq_no, next_seq_no, truncated,
            compacted_before_seq_no, change_count, payload_json, received_at_ns
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            to_i64_u64(conn_id),
            stream_id,
            to_i64_u64(batch.from_seq_no.0),
            to_i64_u64(batch.next_seq_no.0),
            if batch.truncated { 1_i64 } else { 0_i64 },
            batch.compacted_before_seq_no.map(|s| to_i64_u64(s.0)),
            to_i64_u64(batch.changes.len() as u64),
            payload_json,
            received_at_ns,
        ],
    )
    .map_err(|e| format!("insert delta batch: {e}"))?;

    for stamped in &batch.changes {
        match &stamped.change {
            Change::UpsertEntity(entity) => {
                let entity_json =
                    facet_json::to_string(entity).map_err(|e| format!("encode entity: {e}"))?;
                tx.execute(
                    "INSERT INTO entities (conn_id, stream_id, entity_id, entity_json, updated_at_ns)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(conn_id, stream_id, entity_id) DO UPDATE SET
                       entity_json = excluded.entity_json,
                       updated_at_ns = excluded.updated_at_ns",
                    params![
                        to_i64_u64(conn_id),
                        batch.stream_id.0.as_str(),
                        entity.id.as_str(),
                        entity_json,
                        received_at_ns
                    ],
                )
                .map_err(|e| format!("upsert entity: {e}"))?;
            }
            Change::UpsertScope(scope) => {
                let scope_json =
                    facet_json::to_string(scope).map_err(|e| format!("encode scope: {e}"))?;
                tx.execute(
                    "INSERT INTO scopes (conn_id, stream_id, scope_id, scope_json, updated_at_ns)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(conn_id, stream_id, scope_id) DO UPDATE SET
                       scope_json = excluded.scope_json,
                       updated_at_ns = excluded.updated_at_ns",
                    params![
                        to_i64_u64(conn_id),
                        batch.stream_id.0.as_str(),
                        scope.id.as_str(),
                        scope_json,
                        received_at_ns
                    ],
                )
                .map_err(|e| format!("upsert scope: {e}"))?;
            }
            Change::UpsertEntityScopeLink {
                entity_id,
                scope_id,
            } => {
                tx.execute(
                    "INSERT INTO entity_scope_links (conn_id, stream_id, entity_id, scope_id, updated_at_ns)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(conn_id, stream_id, entity_id, scope_id) DO UPDATE SET
                       updated_at_ns = excluded.updated_at_ns",
                    params![
                        to_i64_u64(conn_id),
                        batch.stream_id.0.as_str(),
                        entity_id.as_str(),
                        scope_id.as_str(),
                        received_at_ns
                    ],
                )
                .map_err(|e| format!("upsert entity_scope_link: {e}"))?;
            }
            Change::RemoveEntity { id } => {
                tx.execute(
                    "DELETE FROM entities WHERE conn_id = ?1 AND stream_id = ?2 AND entity_id = ?3",
                    params![to_i64_u64(conn_id), batch.stream_id.0.as_str(), id.as_str()],
                )
                .map_err(|e| format!("delete entity: {e}"))?;
                tx.execute(
                    "DELETE FROM entity_scope_links WHERE conn_id = ?1 AND stream_id = ?2 AND entity_id = ?3",
                    params![to_i64_u64(conn_id), batch.stream_id.0.as_str(), id.as_str()],
                )
                .map_err(|e| format!("delete entity_scope_links for entity: {e}"))?;
                tx.execute(
                    "DELETE FROM edges
                     WHERE conn_id = ?1 AND stream_id = ?2 AND (src_id = ?3 OR dst_id = ?3)",
                    params![to_i64_u64(conn_id), batch.stream_id.0.as_str(), id.as_str()],
                )
                .map_err(|e| format!("delete incident edges: {e}"))?;
            }
            Change::RemoveScope { id } => {
                tx.execute(
                    "DELETE FROM scopes WHERE conn_id = ?1 AND stream_id = ?2 AND scope_id = ?3",
                    params![to_i64_u64(conn_id), batch.stream_id.0.as_str(), id.as_str()],
                )
                .map_err(|e| format!("delete scope: {e}"))?;
                tx.execute(
                    "DELETE FROM entity_scope_links WHERE conn_id = ?1 AND stream_id = ?2 AND scope_id = ?3",
                    params![to_i64_u64(conn_id), batch.stream_id.0.as_str(), id.as_str()],
                )
                .map_err(|e| format!("delete entity_scope_links for scope: {e}"))?;
            }
            Change::RemoveEntityScopeLink {
                entity_id,
                scope_id,
            } => {
                tx.execute(
                    "DELETE FROM entity_scope_links
                     WHERE conn_id = ?1 AND stream_id = ?2 AND entity_id = ?3 AND scope_id = ?4",
                    params![
                        to_i64_u64(conn_id),
                        batch.stream_id.0.as_str(),
                        entity_id.as_str(),
                        scope_id.as_str()
                    ],
                )
                .map_err(|e| format!("delete entity_scope_link: {e}"))?;
            }
            Change::UpsertEdge(edge) => {
                let kind_json = facet_json::to_string(&edge.kind)
                    .map_err(|e| format!("encode edge kind: {e}"))?;
                let edge_json =
                    facet_json::to_string(edge).map_err(|e| format!("encode edge: {e}"))?;
                tx.execute(
                    "INSERT INTO edges (conn_id, stream_id, src_id, dst_id, kind_json, edge_json, updated_at_ns)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(conn_id, stream_id, src_id, dst_id, kind_json) DO UPDATE SET
                       edge_json = excluded.edge_json,
                       updated_at_ns = excluded.updated_at_ns",
                    params![
                        to_i64_u64(conn_id),
                        batch.stream_id.0.as_str(),
                        edge.src.as_str(),
                        edge.dst.as_str(),
                        kind_json,
                        edge_json,
                        received_at_ns
                    ],
                )
                .map_err(|e| format!("upsert edge: {e}"))?;
            }
            Change::RemoveEdge { src, dst, kind } => {
                let kind_json =
                    facet_json::to_string(kind).map_err(|e| format!("encode edge kind: {e}"))?;
                tx.execute(
                    "DELETE FROM edges
                     WHERE conn_id = ?1 AND stream_id = ?2 AND src_id = ?3 AND dst_id = ?4 AND kind_json = ?5",
                    params![
                        to_i64_u64(conn_id),
                        batch.stream_id.0.as_str(),
                        src.as_str(),
                        dst.as_str(),
                        kind_json
                    ],
                )
                .map_err(|e| format!("delete edge: {e}"))?;
            }
            Change::AppendEvent(event) => {
                let event_json =
                    facet_json::to_string(event).map_err(|e| format!("encode event: {e}"))?;
                tx.execute(
                    "INSERT OR REPLACE INTO events (conn_id, stream_id, seq_no, event_id, event_json, at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        to_i64_u64(conn_id),
                        batch.stream_id.0.as_str(),
                        to_i64_u64(stamped.seq_no.0),
                        event.id.as_str(),
                        event_json,
                        to_i64_u64(event.at.as_millis()),
                    ],
                )
                .map_err(|e| format!("append event: {e}"))?;
            }
        }
    }

    tx.execute(
        "INSERT INTO stream_cursors (conn_id, stream_id, next_seq_no, updated_at_ns)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(conn_id, stream_id) DO UPDATE SET
           next_seq_no = excluded.next_seq_no,
           updated_at_ns = excluded.updated_at_ns",
        params![
            to_i64_u64(conn_id),
            batch.stream_id.0.as_str(),
            to_i64_u64(batch.next_seq_no.0),
            received_at_ns
        ],
    )
    .map_err(|e| format!("upsert stream cursor: {e}"))?;

    tx.commit()
        .map_err(|e| format!("commit transaction: {e}"))?;
    Ok(())
}
