use std::sync::Arc;

use facet::Facet;
use moire_trace_types::{BacktraceId, ModuleId, RelPc, RuntimeBase};
use moire_types::ConnectionId;
use moire_wire::{BacktraceRecord, ModuleIdentity, ModuleManifestEntry};
use rusqlite_facet::{ConnectionFacetExt, StatementFacetExt};

use crate::db::Db;
use crate::util::time::now_nanos;

#[derive(Clone)]
pub struct BacktraceFramePersist {
    pub frame_index: u32,
    pub rel_pc: RelPc,
    pub module_path: String,
    pub module_identity: String,
}

#[derive(Clone)]
pub struct StoredModuleManifestEntry {
    pub module_id: ModuleId,
    pub module_path: String,
    pub module_identity: String,
    pub arch: String,
    pub runtime_base: RuntimeBase,
}

#[derive(Facet)]
struct ConnectionUpsertParams {
    conn_id: ConnectionId,
    process_name: String,
    pid: u32,
    connected_at_ns: i64,
}

#[derive(Facet)]
struct ConnectionClosedParams {
    conn_id: ConnectionId,
    disconnected_at_ns: i64,
}

#[derive(Facet)]
struct ConnectionIdParams {
    conn_id: ConnectionId,
}

#[derive(Facet)]
struct ConnectionModuleInsertParams {
    conn_id: ConnectionId,
    module_id: ModuleId,
    module_index: i64,
    module_path: String,
    module_identity: String,
    arch: String,
    runtime_base: RuntimeBase,
}

#[derive(Facet)]
struct BacktraceInsertParams {
    conn_id: ConnectionId,
    backtrace_id: BacktraceId,
    frame_count: i64,
    received_at_ns: i64,
}

#[derive(Facet)]
struct BacktraceFrameInsertParams {
    conn_id: ConnectionId,
    backtrace_id: BacktraceId,
    frame_index: u32,
    module_path: String,
    module_identity: String,
    rel_pc: RelPc,
}

#[derive(Facet)]
struct CutRequestParams {
    cut_id: String,
    requested_at_ns: i64,
}

#[derive(Facet)]
struct CutAckParams {
    cut_id: String,
    conn_id: ConnectionId,
    stream_id: String,
    next_seq_no: u64,
    received_at_ns: i64,
}

#[derive(Facet)]
struct DeltaBatchInsertParams {
    conn_id: ConnectionId,
    stream_id: String,
    from_seq_no: u64,
    next_seq_no: u64,
    truncated: i64,
    compacted_before_seq_no: Option<u64>,
    change_count: u64,
    payload_json: String,
    received_at_ns: i64,
}

#[derive(Facet)]
struct UpsertEntityParams {
    conn_id: ConnectionId,
    stream_id: String,
    entity_id: String,
    entity_json: String,
    updated_at_ns: i64,
}

#[derive(Facet)]
struct UpsertScopeParams {
    conn_id: ConnectionId,
    stream_id: String,
    scope_id: String,
    scope_json: String,
    updated_at_ns: i64,
}

#[derive(Facet)]
struct UpsertEntityScopeLinkParams {
    conn_id: ConnectionId,
    stream_id: String,
    entity_id: String,
    scope_id: String,
    updated_at_ns: i64,
}

#[derive(Facet)]
struct RemoveEntityParams {
    conn_id: ConnectionId,
    stream_id: String,
    entity_id: String,
}

#[derive(Facet)]
struct RemoveScopeParams {
    conn_id: ConnectionId,
    stream_id: String,
    scope_id: String,
}

#[derive(Facet)]
struct RemoveEntityScopeLinkParams {
    conn_id: ConnectionId,
    stream_id: String,
    entity_id: String,
    scope_id: String,
}

#[derive(Facet)]
struct UpsertEdgeParams {
    conn_id: ConnectionId,
    stream_id: String,
    src_id: String,
    dst_id: String,
    kind_json: String,
    edge_json: String,
    updated_at_ns: i64,
}

#[derive(Facet)]
struct RemoveEdgeParams {
    conn_id: ConnectionId,
    stream_id: String,
    src_id: String,
    dst_id: String,
    kind_json: String,
}

#[derive(Facet)]
struct AppendEventParams {
    conn_id: ConnectionId,
    stream_id: String,
    seq_no: u64,
    event_id: String,
    event_json: String,
    at_ms: u64,
}

#[derive(Facet)]
struct StreamCursorUpsertParams {
    conn_id: ConnectionId,
    stream_id: String,
    next_seq_no: u64,
    updated_at_ns: i64,
}

pub fn backtrace_frames_for_store(
    module_manifest: &[StoredModuleManifestEntry],
    record: &BacktraceRecord,
) -> Result<Vec<BacktraceFramePersist>, String> {
    let modules_by_id = module_manifest
        .iter()
        .map(|module| (module.module_id, module))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut frames = Vec::with_capacity(record.frames.len());
    for (frame_index, frame) in record.frames.iter().enumerate() {
        let module_id = frame.module_id;
        let Some(module) = modules_by_id.get(&module_id) else {
            return Err(format!(
                "invariant violated: backtrace frame {frame_index} references module_id {} but handshake manifest for this connection has no matching module id ({} entries)",
                module_id,
                modules_by_id.len()
            ));
        };
        frames.push(BacktraceFramePersist {
            frame_index: frame_index as u32,
            rel_pc: frame.rel_pc,
            module_path: module.module_path.clone(),
            module_identity: module.module_identity.clone(),
        });
    }
    Ok(frames)
}

pub fn into_stored_module_manifest(
    module_manifest: Vec<ModuleManifestEntry>,
) -> Vec<StoredModuleManifestEntry> {
    module_manifest
        .into_iter()
        .map(|module| StoredModuleManifestEntry {
            module_id: module.module_id,
            module_path: module.module_path,
            module_identity: module_identity_key(&module.identity),
            arch: module.arch,
            runtime_base: module.runtime_base,
        })
        .collect()
}

fn module_identity_key(identity: &ModuleIdentity) -> String {
    match identity {
        ModuleIdentity::BuildId(build_id) => format!("build_id:{build_id}"),
        ModuleIdentity::DebugId(debug_id) => format!("debug_id:{debug_id}"),
    }
}

pub async fn persist_connection_upsert(
    db: Arc<Db>,
    conn_id: ConnectionId,
    process_name: String,
    pid: u32,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let conn = db.open()?;
        conn.facet_execute_ref(
            "INSERT INTO connections (conn_id, process_name, pid, connected_at_ns, disconnected_at_ns)
             VALUES (:conn_id, :process_name, :pid, :connected_at_ns, NULL)
             ON CONFLICT(conn_id) DO UPDATE SET
               process_name = excluded.process_name,
               pid = excluded.pid",
            &ConnectionUpsertParams {
                conn_id,
                process_name,
                pid,
                connected_at_ns: now_nanos(),
            },
        )
        .map_err(|error| format!("upsert connection: {error}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("join sqlite: {error}"))?
}

pub async fn persist_connection_closed(db: Arc<Db>, conn_id: ConnectionId) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let conn = db.open()?;
        conn.facet_execute_ref(
            "UPDATE connections
             SET disconnected_at_ns = :disconnected_at_ns
             WHERE conn_id = :conn_id",
            &ConnectionClosedParams {
                conn_id,
                disconnected_at_ns: now_nanos(),
            },
        )
        .map_err(|error| format!("close connection: {error}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("join sqlite: {error}"))?
}

pub async fn persist_connection_module_manifest(
    db: Arc<Db>,
    conn_id: ConnectionId,
    module_manifest: Vec<StoredModuleManifestEntry>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut conn = db.open()?;
        let tx = conn
            .transaction()
            .map_err(|error| format!("start transaction: {error}"))?;
        {
            let mut delete_stmt = tx
                .prepare("DELETE FROM connection_modules WHERE conn_id = :conn_id")
                .map_err(|error| format!("prepare delete connection_modules: {error}"))?;
            delete_stmt
                .facet_execute_ref(&ConnectionIdParams { conn_id })
                .map_err(|error| format!("delete connection_modules: {error}"))?;
        }

        {
            let mut insert_stmt = tx
                .prepare(
                    "INSERT INTO connection_modules (
                        conn_id, module_id, module_index, module_path, module_identity, arch, runtime_base
                     ) VALUES (
                        :conn_id, :module_id, :module_index, :module_path, :module_identity, :arch, :runtime_base
                     )",
                )
                .map_err(|error| format!("prepare insert connection_modules: {error}"))?;
            for (module_index, module) in module_manifest.iter().enumerate() {
                insert_stmt
                    .facet_execute_ref(&ConnectionModuleInsertParams {
                        conn_id,
                        module_id: module.module_id,
                        module_index: module_index as i64,
                        module_path: module.module_path.clone(),
                        module_identity: module.module_identity.clone(),
                        arch: module.arch.clone(),
                        runtime_base: module.runtime_base,
                    })
                    .map_err(|error| format!("insert connection_module[{module_index}]: {error}"))?;
            }
        }
        tx.commit()
            .map_err(|error| format!("commit connection_modules: {error}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("join sqlite: {error}"))?
}

// r[impl symbolicate.server-store]
pub async fn persist_backtrace_record(
    db: Arc<Db>,
    conn_id: ConnectionId,
    backtrace_id: BacktraceId,
    frames: Vec<BacktraceFramePersist>,
) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        let mut conn = db.open()?;
        let tx = conn
            .transaction()
            .map_err(|error| format!("start transaction: {error}"))?;
        let inserted = {
            let mut insert_backtrace_stmt = tx
                .prepare(
                    "INSERT INTO backtraces (conn_id, backtrace_id, frame_count, received_at_ns)
                     VALUES (:conn_id, :backtrace_id, :frame_count, :received_at_ns)
                     ON CONFLICT(conn_id, backtrace_id) DO NOTHING",
                )
                .map_err(|error| format!("prepare insert backtrace: {error}"))?;
            insert_backtrace_stmt
                .facet_execute_ref(&BacktraceInsertParams {
                    conn_id,
                    backtrace_id,
                    frame_count: frames.len() as i64,
                    received_at_ns: now_nanos(),
                })
                .map_err(|error| format!("insert backtrace: {error}"))?
                > 0
        };
        if inserted {
            {
                let mut insert_frame_stmt = tx
                    .prepare(
                        "INSERT INTO backtrace_frames (
                            conn_id, backtrace_id, frame_index, module_path, module_identity, rel_pc
                         ) VALUES (
                            :conn_id, :backtrace_id, :frame_index, :module_path, :module_identity, :rel_pc
                         )",
                    )
                    .map_err(|error| format!("prepare insert backtrace frames: {error}"))?;
                for frame in &frames {
                    insert_frame_stmt
                        .facet_execute_ref(&BacktraceFrameInsertParams {
                            conn_id,
                            backtrace_id,
                            frame_index: frame.frame_index,
                            module_path: frame.module_path.clone(),
                            module_identity: frame.module_identity.clone(),
                            rel_pc: frame.rel_pc,
                        })
                        .map_err(|error| {
                            format!(
                                "insert backtrace frame {}/{}: {error}",
                                frame.frame_index,
                                backtrace_id
                            )
                        })?;
                }
            }
        }
        tx.commit()
            .map_err(|error| format!("commit backtrace record: {error}"))?;
        Ok::<bool, String>(inserted)
    })
    .await
    .map_err(|error| format!("join sqlite: {error}"))?
}

pub async fn persist_cut_request(
    db: Arc<Db>,
    cut_id: String,
    requested_at_ns: i64,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let conn = db.open()?;
        conn.facet_execute_ref(
            "INSERT INTO cuts (cut_id, requested_at_ns) VALUES (?1, ?2)
             ON CONFLICT(cut_id) DO UPDATE SET requested_at_ns = excluded.requested_at_ns",
            &CutRequestParams {
                cut_id,
                requested_at_ns,
            },
        )
        .map_err(|error| format!("upsert cut: {error}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("join sqlite: {error}"))?
}

pub async fn persist_cut_ack(
    db: Arc<Db>,
    cut_id: String,
    conn_id: ConnectionId,
    stream_id: String,
    next_seq_no: u64,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let conn = db.open()?;
        conn.facet_execute_ref(
            "INSERT INTO cut_acks (cut_id, conn_id, stream_id, next_seq_no, received_at_ns)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(cut_id, conn_id) DO UPDATE SET
               stream_id = excluded.stream_id,
               next_seq_no = excluded.next_seq_no,
               received_at_ns = excluded.received_at_ns",
            &CutAckParams {
                cut_id,
                conn_id,
                stream_id,
                next_seq_no,
                received_at_ns: now_nanos(),
            },
        )
        .map_err(|error| format!("upsert cut ack: {error}"))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("join sqlite: {error}"))?
}

pub async fn persist_delta_batch(
    db: Arc<Db>,
    conn_id: ConnectionId,
    batch: moire_types::PullChangesResponse,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || persist_delta_batch_blocking(&db, conn_id, &batch))
        .await
        .map_err(|error| format!("join sqlite: {error}"))?
}

fn persist_delta_batch_blocking(
    db: &Db,
    conn_id: ConnectionId,
    batch: &moire_types::PullChangesResponse,
) -> Result<(), String> {
    use moire_types::Change;

    let mut conn = db.open()?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("start transaction: {error}"))?;
    let stream_id = batch.stream_id.0.as_str().to_string();
    let received_at_ns = now_nanos();
    let payload_json =
        facet_json::to_string(batch).map_err(|error| format!("encode batch: {error}"))?;

    {
        let mut insert_delta_batch_stmt = tx
            .prepare(
                "INSERT INTO delta_batches (
                conn_id, stream_id, from_seq_no, next_seq_no, truncated,
                compacted_before_seq_no, change_count, payload_json, received_at_ns
             ) VALUES (
                :conn_id, :stream_id, :from_seq_no, :next_seq_no, :truncated,
                :compacted_before_seq_no, :change_count, :payload_json, :received_at_ns
             )",
            )
            .map_err(|error| format!("prepare delta batch insert: {error}"))?;
        insert_delta_batch_stmt
            .facet_execute_ref(&DeltaBatchInsertParams {
                conn_id,
                stream_id: stream_id.clone(),
                from_seq_no: batch.from_seq_no.0,
                next_seq_no: batch.next_seq_no.0,
                truncated: if batch.truncated { 1 } else { 0 },
                compacted_before_seq_no: batch.compacted_before_seq_no.map(|seq_no| seq_no.0),
                change_count: batch.changes.len() as u64,
                payload_json,
                received_at_ns,
            })
            .map_err(|error| format!("insert delta batch: {error}"))?;

        let mut upsert_entity_stmt = tx
            .prepare(
                "INSERT INTO entities (conn_id, stream_id, entity_id, entity_json, updated_at_ns)
             VALUES (:conn_id, :stream_id, :entity_id, :entity_json, :updated_at_ns)
             ON CONFLICT(conn_id, stream_id, entity_id) DO UPDATE SET
               entity_json = excluded.entity_json,
               updated_at_ns = excluded.updated_at_ns",
            )
            .map_err(|error| format!("prepare entity upsert: {error}"))?;
        let mut upsert_scope_stmt = tx
            .prepare(
                "INSERT INTO scopes (conn_id, stream_id, scope_id, scope_json, updated_at_ns)
             VALUES (:conn_id, :stream_id, :scope_id, :scope_json, :updated_at_ns)
             ON CONFLICT(conn_id, stream_id, scope_id) DO UPDATE SET
               scope_json = excluded.scope_json,
               updated_at_ns = excluded.updated_at_ns",
            )
            .map_err(|error| format!("prepare scope upsert: {error}"))?;
        let mut upsert_entity_scope_link_stmt = tx
        .prepare(
            "INSERT INTO entity_scope_links (conn_id, stream_id, entity_id, scope_id, updated_at_ns)
             VALUES (:conn_id, :stream_id, :entity_id, :scope_id, :updated_at_ns)
             ON CONFLICT(conn_id, stream_id, entity_id, scope_id) DO UPDATE SET
               updated_at_ns = excluded.updated_at_ns",
        )
        .map_err(|error| format!("prepare entity_scope_link upsert: {error}"))?;
        let mut delete_entity_stmt = tx
            .prepare(
                "DELETE FROM entities
             WHERE conn_id = :conn_id AND stream_id = :stream_id AND entity_id = :entity_id",
            )
            .map_err(|error| format!("prepare delete entity: {error}"))?;
        let mut delete_entity_scope_links_for_entity_stmt = tx
            .prepare(
                "DELETE FROM entity_scope_links
             WHERE conn_id = :conn_id AND stream_id = :stream_id AND entity_id = :entity_id",
            )
            .map_err(|error| format!("prepare delete entity_scope_links for entity: {error}"))?;
        let mut delete_incident_edges_stmt = tx
            .prepare(
                "DELETE FROM edges
             WHERE conn_id = :conn_id AND stream_id = :stream_id
               AND (src_id = :entity_id OR dst_id = :entity_id)",
            )
            .map_err(|error| format!("prepare delete incident edges: {error}"))?;
        let mut delete_scope_stmt = tx
            .prepare(
                "DELETE FROM scopes
             WHERE conn_id = :conn_id AND stream_id = :stream_id AND scope_id = :scope_id",
            )
            .map_err(|error| format!("prepare delete scope: {error}"))?;
        let mut delete_entity_scope_links_for_scope_stmt = tx
            .prepare(
                "DELETE FROM entity_scope_links
             WHERE conn_id = :conn_id AND stream_id = :stream_id AND scope_id = :scope_id",
            )
            .map_err(|error| format!("prepare delete entity_scope_links for scope: {error}"))?;
        let mut delete_entity_scope_link_stmt = tx
            .prepare(
                "DELETE FROM entity_scope_links
             WHERE conn_id = :conn_id AND stream_id = :stream_id
               AND entity_id = :entity_id AND scope_id = :scope_id",
            )
            .map_err(|error| format!("prepare delete entity_scope_link: {error}"))?;
        let mut upsert_edge_stmt = tx
        .prepare(
            "INSERT INTO edges (conn_id, stream_id, src_id, dst_id, kind_json, edge_json, updated_at_ns)
             VALUES (:conn_id, :stream_id, :src_id, :dst_id, :kind_json, :edge_json, :updated_at_ns)
             ON CONFLICT(conn_id, stream_id, src_id, dst_id, kind_json) DO UPDATE SET
               edge_json = excluded.edge_json,
               updated_at_ns = excluded.updated_at_ns",
        )
        .map_err(|error| format!("prepare edge upsert: {error}"))?;
        let mut delete_edge_stmt = tx
            .prepare(
                "DELETE FROM edges
             WHERE conn_id = :conn_id AND stream_id = :stream_id
               AND src_id = :src_id AND dst_id = :dst_id AND kind_json = :kind_json",
            )
            .map_err(|error| format!("prepare delete edge: {error}"))?;
        let mut append_event_stmt = tx
        .prepare(
            "INSERT OR REPLACE INTO events (conn_id, stream_id, seq_no, event_id, event_json, at_ms)
             VALUES (:conn_id, :stream_id, :seq_no, :event_id, :event_json, :at_ms)",
        )
        .map_err(|error| format!("prepare append event: {error}"))?;

        for stamped in &batch.changes {
            match &stamped.change {
                Change::UpsertEntity(entity) => {
                    let entity_json = facet_json::to_string(entity)
                        .map_err(|error| format!("encode entity: {error}"))?;
                    upsert_entity_stmt
                        .facet_execute_ref(&UpsertEntityParams {
                            conn_id,
                            stream_id: batch.stream_id.0.as_str().to_string(),
                            entity_id: entity.id.as_str().to_string(),
                            entity_json,
                            updated_at_ns: received_at_ns,
                        })
                        .map_err(|error| format!("upsert entity: {error}"))?;
                }
                Change::UpsertScope(scope) => {
                    let scope_json = facet_json::to_string(scope)
                        .map_err(|error| format!("encode scope: {error}"))?;
                    upsert_scope_stmt
                        .facet_execute_ref(&UpsertScopeParams {
                            conn_id,
                            stream_id: batch.stream_id.0.as_str().to_string(),
                            scope_id: scope.id.as_str().to_string(),
                            scope_json,
                            updated_at_ns: received_at_ns,
                        })
                        .map_err(|error| format!("upsert scope: {error}"))?;
                }
                Change::UpsertEntityScopeLink {
                    entity_id,
                    scope_id,
                } => {
                    upsert_entity_scope_link_stmt
                        .facet_execute_ref(&UpsertEntityScopeLinkParams {
                            conn_id,
                            stream_id: batch.stream_id.0.as_str().to_string(),
                            entity_id: entity_id.as_str().to_string(),
                            scope_id: scope_id.as_str().to_string(),
                            updated_at_ns: received_at_ns,
                        })
                        .map_err(|error| format!("upsert entity_scope_link: {error}"))?;
                }
                Change::RemoveEntity { id } => {
                    let params = RemoveEntityParams {
                        conn_id,
                        stream_id: batch.stream_id.0.as_str().to_string(),
                        entity_id: id.as_str().to_string(),
                    };
                    delete_entity_stmt
                        .facet_execute_ref(&params)
                        .map_err(|error| format!("delete entity: {error}"))?;
                    delete_entity_scope_links_for_entity_stmt
                        .facet_execute_ref(&params)
                        .map_err(|error| {
                            format!("delete entity_scope_links for entity: {error}")
                        })?;
                    delete_incident_edges_stmt
                        .facet_execute_ref(&params)
                        .map_err(|error| format!("delete incident edges: {error}"))?;
                }
                Change::RemoveScope { id } => {
                    let params = RemoveScopeParams {
                        conn_id,
                        stream_id: batch.stream_id.0.as_str().to_string(),
                        scope_id: id.as_str().to_string(),
                    };
                    delete_scope_stmt
                        .facet_execute_ref(&params)
                        .map_err(|error| format!("delete scope: {error}"))?;
                    delete_entity_scope_links_for_scope_stmt
                        .facet_execute_ref(&params)
                        .map_err(|error| format!("delete entity_scope_links for scope: {error}"))?;
                }
                Change::RemoveEntityScopeLink {
                    entity_id,
                    scope_id,
                } => {
                    delete_entity_scope_link_stmt
                        .facet_execute_ref(&RemoveEntityScopeLinkParams {
                            conn_id,
                            stream_id: batch.stream_id.0.as_str().to_string(),
                            entity_id: entity_id.as_str().to_string(),
                            scope_id: scope_id.as_str().to_string(),
                        })
                        .map_err(|error| format!("delete entity_scope_link: {error}"))?;
                }
                Change::UpsertEdge(edge) => {
                    let kind_json = facet_json::to_string(&edge.kind)
                        .map_err(|error| format!("encode edge kind: {error}"))?;
                    let edge_json = facet_json::to_string(edge)
                        .map_err(|error| format!("encode edge: {error}"))?;
                    upsert_edge_stmt
                        .facet_execute_ref(&UpsertEdgeParams {
                            conn_id,
                            stream_id: batch.stream_id.0.as_str().to_string(),
                            src_id: edge.src.as_str().to_string(),
                            dst_id: edge.dst.as_str().to_string(),
                            kind_json,
                            edge_json,
                            updated_at_ns: received_at_ns,
                        })
                        .map_err(|error| format!("upsert edge: {error}"))?;
                }
                Change::RemoveEdge { src, dst, kind } => {
                    let kind_json = facet_json::to_string(kind)
                        .map_err(|error| format!("encode edge kind: {error}"))?;
                    delete_edge_stmt
                        .facet_execute_ref(&RemoveEdgeParams {
                            conn_id,
                            stream_id: batch.stream_id.0.as_str().to_string(),
                            src_id: src.as_str().to_string(),
                            dst_id: dst.as_str().to_string(),
                            kind_json,
                        })
                        .map_err(|error| format!("delete edge: {error}"))?;
                }
                Change::AppendEvent(event) => {
                    let event_json = facet_json::to_string(event)
                        .map_err(|error| format!("encode event: {error}"))?;
                    append_event_stmt
                        .facet_execute_ref(&AppendEventParams {
                            conn_id,
                            stream_id: batch.stream_id.0.as_str().to_string(),
                            seq_no: stamped.seq_no.0,
                            event_id: event.id.as_str().to_string(),
                            event_json,
                            at_ms: event.at.as_millis(),
                        })
                        .map_err(|error| format!("append event: {error}"))?;
                }
            }
        }

        let mut upsert_stream_cursor_stmt = tx
            .prepare(
                "INSERT INTO stream_cursors (conn_id, stream_id, next_seq_no, updated_at_ns)
             VALUES (:conn_id, :stream_id, :next_seq_no, :updated_at_ns)
             ON CONFLICT(conn_id, stream_id) DO UPDATE SET
               next_seq_no = excluded.next_seq_no,
               updated_at_ns = excluded.updated_at_ns",
            )
            .map_err(|error| format!("prepare stream cursor upsert: {error}"))?;
        upsert_stream_cursor_stmt
            .facet_execute_ref(&StreamCursorUpsertParams {
                conn_id,
                stream_id: batch.stream_id.0.as_str().to_string(),
                next_seq_no: batch.next_seq_no.0,
                updated_at_ns: received_at_ns,
            })
            .map_err(|error| format!("upsert stream cursor: {error}"))?;
    }

    tx.commit()
        .map_err(|error| format!("commit transaction: {error}"))?;
    Ok(())
}
