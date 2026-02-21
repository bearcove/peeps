use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, Request, State};
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use facet::Facet;
use figue as args;
use moire_types::{
    ConnectedProcessInfo, ConnectionsResponse, CutStatusResponse, RecordCurrentResponse,
    RecordStartRequest, RecordingImportBody, TriggerCutResponse,
};
use moire_web::api::snapshot::{
    api_snapshot, api_snapshot_current, api_snapshot_symbolication_ws, take_snapshot_internal,
};
use moire_web::api::sql::{execute_named_query_request, execute_sql_request};
use moire_web::app::{AppState, ConnectedProcess, CutState, DevProxyState, ServerState};
use moire_web::db::{
    Db, backtrace_frames_for_store, init_sqlite, into_stored_module_manifest,
    load_next_connection_id, persist_backtrace_record, persist_connection_closed,
    persist_connection_module_manifest, persist_connection_upsert, persist_cut_ack,
    persist_cut_request, persist_delta_batch,
};
use moire_web::proxy::vite::proxy_vite_request;
use moire_web::recording::session::{
    RecordingState, build_imported_frames, export_frame_rows, frame_json_by_index, push_frame,
    recording_session_info,
};
use moire_web::util::http::{json_error, json_ok};
use moire_web::util::time::{now_ms, now_nanos};
use moire_wire::{
    ClientMessage, ServerMessage, decode_client_message_default, decode_protocol_magic,
    encode_server_message_default,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Child;
use tokio::sync::{Mutex, Notify, mpsc};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

#[derive(Facet, Debug)]
struct Cli {
    #[facet(flatten)]
    builtins: args::FigueBuiltins,
    #[facet(args::named, default)]
    dev: bool,
}

const DEFAULT_VITE_ADDR: &str = "[::]:9131";
const PROXY_BODY_LIMIT_BYTES: usize = 8 * 1024 * 1024;
const REAPER_PIPE_FD_ENV: &str = "MOIRE_REAPER_PIPE_FD";
const REAPER_PGID_ENV: &str = "MOIRE_REAPER_PGID";

fn main() {
    // Reaper mode: watch the pipe, kill the process group when it closes.
    // Must NOT call die_with_parent() — we need to outlive the parent briefly.
    #[cfg(unix)]
    if let (Ok(fd_str), Ok(pgid_str)) = (
        std::env::var(REAPER_PIPE_FD_ENV),
        std::env::var(REAPER_PGID_ENV),
    ) && let (Ok(fd), Ok(pgid)) = (
        fd_str.parse::<libc::c_int>(),
        pgid_str.parse::<libc::pid_t>(),
    ) {
        reaper_main(fd, pgid);
        return;
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

    // r[impl config.web.tcp-listen]
    let tcp_addr = std::env::var("MOIRE_LISTEN").unwrap_or_else(|_| "127.0.0.1:9119".into());
    // r[impl config.web.http-listen]
    let http_addr = std::env::var("MOIRE_HTTP").unwrap_or_else(|_| "127.0.0.1:9130".into());
    // r[impl config.web.vite-addr]
    let vite_addr = std::env::var("MOIRE_VITE_ADDR").unwrap_or_else(|_| DEFAULT_VITE_ADDR.into());
    // r[impl config.web.db-path]
    let db_path =
        PathBuf::from(std::env::var("MOIRE_DB").unwrap_or_else(|_| "moire-web.sqlite".into()));
    let db = Db::new(db_path);
    init_sqlite(&db).map_err(|e| format!("failed to init sqlite at {:?}: {e}", db.path()))?;
    let next_conn_id = load_next_connection_id(&db)
        .map_err(|e| format!("failed to load next connection id at {:?}: {e}", db.path()))?;

    let mut dev_vite_child: Option<Child> = None;
    let dev_proxy = if cli.dev {
        let child = start_vite_dev_server(&vite_addr).await?;
        info!(vite_addr = %vite_addr, "moire-web --dev launched Vite");
        dev_vite_child = Some(child);
        Some(DevProxyState {
            base_url: Arc::new(format!("http://{vite_addr}")),
        })
    } else {
        None
    };

    let state = AppState {
        inner: Arc::new(Mutex::new(ServerState {
            next_conn_id,
            next_cut_id: 1,
            next_snapshot_id: 1,
            next_session_id: 1,
            connections: HashMap::new(),
            cuts: BTreeMap::new(),
            pending_snapshots: HashMap::new(),
            snapshot_streams: HashMap::new(),
            last_snapshot_json: None,
            recording: None,
        })),
        db: Arc::new(db),
        dev_proxy,
    };

    let tcp_listener = TcpListener::bind(&tcp_addr)
        .await
        .map_err(|e| format!("failed to bind TCP on {tcp_addr}: {e}"))?;
    info!(%tcp_addr, next_conn_id, "moire-web TCP ingest listener ready");

    let http_listener = TcpListener::bind(&http_addr)
        .await
        .map_err(|e| format!("failed to bind HTTP on {http_addr}: {e}"))?;
    if cli.dev {
        info!(%http_addr, vite_addr = %vite_addr, "moire-web HTTP API + Vite proxy ready");
    } else {
        info!(%http_addr, "moire-web HTTP API ready");
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
        let cut_id = moire_types::CutId(String::from(cut_id_string.as_str()));
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

    let request = ServerMessage::CutRequest(moire_types::CutRequest { cut_id });
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
    AxumPath(cut_id): AxumPath<String>,
) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let Some(cut) = guard.cuts.get(&cut_id) else {
        return (StatusCode::NOT_FOUND, format!("unknown cut id: {cut_id}")).into_response();
    };

    let pending_conn_ids: Vec<u64> = cut.pending_conn_ids.iter().copied().collect();
    info!(
        cut_id = %cut_id,
        pending_connections = cut.pending_conn_ids.len(),
        acked_connections = cut.acks.len(),
        "cut status requested"
    );
    json_ok(&CutStatusResponse {
        cut_id,
        requested_at_ns: cut.requested_at_ns,
        pending_connections: cut.pending_conn_ids.len(),
        acked_connections: cut.acks.len(),
        pending_conn_ids,
    })
}

async fn api_sql(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    execute_sql_request(body, state.db.clone()).await
}

async fn api_query(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    execute_named_query_request(body, state.db.clone()).await
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

async fn api_record_stop(State(state): State<AppState>) -> impl IntoResponse {
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

async fn api_record_current(State(state): State<AppState>) -> impl IntoResponse {
    let guard = state.inner.lock().await;
    let session = guard.recording.as_ref().map(recording_session_info);
    json_ok(&RecordCurrentResponse { session })
}

async fn api_record_frame(
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

fn parse_cli() -> Result<Cli, String> {
    let figue_config = args::builder::<Cli>()
        .map_err(|e| format!("failed to build CLI schema: {e}"))?
        .cli(|cli| cli.strict())
        .help(|h| {
            h.program_name("moire-web")
                .description("SQLite-backed moire ingest + API server")
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

    println!("  moire-web ready ({mode})");
    println!();
    println!("  \x1b[32mOpen in browser: http://{http_addr}\x1b[0m");
    println!();
    println!("  Connect apps with:");
    println!("    \x1b[32mMOIRE_DASHBOARD={tcp_addr}\x1b[0m <your-binary>");
    println!();
    println!();
}

async fn start_vite_dev_server(vite_addr: &str) -> Result<Child, String> {
    let socket_addr = std::net::SocketAddr::from_str(vite_addr)
        .map_err(|e| format!("invalid MOIRE_VITE_ADDR '{vite_addr}': {e}"))?;
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
        .arg("moire-frontend")
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
    // When we (moire-web) die, the write end closes and the reaper wakes up.
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
        .arg("moire-frontend")
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
        .current_dir(workspace_root)
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
        .arg("moire-frontend")
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
            "pnpm install succeeded but vite is still unavailable for moire-frontend".to_string(),
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
    proxy_vite_request(proxy.base_url.as_str(), request, PROXY_BODY_LIMIT_BYTES).await
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
                handshake_received: false,
                module_manifest: Vec::new(),
                tx: msg_tx,
            },
        );
        conn_id
    };
    if let Err(e) =
        persist_connection_upsert(state.db.clone(), conn_id, format!("unknown-{conn_id}"), 0).await
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
    if let Err(e) = persist_connection_closed(state.db.clone(), conn_id).await {
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
    // r[impl wire.magic]
    let mut magic = [0u8; 4];
    reader
        .read_exact(&mut magic)
        .await
        .map_err(|e| format!("read protocol magic: {e}"))?;
    decode_protocol_magic(magic).map_err(|e| format!("invalid protocol magic: {e}"))?;

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
        if payload_len > moire_wire::DEFAULT_MAX_FRAME_BYTES {
            return Err(format!("frame too large: {payload_len}"));
        }

        let mut payload = vec![0u8; payload_len];
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|e| format!("read frame payload: {e}"))?;

        let mut framed = Vec::with_capacity(4 + payload_len);
        framed.extend_from_slice(&len_buf);
        framed.extend_from_slice(&payload);
        let message = decode_client_message_default(&framed)
            .map_err(|e| format!("decode client message: {e}"))?;

        match message {
            ClientMessage::Handshake(handshake) => {
                // r[impl wire.handshake.reject]
                validate_handshake(&handshake)
                    .map_err(|e| format!("reject handshake for conn {conn_id}: {e}"))?;
                let process_name = handshake.process_name.to_string();
                let pid = handshake.pid;
                let module_manifest_entries = handshake.module_manifest.len();
                let stored_manifest = into_stored_module_manifest(handshake.module_manifest);
                let mut guard = state.inner.lock().await;
                if let Some(conn) = guard.connections.get_mut(&conn_id) {
                    conn.process_name = process_name.clone();
                    conn.pid = pid;
                    conn.handshake_received = true;
                    conn.module_manifest = stored_manifest.clone();
                }
                drop(guard);
                if let Err(e) =
                    persist_connection_upsert(state.db.clone(), conn_id, process_name.clone(), pid)
                        .await
                {
                    warn!(conn_id, %e, "failed to persist handshake");
                }
                if let Err(e) =
                    persist_connection_module_manifest(state.db.clone(), conn_id, stored_manifest)
                        .await
                {
                    warn!(conn_id, %e, "failed to persist module manifest");
                }
                info!(
                    conn_id,
                    process_name, pid, module_manifest_entries, "handshake accepted"
                );
            }
            ClientMessage::SnapshotReply(reply) => {
                info!(
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
                if let Err(e) = persist_delta_batch(state.db.clone(), conn_id, batch).await {
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
                    info!(
                        conn_id,
                        cut_id = %cut_id,
                        pending_connections = cut.pending_conn_ids.len(),
                        acked_connections = cut.acks.len(),
                        "received cut ack"
                    );
                } else {
                    warn!(conn_id, cut_id = %cut_id, "received cut ack for unknown cut");
                }
                drop(guard);
                if let Err(e) = persist_cut_ack(
                    state.db.clone(),
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
            ClientMessage::BacktraceRecord(record) => {
                let (handshake_received, manifest) = {
                    let guard = state.inner.lock().await;
                    guard
                        .connections
                        .get(&conn_id)
                        .map(|conn| (conn.handshake_received, conn.module_manifest.clone()))
                        .ok_or_else(|| {
                            format!(
                                "invariant violated: unknown connection {conn_id} for backtrace {}",
                                record.id.get()
                            )
                        })?
                };
                if !handshake_received {
                    return Err(format!(
                        "protocol violation: received backtrace {} before handshake on conn {conn_id}",
                        record.id.get()
                    ));
                }
                // r[impl symbolicate.server-store]
                let backtrace_id = record.id.get();
                let frames = backtrace_frames_for_store(&manifest, &record)?;
                let unknown_module_frames = frames
                    .iter()
                    .filter(|frame| frame.module_path.starts_with("<unknown-module-id:"))
                    .count();
                if unknown_module_frames > 0 {
                    warn!(
                        conn_id,
                        backtrace_id,
                        total_frames = frames.len(),
                        unknown_module_frames,
                        "backtrace stored with unknown module ids from manifest"
                    );
                }
                let inserted =
                    persist_backtrace_record(state.db.clone(), conn_id, backtrace_id, frames)
                        .await?;
                if !inserted {
                    debug!(
                        conn_id,
                        backtrace_id, "backtrace already existed in storage"
                    );
                }
            }
        }
    }
}

fn validate_handshake(handshake: &moire_wire::Handshake) -> Result<(), String> {
    if handshake.process_name.trim().is_empty() {
        return Err("process_name must be non-empty".to_string());
    }

    for (index, module) in handshake.module_manifest.iter().enumerate() {
        if module.module_path.trim().is_empty() {
            return Err(format!(
                "module_manifest[{index}].module_path must be non-empty"
            ));
        }
        if !FsPath::new(module.module_path.as_str()).is_absolute() {
            return Err(format!(
                "module_manifest[{index}].module_path must be absolute"
            ));
        }
        if module.runtime_base == 0 {
            return Err(format!(
                "module_manifest[{index}].runtime_base must be non-zero"
            ));
        }
        if module.arch.trim().is_empty() {
            return Err(format!("module_manifest[{index}].arch must be non-empty"));
        }
        match &module.identity {
            moire_wire::ModuleIdentity::BuildId(build_id) => {
                if build_id.trim().is_empty() {
                    return Err(format!(
                        "module_manifest[{index}].identity.build_id must be non-empty"
                    ));
                }
            }
            moire_wire::ModuleIdentity::DebugId(debug_id) => {
                if debug_id.trim().is_empty() {
                    return Err(format!(
                        "module_manifest[{index}].identity.debug_id must be non-empty"
                    ));
                }
            }
        }
    }

    Ok(())
}
