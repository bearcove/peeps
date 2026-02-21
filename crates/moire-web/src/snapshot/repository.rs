use std::collections::BTreeMap;

use facet::Facet;
use moire_trace_types::RelPc;
use moire_types::ConnectionId;
use rusqlite_facet::StatementFacetExt;

use crate::db::Db;

#[derive(Facet, Clone)]
pub(crate) struct StoredBacktraceFrameRow {
    pub(crate) frame_index: u32,
    pub(crate) module_path: String,
    pub(crate) module_identity: String,
    pub(crate) rel_pc: RelPc,
}

#[derive(Facet, Clone)]
pub(crate) struct SymbolicatedFrameRow {
    pub(crate) frame_index: u32,
    pub(crate) module_path: String,
    pub(crate) rel_pc: RelPc,
    pub(crate) status: String,
    pub(crate) function_name: Option<String>,
    pub(crate) source_file_path: Option<String>,
    pub(crate) source_line: Option<i64>,
    pub(crate) unresolved_reason: Option<String>,
}

pub(crate) struct BacktraceFrameBatch {
    pub(crate) backtrace_id: u64,
    pub(crate) raw_rows: Vec<StoredBacktraceFrameRow>,
    pub(crate) symbolicated_by_index: BTreeMap<u32, SymbolicatedFrameRow>,
}

#[derive(Facet)]
struct BacktraceFrameParams {
    conn_id: ConnectionId,
    backtrace_id: u64,
}

pub(crate) fn load_backtrace_frame_batches(
    db: &Db,
    pairs: &[(ConnectionId, u64)],
) -> Result<Vec<BacktraceFrameBatch>, String> {
    let conn = db.open()?;

    let mut backtrace_owner: BTreeMap<u64, ConnectionId> = BTreeMap::new();
    for (conn_id, backtrace_id) in pairs {
        match backtrace_owner.insert(*backtrace_id, *conn_id) {
            None => {}
            Some(existing_conn_id) if existing_conn_id == *conn_id => {}
            Some(existing_conn_id) => {
                return Err(format!(
                    "invariant violated: backtrace_id {backtrace_id} appears on multiple connections ({existing_conn_id}, {conn_id})"
                ));
            }
        }
    }

    let mut raw_stmt = conn
        .prepare(
            "SELECT frame_index, module_path, module_identity, rel_pc
             FROM backtrace_frames
             WHERE conn_id = :conn_id AND backtrace_id = :backtrace_id
             ORDER BY frame_index ASC",
        )
        .map_err(|error| format!("prepare backtrace_frames read: {error}"))?;
    let mut symbol_stmt = conn
        .prepare(
            "SELECT frame_index, module_path, rel_pc, status, function_name, source_file_path, source_line, unresolved_reason
             FROM symbolicated_frames
             WHERE conn_id = :conn_id AND backtrace_id = :backtrace_id",
        )
        .map_err(|error| format!("prepare symbolicated_frames read: {error}"))?;

    let mut batches = Vec::with_capacity(backtrace_owner.len());
    for (backtrace_id, conn_id) in backtrace_owner {
        let params = BacktraceFrameParams {
            conn_id,
            backtrace_id,
        };
        let raw_rows = raw_stmt
            .facet_query_ref::<StoredBacktraceFrameRow, _>(&params)
            .map_err(|error| format!("query backtrace_frames: {error}"))?;
        if raw_rows.is_empty() {
            return Err(format!(
                "invariant violated: referenced backtrace {backtrace_id} missing in storage"
            ));
        }

        let symbolicated_by_index = symbol_stmt
            .facet_query_ref::<SymbolicatedFrameRow, _>(&params)
            .map_err(|error| format!("query symbolicated_frames: {error}"))?
            .into_iter()
            .map(|row| (row.frame_index, row))
            .collect();

        batches.push(BacktraceFrameBatch {
            backtrace_id,
            raw_rows,
            symbolicated_by_index,
        });
    }

    Ok(batches)
}
