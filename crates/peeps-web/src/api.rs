//! HTTP API endpoints: POST /api/jump-now, POST /api/sql, GET /api/validate/:snapshot_id
//!
//! SQL enforcement: authorizer, progress handler, hard caps.

use std::path::PathBuf;
use std::time::Instant;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rusqlite::types::Value;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::AppState;

// ── Constants ────────────────────────────────────────────────────

const MAX_ROWS: usize = 5000;
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024; // 4 MiB
const MAX_EXECUTION_MS: u64 = 750;
/// Progress handler callback interval (in SQLite virtual-machine ops).
const PROGRESS_HANDLER_OPS: i32 = 1000;

// ── Scoped TEMP VIEW tables ──────────────────────────────────────

/// Tables that get scoped TEMP VIEWs and are blocked from direct access.
/// Each entry is (table_name, columns_excluding_snapshot_id).
const SCOPED_TABLES: &[(&str, &str)] = &[
    ("nodes", "id, kind, process, proc_key, attrs_json"),
    ("edges", "src_id, dst_id, kind, attrs_json"),
    (
        "unresolved_edges",
        "src_id, dst_id, missing_side, reason, referenced_proc_key, attrs_json",
    ),
    (
        "snapshot_processes",
        "process, pid, proc_key, status, recv_at_ns, error_text",
    ),
];

// ── Request/response types ───────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct JumpNowResponse {
    pub snapshot_id: i64,
    pub requested: usize,
    pub responded: usize,
    pub timed_out: usize,
}

#[derive(Debug, Deserialize)]
pub struct SqlRequest {
    pub snapshot_id: i64,
    pub sql: String,
    #[serde(default)]
    pub params: Vec<JsonValue>,
}

#[derive(Debug, Serialize)]
pub struct SqlResponse {
    pub snapshot_id: i64,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
    pub row_count: usize,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiError {
    error: String,
}

fn api_error(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: msg.into() }))
}

// ── POST /api/jump-now ───────────────────────────────────────────

pub async fn api_jump_now(
    State(state): State<AppState>,
) -> Result<Json<JumpNowResponse>, (StatusCode, Json<ApiError>)> {
    let (snapshot_id, processes_requested) = crate::trigger_snapshot(&state)
        .await
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Read back process statuses for the response
    let db_path = state.db_path.clone();
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&*db_path)
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("db open: {e}")))?;

        let mut responded = 0usize;
        let mut timed_out = 0usize;

        let mut stmt = conn
            .prepare("SELECT status, COUNT(*) FROM snapshot_processes WHERE snapshot_id = ?1 GROUP BY status")
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("prepare: {e}")))?;

        let rows = stmt
            .query_map(params![snapshot_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
            })
            .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("query: {e}")))?;

        for row in rows {
            let (status, count) = row.map_err(|e| {
                api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("row: {e}"))
            })?;
            match status.as_str() {
                "responded" => responded += count,
                "timeout" => timed_out += count,
                _ => {}
            }
        }

        Ok(Json(JumpNowResponse {
            snapshot_id,
            requested: processes_requested,
            responded,
            timed_out,
        }))
    })
    .await
    .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("join: {e}")))?
}

// ── POST /api/sql ────────────────────────────────────────────────

pub async fn api_sql(
    State(state): State<AppState>,
    Json(req): Json<SqlRequest>,
) -> Result<Json<SqlResponse>, (StatusCode, Json<ApiError>)> {
    let db_path = state.db_path.clone();
    let result = tokio::task::spawn_blocking(move || sql_blocking(&db_path, req))
        .await
        .map_err(|e| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("join error: {e}"),
            )
        })?;
    result
}

fn sql_blocking(
    db_path: &PathBuf,
    req: SqlRequest,
) -> Result<Json<SqlResponse>, (StatusCode, Json<ApiError>)> {
    let conn = Connection::open(db_path)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, format!("db open: {e}")))?;

    // 1. Create scoped TEMP VIEWs for this snapshot_id
    create_scoped_views(&conn, req.snapshot_id).map_err(|e| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("create views: {e}"),
        )
    })?;

    // 2. Install authorizer to block direct main.* table access
    install_authorizer(&conn);

    // 3. Install progress handler for execution time limit
    let deadline = Instant::now() + std::time::Duration::from_millis(MAX_EXECUTION_MS);
    install_progress_handler(&conn, deadline);

    // 4. Reject multiple statements and direct main schema access
    let sql = req.sql.trim();
    if sql.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "empty SQL"));
    }
    reject_multiple_statements(sql)?;
    reject_main_schema_access(sql)?;

    // 5. Prepare the statement
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, format!("prepare error: {e}")))?;

    // 6. Bind parameters
    let param_values = convert_params(&req.params)?;
    for (i, val) in param_values.iter().enumerate() {
        stmt.raw_bind_parameter(i + 1, val).map_err(|e| {
            api_error(
                StatusCode::BAD_REQUEST,
                format!("bind param {}: {e}", i + 1),
            )
        })?;
    }

    // 7. Execute with row/byte caps
    let column_count = stmt.column_count();
    let columns: Vec<String> = (0..column_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();

    let mut rows: Vec<Vec<JsonValue>> = Vec::new();
    let mut total_bytes: usize = 0;
    let mut truncated = false;

    let mut raw_rows = stmt.raw_query();
    loop {
        let row = match raw_rows.next() {
            Ok(Some(row)) => row,
            Ok(None) => break,
            Err(e) => {
                if is_interrupt_error(&e) {
                    truncated = true;
                    break;
                }
                return Err(api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("query error: {e}"),
                ));
            }
        };

        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }

        let mut row_values = Vec::with_capacity(column_count);
        for i in 0..column_count {
            let val = sqlite_value_to_json(row, i);
            row_values.push(val);
        }

        let row_json = serde_json::to_string(&row_values).unwrap_or_default();
        total_bytes += row_json.len();
        if total_bytes > MAX_RESPONSE_BYTES {
            truncated = true;
            break;
        }

        rows.push(row_values);
    }

    let row_count = rows.len();

    Ok(Json(SqlResponse {
        snapshot_id: req.snapshot_id,
        columns,
        rows,
        row_count,
        truncated,
    }))
}

// ── Multiple statement rejection ─────────────────────────────────

/// Reject SQL input that contains more than one statement.
///
/// Scans for a semicolon that is followed by non-whitespace characters,
/// skipping quoted strings and comments.
fn reject_multiple_statements(sql: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let mut found_semicolon = false;

    while i < len {
        match bytes[i] {
            // Skip single-quoted strings
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\'' {
                        i += 1;
                        if i < len && bytes[i] == b'\'' {
                            i += 1; // escaped quote
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            // Skip double-quoted identifiers
            b'"' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'"' {
                        i += 1;
                        if i < len && bytes[i] == b'"' {
                            i += 1;
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            // Skip -- line comments
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
            }
            // Skip /* block comments */
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < len {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            b';' => {
                found_semicolon = true;
                i += 1;
            }
            _ => {
                if found_semicolon && !bytes[i].is_ascii_whitespace() {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "multiple statements not allowed",
                    ));
                }
                i += 1;
            }
        }
    }
    Ok(())
}

/// Reject queries that try to bypass scoped TEMP VIEWs by referencing `main.*` directly.
fn reject_main_schema_access(sql: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    // Case-insensitive check for `main.` prefix on scoped table names.
    let lower = sql.to_ascii_lowercase();
    for (table, _) in SCOPED_TABLES {
        if lower.contains(&format!("main.{table}")) || lower.contains(&format!("main.[{table}]")) {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("direct access to main.{table} is not allowed; use {table} instead"),
            ));
        }
    }
    Ok(())
}

// ── Scoped TEMP VIEWs ───────────────────────────────────────────

fn create_scoped_views(conn: &Connection, snapshot_id: i64) -> rusqlite::Result<()> {
    for (table, cols) in SCOPED_TABLES {
        conn.execute_batch(&format!(
            "CREATE TEMP VIEW [{table}] AS SELECT {cols} FROM main.[{table}] WHERE snapshot_id = {snapshot_id}"
        ))?;
    }
    Ok(())
}

// ── SQLite authorizer ────────────────────────────────────────────

fn install_authorizer(conn: &Connection) {
    conn.authorizer(Some(|ctx: rusqlite::hooks::AuthContext<'_>| {
        use rusqlite::hooks::{AuthAction, Authorization};

        match ctx.action {
            AuthAction::Read { table_name, .. } => {
                // Block sqlite_master reads
                if table_name == "sqlite_master" || table_name == "sqlite_temp_master" {
                    return Authorization::Deny;
                }

                // Allow all column reads — scoped TEMP VIEWs shadow the main
                // table names so user queries go through the view. We block
                // direct `main.*` access via reject_main_schema_access().
                Authorization::Allow
            }
            AuthAction::Select => Authorization::Allow,
            AuthAction::Function { .. } => Authorization::Allow,
            AuthAction::Recursive => Authorization::Allow,
            // Block everything else
            _ => Authorization::Deny,
        }
    }));
}

// ── Progress handler (execution time limit) ──────────────────────

fn install_progress_handler(conn: &Connection, deadline: Instant) {
    conn.progress_handler(
        PROGRESS_HANDLER_OPS,
        Some(move || Instant::now() > deadline),
    );
}

fn is_interrupt_error(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::OperationInterrupted,
                ..
            },
            _
        )
    )
}

// ── Parameter conversion ─────────────────────────────────────────

fn convert_params(params: &[JsonValue]) -> Result<Vec<Value>, (StatusCode, Json<ApiError>)> {
    params
        .iter()
        .enumerate()
        .map(|(i, v)| match v {
            JsonValue::Null => Ok(Value::Null),
            JsonValue::Bool(b) => Ok(Value::Integer(if *b { 1 } else { 0 })),
            JsonValue::Number(n) => {
                if let Some(int) = n.as_i64() {
                    Ok(Value::Integer(int))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Real(f))
                } else {
                    Err(api_error(
                        StatusCode::BAD_REQUEST,
                        format!("param {}: unsupported number", i + 1),
                    ))
                }
            }
            JsonValue::String(s) => Ok(Value::Text(s.clone())),
            _ => Err(api_error(
                StatusCode::BAD_REQUEST,
                format!("param {}: unsupported type (object/array)", i + 1),
            )),
        })
        .collect()
}

// ── SQLite value → JSON ──────────────────────────────────────────

fn sqlite_value_to_json(row: &rusqlite::Row<'_>, idx: usize) -> JsonValue {
    use rusqlite::types::ValueRef;

    match row.get_ref(idx) {
        Ok(ValueRef::Null) => JsonValue::Null,
        Ok(ValueRef::Integer(i)) => JsonValue::Number(i.into()),
        Ok(ValueRef::Real(f)) => serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        Ok(ValueRef::Text(bytes)) => {
            let s = String::from_utf8_lossy(bytes);
            JsonValue::String(s.into_owned())
        }
        Ok(ValueRef::Blob(_)) => JsonValue::Null,
        Err(_) => JsonValue::Null,
    }
}
