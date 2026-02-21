use facet::Facet;
use rusqlite::Connection;
use rusqlite_facet::ConnectionFacetExt;

use crate::db::Db;

const DB_SCHEMA_VERSION: i64 = 5;

#[derive(Facet)]
struct NoParams;

#[derive(Facet)]
struct UserVersionRow {
    user_version: i64,
}

#[derive(Facet)]
struct MaxConnIdRow {
    max_conn_id: i64,
}

pub fn init_sqlite(db: &Db) -> Result<(), String> {
    let conn = db.open()?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
        .map_err(|error| format!("init sqlite pragmas: {error}"))?;

    let user_version = conn
        .facet_query_one_ref::<UserVersionRow, _>(
            "SELECT user_version AS user_version FROM pragma_user_version",
            &NoParams,
        )
        .map_err(|error| format!("read sqlite user_version: {error}"))?
        .user_version;

    if user_version > DB_SCHEMA_VERSION {
        return Err(format!(
            "database schema version {} is newer than supported {}",
            user_version, DB_SCHEMA_VERSION
        ));
    }

    if user_version < DB_SCHEMA_VERSION {
        reset_managed_schema(&conn)?;
        conn.pragma_update(None, "user_version", DB_SCHEMA_VERSION)
            .map_err(|error| format!("set sqlite user_version: {error}"))?;
    }

    conn.execute_batch(managed_schema_sql())
        .map_err(|error| format!("ensure schema: {error}"))?;
    Ok(())
}

pub fn load_next_connection_id(db: &Db) -> Result<u64, String> {
    let conn = db.open()?;
    let max_conn_id = conn
        .facet_query_one_ref::<MaxConnIdRow, _>(
            "SELECT COALESCE(MAX(conn_id), 0) AS max_conn_id FROM connections",
            &NoParams,
        )
        .map_err(|error| format!("read max conn_id: {error}"))?
        .max_conn_id;
    if max_conn_id < 0 {
        return Err(format!(
            "invariant violated: negative conn_id in storage ({max_conn_id})"
        ));
    }
    let max_conn_id = u64::try_from(max_conn_id)
        .map_err(|error| format!("convert max conn_id to u64: {error}"))?;
    max_conn_id
        .checked_add(1)
        .ok_or_else(|| String::from("invariant violated: conn_id overflow"))
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
        DROP TABLE IF EXISTS top_application_frames;
        DROP TABLE IF EXISTS symbolicated_frames;
        DROP TABLE IF EXISTS symbolication_cache;
        DROP TABLE IF EXISTS backtrace_frames;
        DROP TABLE IF EXISTS backtraces;
        DROP TABLE IF EXISTS connection_modules;
        DROP TABLE IF EXISTS connections;
        ",
    )
    .map_err(|error| format!("reset schema: {error}"))
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

    CREATE TABLE IF NOT EXISTS connection_modules (
        conn_id INTEGER NOT NULL,
        module_id INTEGER NOT NULL,
        module_index INTEGER NOT NULL,
        module_path TEXT NOT NULL,
        module_identity TEXT NOT NULL,
        arch TEXT NOT NULL,
        runtime_base INTEGER NOT NULL,
        PRIMARY KEY (conn_id, module_index),
        UNIQUE (conn_id, module_id)
    );

    CREATE TABLE IF NOT EXISTS backtraces (
        conn_id INTEGER NOT NULL,
        backtrace_id INTEGER NOT NULL,
        frame_count INTEGER NOT NULL,
        received_at_ns INTEGER NOT NULL,
        PRIMARY KEY (conn_id, backtrace_id)
    );

    CREATE TABLE IF NOT EXISTS backtrace_frames (
        conn_id INTEGER NOT NULL,
        backtrace_id INTEGER NOT NULL,
        frame_index INTEGER NOT NULL,
        module_path TEXT NOT NULL,
        module_identity TEXT NOT NULL,
        rel_pc INTEGER NOT NULL,
        PRIMARY KEY (conn_id, backtrace_id, frame_index)
    );
    CREATE INDEX IF NOT EXISTS idx_backtrace_frames_identity_pc
        ON backtrace_frames (module_identity, rel_pc);

    CREATE TABLE IF NOT EXISTS symbolication_cache (
        module_identity TEXT NOT NULL,
        rel_pc INTEGER NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('resolved', 'unresolved')),
        function_name TEXT,
        crate_name TEXT,
        crate_module_path TEXT,
        source_file_path TEXT,
        source_line INTEGER,
        source_col INTEGER,
        unresolved_reason TEXT,
        updated_at_ns INTEGER NOT NULL,
        PRIMARY KEY (module_identity, rel_pc)
    );

    CREATE TABLE IF NOT EXISTS symbolicated_frames (
        conn_id INTEGER NOT NULL,
        backtrace_id INTEGER NOT NULL,
        frame_index INTEGER NOT NULL,
        module_path TEXT NOT NULL,
        rel_pc INTEGER NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('resolved', 'unresolved')),
        function_name TEXT,
        crate_name TEXT,
        crate_module_path TEXT,
        source_file_path TEXT,
        source_line INTEGER,
        source_col INTEGER,
        unresolved_reason TEXT,
        updated_at_ns INTEGER NOT NULL,
        PRIMARY KEY (conn_id, backtrace_id, frame_index)
    );
    CREATE INDEX IF NOT EXISTS idx_symbolicated_frames_backtrace
        ON symbolicated_frames (conn_id, backtrace_id, frame_index);

    CREATE TABLE IF NOT EXISTS top_application_frames (
        conn_id INTEGER NOT NULL,
        backtrace_id INTEGER NOT NULL,
        frame_index INTEGER NOT NULL,
        function_name TEXT,
        crate_name TEXT NOT NULL,
        crate_module_path TEXT,
        source_file_path TEXT,
        source_line INTEGER,
        source_col INTEGER,
        updated_at_ns INTEGER NOT NULL,
        PRIMARY KEY (conn_id, backtrace_id)
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
