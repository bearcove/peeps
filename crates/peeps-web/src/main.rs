mod api;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::Router;
use peeps_types::{GraphReply, SnapshotRequest};
use rusqlite::{params, Connection};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, error, info, warn};

// ── Types ────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db_path: Arc<PathBuf>,
    pub(crate) snapshot_ctl: Arc<SnapshotController>,
}

pub(crate) struct SnapshotController {
    pub(crate) inner: Mutex<SnapshotControllerInner>,
}

pub(crate) struct SnapshotControllerInner {
    pub(crate) connections: HashMap<u64, ConnectedProcess>,
    next_conn_id: u64,
    pub(crate) in_flight: Option<InFlightSnapshot>,
}

pub(crate) struct ConnectedProcess {
    pub(crate) proc_key: String,
    pub(crate) process_name: String,
    tx: tokio::sync::mpsc::Sender<Vec<u8>>,
}

pub(crate) struct InFlightSnapshot {
    pub(crate) snapshot_id: i64,
    pub(crate) requested_at_ns: i64,
    pub(crate) timeout_ms: i64,
    pub(crate) requested: BTreeSet<String>,
    pub(crate) pending: BTreeSet<String>,
    completion_tx: Option<oneshot::Sender<()>>,
}

use peeps_types::GraphSnapshot;

pub(crate) const DEFAULT_TIMEOUT_MS: i64 = 5000;
const MAX_SNAPSHOTS: i64 = 500;
const INGEST_EVENTS_RETENTION_DAYS: i64 = 7;
const EVENTS_RETENTION_DAYS: i64 = 7;
const FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
  <style>
    path { fill: #111827; }
    @media (prefers-color-scheme: dark) { path { fill: #f9fafb; } }
  </style>
  <path d="M201.54,54.46A104,104,0,0,0,54.46,201.54,104,104,0,0,0,201.54,54.46ZM190.23,65.78a88.18,88.18,0,0,1,11,13.48L167.55,119,139.63,40.78A87.34,87.34,0,0,1,190.23,65.78ZM155.59,133l-18.16,21.37-27.59-5L100.41,123l18.16-21.37,27.59,5ZM65.77,65.78a87.34,87.34,0,0,1,56.66-25.59l17.51,49L58.3,74.32A88,88,0,0,1,65.77,65.78ZM46.65,161.54a88.41,88.41,0,0,1,2.53-72.62l51.21,9.35Zm19.12,28.68a88.18,88.18,0,0,1-11-13.48L88.45,137l27.92,78.18A87.34,87.34,0,0,1,65.77,190.22Zm124.46,0a87.34,87.34,0,0,1-56.66,25.59l-17.51-49,81.64,14.91A88,88,0,0,1,190.23,190.22Zm-34.62-32.49,53.74-63.27a88.41,88.41,0,0,1-2.53,72.62Z"/>
</svg>
"##;

// ── Main ─────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let tcp_addr = std::env::var("PEEPS_LISTEN").unwrap_or_else(|_| "127.0.0.1:9119".into());
    let http_addr = std::env::var("PEEPS_HTTP").unwrap_or_else(|_| "127.0.0.1:9130".into());
    let db_path = std::env::var("PEEPS_DB").unwrap_or_else(|_| "./peeps-web.sqlite".into());

    init_db(&db_path).expect("init sqlite schema");

    let state = AppState {
        db_path: Arc::new(PathBuf::from(&db_path)),
        snapshot_ctl: Arc::new(SnapshotController {
            inner: Mutex::new(SnapshotControllerInner {
                connections: HashMap::new(),
                next_conn_id: 1,
                in_flight: None,
            }),
        }),
    };

    let tcp_listener = TcpListener::bind(&tcp_addr)
        .await
        .unwrap_or_else(|e| panic!("[peeps-web] failed to bind TCP on {tcp_addr}: {e}"));
    info!(%tcp_addr, "TCP listener ready (pull-based ingest)");

    let http_listener = TcpListener::bind(&http_addr)
        .await
        .unwrap_or_else(|e| panic!("[peeps-web] failed to bind HTTP on {http_addr}: {e}"));
    info!(%http_addr, "HTTP server ready");
    info!(%db_path, "sqlite DB");

    let app = Router::new()
        .route("/health", get(health))
        .route("/favicon.svg", get(favicon_svg))
        .route("/favicon.ico", get(favicon_ico))
        .route("/api/jump-now", post(api::api_jump_now))
        .route("/api/snapshot-progress", get(api::api_snapshot_progress))
        .route("/api/sql", post(api::api_sql))
        .with_state(state.clone());

    tokio::select! {
        _ = run_tcp_acceptor(tcp_listener, state.clone()) => {}
        result = axum::serve(http_listener, app) => {
            if let Err(e) = result {
                error!(%e, "HTTP server error");
            }
        }
    }
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn favicon_svg() -> impl IntoResponse {
    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                "image/svg+xml; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        FAVICON_SVG,
    )
}

async fn favicon_ico() -> Redirect {
    Redirect::permanent("/favicon.svg")
}

// ── Snapshot orchestration (used by api module) ──────────────────

pub(crate) fn allocate_snapshot_id(
    db_path: &PathBuf,
    now_ns: i64,
    timeout_ms: i64,
) -> Result<i64, String> {
    let conn = open_db(db_path);
    conn.execute(
        "INSERT INTO snapshots (requested_at_ns, timeout_ms) VALUES (?1, ?2)",
        params![now_ns, timeout_ms],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub(crate) async fn trigger_snapshot(state: &AppState) -> Result<(i64, usize), String> {
    let (snapshot_id, completion_rx, processes_requested) = {
        let mut ctl = state.snapshot_ctl.inner.lock().await;

        if ctl.in_flight.is_some() {
            return Err("snapshot already in flight".into());
        }

        if ctl.connections.is_empty() {
            return Err("no connected processes".into());
        }

        let now_ns = now_nanos();
        let snapshot_id = allocate_snapshot_id(&state.db_path, now_ns, DEFAULT_TIMEOUT_MS)?;

        let pending: BTreeSet<String> = ctl
            .connections
            .values()
            .map(|c| c.proc_key.clone())
            .collect();
        let processes_requested = pending.len();

        let (completion_tx, completion_rx) = oneshot::channel();

        ctl.in_flight = Some(InFlightSnapshot {
            snapshot_id,
            requested_at_ns: now_ns,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            requested: pending.clone(),
            pending: pending.clone(),
            completion_tx: Some(completion_tx),
        });

        info!(
            snapshot_id,
            ?pending,
            "triggering snapshot for {} processes",
            pending.len()
        );

        let req = SnapshotRequest {
            r#type: "snapshot_request".to_string(),
            snapshot_id,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        };
        let req_json = facet_json::to_vec(&req).map_err(|e| e.to_string())?;

        for conn in ctl.connections.values() {
            if let Err(e) = conn.tx.try_send(req_json.clone()) {
                error!(proc_key = %conn.proc_key, %e, "failed to send snapshot request");
            }
        }

        (snapshot_id, completion_rx, processes_requested)
    };

    let timeout = tokio::time::Duration::from_millis(DEFAULT_TIMEOUT_MS as u64 + 500);
    let _ = tokio::time::timeout(timeout, completion_rx).await;

    finalize_snapshot(&state.db_path, &state.snapshot_ctl, snapshot_id).await?;

    if let Err(e) = run_retention(&state.db_path) {
        warn!(%e, "retention error");
    }

    Ok((snapshot_id, processes_requested))
}

async fn finalize_snapshot(
    db_path: &PathBuf,
    ctl: &SnapshotController,
    snapshot_id: i64,
) -> Result<(), String> {
    let mut guard = ctl.inner.lock().await;
    let in_flight = match guard.in_flight.take() {
        Some(f) if f.snapshot_id == snapshot_id => f,
        other => {
            guard.in_flight = other;
            return Err("snapshot_id mismatch during finalize".into());
        }
    };

    let conn = open_db(db_path);
    let now_ns = now_nanos();

    if !in_flight.pending.is_empty() {
        warn!(snapshot_id, pending = ?in_flight.pending, "finalizing with {} unresponsive processes", in_flight.pending.len());
    }

    for proc_key in &in_flight.pending {
        let still_connected = guard.connections.values().any(|c| &c.proc_key == proc_key);
        let status = if still_connected {
            "timeout"
        } else {
            "disconnected"
        };
        warn!(snapshot_id, %proc_key, %status, "process did not respond");

        conn.execute(
            "INSERT OR IGNORE INTO snapshot_processes (snapshot_id, process, pid, proc_key, status, error_text)
             VALUES (?1, '', NULL, ?2, ?3, NULL)",
            params![snapshot_id, proc_key, status],
        )
        .map_err(|e| e.to_string())?;
    }

    conn.execute(
        "UPDATE snapshots SET completed_at_ns = ?1 WHERE snapshot_id = ?2",
        params![now_ns, snapshot_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

// ── TCP acceptor + connection handler ────────────────────────────

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
            Err(e) => {
                error!(%e, "TCP accept failed");
            }
        }
    }
}

async fn handle_conn(stream: TcpStream, state: AppState) -> Result<(), String> {
    let (mut reader, mut writer) = stream.into_split();

    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(32);
    let conn_id = {
        let mut ctl = state.snapshot_ctl.inner.lock().await;
        let id = ctl.next_conn_id;
        ctl.next_conn_id += 1;
        ctl.connections.insert(
            id,
            ConnectedProcess {
                proc_key: format!("unknown-{id}"),
                process_name: String::new(),
                tx: msg_tx,
            },
        );
        id
    };

    let writer_handle = tokio::spawn(async move {
        while let Some(payload) = msg_rx.recv().await {
            let len = (payload.len() as u32).to_be_bytes();
            if writer.write_all(&len).await.is_err() {
                break;
            }
            if writer.write_all(&payload).await.is_err() {
                break;
            }
        }
    });

    let result = read_replies(&mut reader, conn_id, &state).await;

    {
        let mut ctl = state.snapshot_ctl.inner.lock().await;
        ctl.connections.remove(&conn_id);
    }
    writer_handle.abort();

    result
}

async fn read_replies(
    reader: &mut tokio::net::tcp::OwnedReadHalf,
    conn_id: u64,
    state: &AppState,
) -> Result<(), String> {
    debug!(conn_id, "waiting for replies");
    loop {
        let mut len_buf = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut len_buf).await {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                debug!(conn_id, "connection closed (EOF)");
                return Ok(());
            }
            return Err(format!("read frame len: {e}"));
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        debug!(conn_id, frame_len = len, "received frame header");
        if len > 128 * 1024 * 1024 {
            return Err(format!("frame too large: {len} bytes"));
        }
        let mut frame = vec![0u8; len];
        reader
            .read_exact(&mut frame)
            .await
            .map_err(|e| format!("read frame payload: {e}"))?;

        debug!(
            conn_id,
            frame_len = len,
            "received full frame, deserializing"
        );
        let reply: GraphReply = match facet_json::from_slice(&frame) {
            Ok(r) => {
                debug!(conn_id, "deserialized graph reply OK");
                r
            }
            Err(e) => {
                warn!(conn_id, %e, "failed to deserialize graph reply");
                record_ingest_event(
                    &state.db_path,
                    None,
                    None,
                    None,
                    None,
                    "decode_error",
                    &format!("JSON decode failed: {e}"),
                );
                continue;
            }
        };

        if reply.r#type != "graph_reply" {
            warn!(conn_id, msg_type = %reply.r#type, "unexpected message type");
            record_ingest_event(
                &state.db_path,
                None,
                Some(&reply.process),
                Some(reply.pid),
                None,
                "other",
                &format!("unexpected message type: {}", reply.r#type),
            );
            continue;
        }

        let proc_key = peeps_types::make_proc_key(&reply.process, reply.pid);

        {
            let mut ctl = state.snapshot_ctl.inner.lock().await;
            if let Some(conn) = ctl.connections.get_mut(&conn_id) {
                conn.proc_key = proc_key.clone();
                conn.process_name = reply.process.clone();
            }
        }

        process_reply(state, &reply, &proc_key).await;
    }
}

async fn process_reply(state: &AppState, reply: &GraphReply, proc_key: &str) {
    let now_ns = now_nanos();

    let snapshot_id = {
        let ctl = state.snapshot_ctl.inner.lock().await;
        match &ctl.in_flight {
            Some(f) if f.snapshot_id == reply.snapshot_id => f.snapshot_id,
            Some(f) => {
                let expected = f.snapshot_id;
                warn!(
                    %proc_key,
                    expected_snapshot_id = expected,
                    got_snapshot_id = reply.snapshot_id,
                    "snapshot_id mismatch"
                );
                record_ingest_event(
                    &state.db_path,
                    Some(reply.snapshot_id),
                    Some(&reply.process),
                    Some(reply.pid),
                    Some(proc_key),
                    "snapshot_id_mismatch",
                    &format!("expected snapshot_id={expected}, got {}", reply.snapshot_id),
                );
                return;
            }
            None => {
                warn!(
                    %proc_key,
                    snapshot_id = reply.snapshot_id,
                    "late reply, no in-flight snapshot"
                );
                record_ingest_event(
                    &state.db_path,
                    Some(reply.snapshot_id),
                    Some(&reply.process),
                    Some(reply.pid),
                    Some(proc_key),
                    "late_reply",
                    &format!(
                        "no in-flight snapshot, reply for snapshot_id={}",
                        reply.snapshot_id
                    ),
                );
                return;
            }
        }
    };

    let graph = reply.graph.as_ref();

    if let Err(e) = persist_reply(
        &state.db_path,
        snapshot_id,
        &reply.process,
        reply.pid,
        proc_key,
        now_ns,
        graph,
    ) {
        record_ingest_event(
            &state.db_path,
            Some(snapshot_id),
            Some(&reply.process),
            Some(reply.pid),
            Some(proc_key),
            "other",
            &format!("persist failed: {e}"),
        );
    } else {
        let (node_count, edge_count) = graph
            .map(|g| (g.nodes.len(), g.edges.len()))
            .unwrap_or((0, 0));
        info!(
            snapshot_id,
            process = %reply.process,
            %proc_key,
            node_count,
            edge_count,
            "snapshot reply persisted"
        );
    }

    // Always mark the process as responded, even if persist failed —
    // the process DID reply, we just couldn't store it.
    let mut ctl = state.snapshot_ctl.inner.lock().await;
    if let Some(ref mut in_flight) = ctl.in_flight {
        if in_flight.snapshot_id == snapshot_id {
            in_flight.pending.remove(proc_key);
            if in_flight.pending.is_empty() {
                if let Some(tx) = in_flight.completion_tx.take() {
                    let _ = tx.send(());
                }
            }
        }
    }
}

// ── Reply persistence ────────────────────────────────────────────

fn persist_reply(
    db_path: &PathBuf,
    snapshot_id: i64,
    process: &str,
    pid: u32,
    proc_key: &str,
    recv_at_ns: i64,
    graph: Option<&GraphSnapshot>,
) -> Result<(), String> {
    let mut conn = open_db(db_path);
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT OR REPLACE INTO snapshot_processes (snapshot_id, process, pid, proc_key, status, recv_at_ns)
         VALUES (?1, ?2, ?3, ?4, 'responded', ?5)",
        params![snapshot_id, process, pid, proc_key, recv_at_ns],
    )
    .map_err(|e| e.to_string())?;

    if let Some(graph) = graph {
        for node in &graph.nodes {
            tx.execute(
                "INSERT OR REPLACE INTO nodes (snapshot_id, id, kind, process, proc_key, attrs_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    snapshot_id,
                    node.id,
                    node.kind.as_str(),
                    process,
                    proc_key,
                    node.attrs_json
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        for edge in &graph.edges {
            tx.execute(
                "INSERT OR REPLACE INTO edges (snapshot_id, src_id, dst_id, kind, attrs_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    snapshot_id,
                    edge.src,
                    edge.dst,
                    edge.kind.as_str(),
                    edge.attrs_json
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        if let Some(events) = &graph.events {
            for event in events {
                let ts_ns = if event.ts_ns > i64::MAX as u64 {
                    i64::MAX
                } else {
                    event.ts_ns as i64
                };
                tx.execute(
                    "INSERT OR REPLACE INTO events (id, ts_ns, proc_key, entity_id, name, parent_entity_id, attrs_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        &event.id,
                        ts_ns,
                        &event.proc_key,
                        &event.entity_id,
                        &event.name,
                        event.parent_entity_id.as_deref(),
                        &event.attrs_json
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    tx.commit().map_err(|e| e.to_string())
}

// ── Ingest events ────────────────────────────────────────────────

fn record_ingest_event(
    db_path: &PathBuf,
    snapshot_id: Option<i64>,
    process: Option<&str>,
    pid: Option<u32>,
    proc_key: Option<&str>,
    event_kind: &str,
    detail: &str,
) {
    let conn = open_db(db_path);
    let now_ns = now_nanos();
    if let Err(e) = conn.execute(
        "INSERT INTO ingest_events (event_at_ns, snapshot_id, process, pid, proc_key, event_kind, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![now_ns, snapshot_id, process, pid, proc_key, event_kind, detail],
    ) {
        error!(%e, %event_kind, "failed to record ingest event");
    }
}

// ── Retention ────────────────────────────────────────────────────

pub(crate) fn run_retention(db_path: &PathBuf) -> Result<(), String> {
    let conn = open_db(db_path);

    let cutoff: Option<i64> = conn
        .query_row(
            "SELECT snapshot_id FROM snapshots ORDER BY snapshot_id DESC LIMIT 1 OFFSET ?1",
            params![MAX_SNAPSHOTS],
            |row| row.get(0),
        )
        .ok();

    if let Some(cutoff_id) = cutoff {
        conn.execute_batch(&format!(
            "DELETE FROM edges WHERE snapshot_id <= {cutoff_id};
             DELETE FROM nodes WHERE snapshot_id <= {cutoff_id};
             DELETE FROM snapshot_processes WHERE snapshot_id <= {cutoff_id};
             DELETE FROM snapshots WHERE snapshot_id <= {cutoff_id};"
        ))
        .map_err(|e| e.to_string())?;
    }

    let cutoff_ns = now_nanos() - (INGEST_EVENTS_RETENTION_DAYS * 24 * 60 * 60 * 1_000_000_000);
    conn.execute(
        "DELETE FROM ingest_events WHERE event_at_ns < ?1",
        params![cutoff_ns],
    )
    .map_err(|e| e.to_string())?;

    let events_cutoff_ns = now_nanos() - (EVENTS_RETENTION_DAYS * 24 * 60 * 60 * 1_000_000_000);
    conn.execute(
        "DELETE FROM events WHERE ts_ns < ?1",
        params![events_cutoff_ns],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

// ── SQLite init / open ───────────────────────────────────────────

fn init_db(path: &str) -> rusqlite::Result<()> {
    let conn = Connection::open(path)?;

    // Migrate: drop the edges table if it has an outdated schema.
    // This is safe because snapshot data is ephemeral.
    let needs_migration: bool = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='edges'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .is_some_and(|sql| {
            // Old PK without kind column, or CHECK constraint that restricts edge kinds
            !sql.contains("src_id, dst_id, kind)") || sql.contains("CHECK")
        });
    if needs_migration {
        conn.execute_batch("DROP TABLE IF EXISTS edges;")?;
    }

    // Migrate: drop the events table if it has an outdated schema.
    // This is safe because runtime event diagnostics are ephemeral.
    let events_needs_migration: bool = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='events'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .is_some_and(|sql| {
            !sql.contains("id TEXT PRIMARY KEY")
                || !sql.contains("ts_ns INTEGER NOT NULL")
                || !sql.contains("proc_key TEXT NOT NULL")
                || !sql.contains("entity_id TEXT NOT NULL")
                || !sql.contains("name TEXT NOT NULL")
                || !sql.contains("parent_entity_id TEXT")
                || !sql.contains("attrs_json TEXT NOT NULL")
        });
    if events_needs_migration {
        conn.execute_batch("DROP TABLE IF EXISTS events;")?;
    }

    // Legacy table from an abandoned unresolved-edge pipeline.
    // Ghost endpoint synthesis now happens in the frontend from dangling edges.
    conn.execute_batch("DROP TABLE IF EXISTS unresolved_edges;")?;

    conn.execute_batch(
        "
        PRAGMA journal_mode=WAL;
        PRAGMA synchronous=NORMAL;

        CREATE TABLE IF NOT EXISTS snapshots (
            snapshot_id INTEGER PRIMARY KEY,
            requested_at_ns INTEGER NOT NULL,
            completed_at_ns INTEGER,
            timeout_ms INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS snapshot_processes (
            snapshot_id INTEGER NOT NULL,
            process TEXT NOT NULL,
            pid INTEGER,
            proc_key TEXT NOT NULL,
            status TEXT NOT NULL,
            recv_at_ns INTEGER,
            error_text TEXT,
            PRIMARY KEY (snapshot_id, proc_key)
        );

        CREATE TABLE IF NOT EXISTS nodes (
            snapshot_id INTEGER NOT NULL,
            id TEXT NOT NULL,
            kind TEXT NOT NULL,
            process TEXT NOT NULL,
            proc_key TEXT NOT NULL,
            attrs_json TEXT NOT NULL,
            PRIMARY KEY (snapshot_id, id)
        );

        CREATE TABLE IF NOT EXISTS edges (
            snapshot_id INTEGER NOT NULL,
            src_id TEXT NOT NULL,
            dst_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            attrs_json TEXT NOT NULL,
            PRIMARY KEY (snapshot_id, src_id, dst_id, kind)
        );

        CREATE TABLE IF NOT EXISTS ingest_events (
            event_id INTEGER PRIMARY KEY,
            event_at_ns INTEGER NOT NULL,
            snapshot_id INTEGER,
            process TEXT,
            pid INTEGER,
            proc_key TEXT,
            event_kind TEXT NOT NULL,
            detail TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS events (
            id TEXT PRIMARY KEY,
            ts_ns INTEGER NOT NULL,
            proc_key TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            name TEXT NOT NULL,
            parent_entity_id TEXT,
            attrs_json TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_nodes_snapshot_kind ON nodes(snapshot_id, kind);
        CREATE INDEX IF NOT EXISTS idx_nodes_snapshot_proc_key ON nodes(snapshot_id, proc_key);
        CREATE INDEX IF NOT EXISTS idx_edges_snapshot_src ON edges(snapshot_id, src_id);
        CREATE INDEX IF NOT EXISTS idx_edges_snapshot_dst ON edges(snapshot_id, dst_id);
        CREATE INDEX IF NOT EXISTS idx_events_proc_entity_ts ON events(proc_key, entity_id, ts_ns DESC);
        CREATE INDEX IF NOT EXISTS idx_events_proc_parent_ts ON events(proc_key, parent_entity_id, ts_ns DESC);
        CREATE INDEX IF NOT EXISTS idx_events_ts_id ON events(ts_ns DESC, id DESC);
        ",
    )?;
    Ok(())
}

pub(crate) fn open_db(path: &PathBuf) -> Connection {
    Connection::open(path).expect("open sqlite")
}

// ── Helpers ──────────────────────────────────────────────────────

pub(crate) fn now_nanos() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64
}
