use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::{self, Body, Bytes};
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use axum::Router;
use compact_str::CompactString;
use facet::Facet;
use figue as args;
use peeps_types::Change;
use peeps_wire::{
    decode_client_message_default, encode_server_message_default, ClientMessage, ServerMessage,
};
use rusqlite::{params, Connection};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Child;
use tokio::sync::{mpsc, Mutex};
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
    connections: HashMap<u64, ConnectedProcess>,
    cuts: BTreeMap<String, CutState>,
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

#[derive(Facet)]
struct ConnectionsResponse {
    connected_processes: usize,
    processes: Vec<ConnectedProcessInfo>,
}

#[derive(Facet)]
struct ConnectedProcessInfo {
    conn_id: u64,
    process_name: String,
    pid: u32,
}

#[derive(Facet)]
struct TriggerCutResponse {
    cut_id: String,
    requested_at_ns: i64,
    requested_connections: usize,
}

#[derive(Facet)]
struct CutStatusResponse {
    cut_id: String,
    requested_at_ns: i64,
    pending_connections: usize,
    acked_connections: usize,
    pending_conn_ids: Vec<u64>,
}

#[derive(Facet)]
struct ApiError {
    error: String,
}

#[derive(Facet)]
struct SqlRequest {
    sql: CompactString,
}

#[derive(Facet)]
struct QueryRequest {
    name: CompactString,
    #[facet(skip_unless_truthy)]
    limit: Option<u32>,
}

#[derive(Facet)]
struct SqlResponse {
    columns: Vec<CompactString>,
    rows: Vec<facet_value::Value>,
    row_count: u32,
}

#[derive(Facet)]
struct SnapshotResponse {
    entities: Vec<peeps_types::Entity>,
    edges: Vec<SnapshotEdge>,
}

#[derive(Facet)]
struct SnapshotEdge {
    src_id: String,
    dst_id: String,
    kind: String,
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

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
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
            connections: HashMap::new(),
            cuts: BTreeMap::new(),
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
        .route("/api/snapshot", post(api_snapshot));
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
        let cut_id = peeps_types::CutId(CompactString::from(cut_id_string.as_str()));
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
    let db_path = state.db_path.clone();
    match tokio::task::spawn_blocking(move || snapshot_blocking(&db_path)).await {
        Ok(Ok(resp)) => json_ok(&resp),
        Ok(Err(err)) => json_error(StatusCode::INTERNAL_SERVER_ERROR, err),
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("snapshot worker join error: {e}"),
        ),
    }
}

fn snapshot_blocking(db_path: &PathBuf) -> Result<SnapshotResponse, String> {
    let conn = Connection::open(db_path).map_err(|e| format!("open sqlite: {e}"))?;

    let entity_jsons: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT entity_json FROM entities")
            .map_err(|e| format!("prepare entities: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("query entities: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("read entity row: {e}"))?;
        rows
    };
    let entities: Vec<peeps_types::Entity> = entity_jsons
        .iter()
        .map(|json| {
            facet_json::from_slice(json.as_bytes()).map_err(|e| format!("parse entity: {e}"))
        })
        .collect::<Result<_, _>>()?;

    let raw_edges: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT src_id, dst_id, kind_json FROM edges")
            .map_err(|e| format!("prepare edges: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("query edges: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("read edge row: {e}"))?;
        rows
    };
    let edges: Vec<SnapshotEdge> = raw_edges
        .into_iter()
        .map(|(src_id, dst_id, kind_json)| {
            let kind: String = facet_json::from_slice(kind_json.as_bytes())
                .map_err(|e| format!("parse kind: {e}"))?;
            Ok(SnapshotEdge { src_id, dst_id, kind })
        })
        .collect::<Result<_, String>>()?;

    Ok(SnapshotResponse { entities, edges })
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

    let child = command.spawn().map_err(|e| {
        format!(
            "failed to launch Vite via pnpm in {}: {e}",
            workspace_root.display()
        )
    })?;
    wait_for_tcp_ready(vite_addr, Duration::from_secs(20)).await?;
    Ok(child)
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

    {
        let mut guard = state.inner.lock().await;
        guard.connections.remove(&conn_id);
        for cut in guard.cuts.values_mut() {
            cut.pending_conn_ids.remove(&conn_id);
            cut.acks.remove(&conn_id);
        }
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
                    process_name = %reply.process_name,
                    has_snapshot = reply.snapshot.is_some(),
                    "received snapshot reply"
                );
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

fn to_i64_u64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
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
    let columns: Vec<CompactString> = (0..column_count)
        .map(|i| CompactString::from(stmt.column_name(i).unwrap_or("?")))
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
            "unknown query pack: {name}. expected one of: blockers, blocked-senders, blocked-receivers, stalled-sends, channel-pressure, channel-health, scope-membership, stale-blockers"
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
