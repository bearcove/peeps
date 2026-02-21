use std::collections::{HashMap, hash_map::Entry};
use std::path::Path as FsPath;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use facet::Facet;
use moire_trace_types::RelPc;
use object::{Object, ObjectSegment};
use rusqlite::Transaction;
use rusqlite_facet::StatementFacetExt;
use tracing::{debug, info, warn};

use crate::db::Db;
use crate::util::time::now_nanos;

const SQLITE_BUSY_TIMEOUT_MS: u64 = 5_000;
const SYMBOLICATION_UNRESOLVED_EAGER_PREFIX: &str = "symbolication engine not wired:";
const TOP_FRAME_CRATE_EXCLUSIONS: &[&str] = &[
    "std",
    "core",
    "alloc",
    "tokio",
    "tokio_util",
    "futures",
    "futures_core",
    "futures_util",
    "moire",
    "moire_trace_capture",
];
static SYMBOLIZER_LOADER_ATTEMPT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Facet)]
struct SymbolicationCacheEntry {
    status: String,
    function_name: Option<String>,
    crate_name: Option<String>,
    crate_module_path: Option<String>,
    source_file_path: Option<String>,
    source_line: Option<i64>,
    source_col: Option<i64>,
    unresolved_reason: Option<String>,
}

#[derive(Facet, Clone)]
struct PendingFrameJob {
    conn_id: u64,
    backtrace_id: u64,
    frame_index: u32,
    module_path: String,
    module_identity: String,
    rel_pc: RelPc,
}

#[derive(Facet)]
struct PendingFrameLookupParams {
    conn_id: u64,
    backtrace_id: u64,
}

#[derive(Facet)]
struct CacheLookupParams<'a> {
    module_identity: &'a str,
    rel_pc: RelPc,
}

#[derive(Facet)]
struct UpsertSymbolicationCacheParams {
    module_identity: String,
    rel_pc: RelPc,
    status: String,
    function_name: Option<String>,
    crate_name: Option<String>,
    crate_module_path: Option<String>,
    source_file_path: Option<String>,
    source_line: Option<i64>,
    source_col: Option<i64>,
    unresolved_reason: Option<String>,
    updated_at_ns: i64,
}

#[derive(Facet)]
struct UpsertSymbolicatedFrameParams {
    conn_id: u64,
    backtrace_id: u64,
    frame_index: u32,
    module_path: String,
    rel_pc: RelPc,
    status: String,
    function_name: Option<String>,
    crate_name: Option<String>,
    crate_module_path: Option<String>,
    source_file_path: Option<String>,
    source_line: Option<i64>,
    source_col: Option<i64>,
    unresolved_reason: Option<String>,
    updated_at_ns: i64,
}

#[derive(Facet)]
struct TopFrameLookupParams {
    conn_id: u64,
    backtrace_id: u64,
    exclude_1: &'static str,
    exclude_2: &'static str,
    exclude_3: &'static str,
    exclude_4: &'static str,
    exclude_5: &'static str,
    exclude_6: &'static str,
    exclude_7: &'static str,
    exclude_8: &'static str,
    exclude_9: &'static str,
    exclude_10: &'static str,
}

#[derive(Facet)]
struct TopFrameCandidate {
    frame_index: i64,
    function_name: Option<String>,
    crate_name: String,
    crate_module_path: Option<String>,
    source_file_path: Option<String>,
    source_line: Option<i64>,
    source_col: Option<i64>,
}

#[derive(Facet)]
struct TopFrameUpsertParams {
    conn_id: u64,
    backtrace_id: u64,
    frame_index: i64,
    function_name: Option<String>,
    crate_name: String,
    crate_module_path: Option<String>,
    source_file_path: Option<String>,
    source_line: Option<i64>,
    source_col: Option<i64>,
    updated_at_ns: i64,
}

enum ModuleSymbolizerState {
    Ready {
        loader: Box<addr2line::Loader>,
        linked_image_base: u64,
    },
    Failed(String),
}

pub async fn symbolicate_pending_frames_for_pairs(
    db: Arc<Db>,
    pairs: &[(u64, u64)],
) -> Result<usize, String> {
    if pairs.is_empty() {
        return Ok(0);
    }
    let pairs = pairs.to_vec();
    tokio::task::spawn_blocking(move || symbolicate_pending_frames_for_pairs_blocking(&db, &pairs))
        .await
        .map_err(|error| format!("join symbolication worker: {error}"))?
}

fn symbolicate_pending_frames_for_pairs_blocking(
    db: &Db,
    pairs: &[(u64, u64)],
) -> Result<usize, String> {
    let started = Instant::now();
    let mut conn = db.open()?;
    conn.busy_timeout(Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS))
        .map_err(|error| format!("set sqlite busy_timeout: {error}"))?;
    let tx = conn
        .transaction()
        .map_err(|error| format!("start transaction: {error}"))?;

    let mut pending_stmt = tx
        .prepare(
            "SELECT bf.conn_id, bf.backtrace_id, bf.frame_index, bf.module_path, bf.module_identity, bf.rel_pc
             FROM backtrace_frames bf
             LEFT JOIN symbolicated_frames sf
                ON sf.conn_id = bf.conn_id
               AND sf.backtrace_id = bf.backtrace_id
               AND sf.frame_index = bf.frame_index
             WHERE bf.conn_id = :conn_id
               AND bf.backtrace_id = :backtrace_id
               AND (
                    sf.conn_id IS NULL
                    OR (
                        sf.status = 'unresolved'
                        AND sf.unresolved_reason LIKE 'symbolication engine not wired:%'
                    )
               )
             ORDER BY bf.frame_index ASC",
        )
        .map_err(|error| format!("prepare pending frame query: {error}"))?;

    let mut module_cache: HashMap<String, ModuleSymbolizerState> = HashMap::new();
    let mut processed = 0usize;
    let mut unknown_module_jobs = 0usize;
    for (conn_id, backtrace_id) in pairs {
        let params = PendingFrameLookupParams {
            conn_id: *conn_id,
            backtrace_id: *backtrace_id,
        };
        let jobs = pending_stmt
            .facet_query_ref::<PendingFrameJob, _>(&params)
            .map_err(|error| format!("query pending frame rows: {error}"))?;

        for job in &jobs {
            if job.module_path.starts_with("<unknown-module-id:") {
                unknown_module_jobs = unknown_module_jobs.saturating_add(1);
            }
            let cache = if job.module_identity != "unknown" {
                if let Some(hit) =
                    lookup_symbolication_cache(&tx, job.module_identity.as_str(), job.rel_pc)?
                {
                    if hit.status == "unresolved"
                        && hit
                            .unresolved_reason
                            .as_deref()
                            .is_some_and(should_retry_unresolved_reason)
                    {
                        debug!(
                            conn_id = job.conn_id,
                            backtrace_id = job.backtrace_id,
                            frame_index = job.frame_index,
                            "retrying previously scaffolded unresolved cache entry"
                        );
                        let resolved = resolve_frame_symbolication(job, &mut module_cache);
                        upsert_symbolication_cache(
                            &tx,
                            job.module_identity.as_str(),
                            job.rel_pc,
                            &resolved,
                        )?;
                        resolved
                    } else {
                        hit
                    }
                } else {
                    let resolved = resolve_frame_symbolication(job, &mut module_cache);
                    upsert_symbolication_cache(
                        &tx,
                        job.module_identity.as_str(),
                        job.rel_pc,
                        &resolved,
                    )?;
                    resolved
                }
            } else {
                resolve_frame_symbolication(job, &mut module_cache)
            };
            upsert_symbolicated_frame(
                &tx,
                job.conn_id,
                job.backtrace_id,
                job.frame_index,
                job.module_path.as_str(),
                job.rel_pc,
                &cache,
            )?;
            processed += 1;
        }
        if !jobs.is_empty() {
            update_top_application_frame(&tx, *conn_id, *backtrace_id)?;
        }
    }

    drop(pending_stmt);
    tx.commit()
        .map_err(|error| format!("commit symbolication pass: {error}"))?;
    if processed > 0 {
        info!(
            processed_frames = processed,
            unknown_module_jobs,
            module_cache_entries = module_cache.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "symbolication pass completed"
        );
        if module_cache.is_empty() && unknown_module_jobs > 0 {
            warn!(
                unknown_module_jobs,
                "symbolication did not open any modules because jobs referenced unknown module ids"
            );
        }
    }
    Ok(processed)
}

fn should_retry_unresolved_reason(reason: &str) -> bool {
    reason.starts_with(SYMBOLICATION_UNRESOLVED_EAGER_PREFIX)
        || reason.starts_with("no source location in debug info for '")
        || reason.starts_with("lookup frames for '")
        || reason.starts_with("iterate frames for '")
}

fn resolve_frame_symbolication(
    job: &PendingFrameJob,
    module_cache: &mut HashMap<String, ModuleSymbolizerState>,
) -> SymbolicationCacheEntry {
    let unresolved = |reason: String| SymbolicationCacheEntry {
        status: String::from("unresolved"),
        function_name: None,
        crate_name: None,
        crate_module_path: None,
        source_file_path: None,
        source_line: None,
        source_col: None,
        unresolved_reason: Some(reason),
    };
    if job.module_path.starts_with("<unknown-module-id:") {
        return unresolved(format!(
            "module id not found in module manifest: {}",
            job.module_path
        ));
    }

    let state = match module_cache.entry(job.module_path.clone()) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let _loader_attempt_id = SYMBOLIZER_LOADER_ATTEMPT_ID.fetch_add(1, Ordering::Relaxed);
            let loaded = match addr2line::Loader::new(job.module_path.as_str()) {
                Ok(loader) => {
                    let linked_image_base =
                        match linked_image_base_for_file(FsPath::new(job.module_path.as_str())) {
                            Ok(base) => base,
                            Err(error) => return unresolved(error),
                        };
                    ModuleSymbolizerState::Ready {
                        loader: Box::new(loader),
                        linked_image_base,
                    }
                }
                Err(error) => ModuleSymbolizerState::Failed(format!(
                    "open debug object '{}': {error}",
                    job.module_path
                )),
            };
            entry.insert(loaded)
        }
    };

    let ModuleSymbolizerState::Ready {
        loader,
        linked_image_base,
    } = state
    else {
        let ModuleSymbolizerState::Failed(reason) = state else {
            unreachable!()
        };
        return unresolved(reason.clone());
    };

    // r[impl symbolicate.addr-space]
    let lookup_pc = match linked_image_base.checked_add(job.rel_pc.get()) {
        Some(pc) => pc,
        None => {
            return unresolved(format!(
                "address overflow combining linked image base 0x{:x} with rel_pc 0x{:x} for '{}'",
                linked_image_base,
                job.rel_pc.get(),
                job.module_path
            ));
        }
    };
    let mut function_name = None::<String>;
    let mut source_file = None::<String>;
    let mut source_line = None::<i64>;
    let mut source_col = None::<i64>;

    let mut frames = match loader.find_frames(lookup_pc) {
        Ok(frames) => frames,
        Err(error) => {
            return unresolved(format!(
                "lookup frames for '{}' +0x{:x}: {error}",
                job.module_path,
                job.rel_pc.get()
            ));
        }
    };

    loop {
        match frames.next() {
            Ok(Some(frame)) => {
                if function_name.is_none()
                    && let Some(function) = frame.function
                {
                    match function.demangle() {
                        Ok(name) => {
                            function_name = Some(strip_rust_hash_suffix(name.as_ref()).to_owned())
                        }
                        Err(_) => match function.raw_name() {
                            Ok(name) => {
                                function_name =
                                    Some(strip_rust_hash_suffix(name.as_ref()).to_owned())
                            }
                            Err(error) => {
                                return unresolved(format!(
                                    "decode function name for '{}' +0x{:x}: {error}",
                                    job.module_path,
                                    job.rel_pc.get()
                                ));
                            }
                        },
                    }
                }
                if source_file.is_none()
                    && let Some(location) = frame.location
                    && let Some(path) = location.file
                {
                    source_file = Some(path.to_string());
                    source_line = location.line.map(i64::from);
                    source_col = location.column.map(i64::from);
                }
                if function_name.is_some() && source_file.is_some() {
                    break;
                }
            }
            Ok(None) => break,
            Err(error) => {
                return unresolved(format!(
                    "iterate frames for '{}' +0x{:x}: {error}",
                    job.module_path,
                    job.rel_pc.get()
                ));
            }
        }
    }
    if function_name.is_none()
        && let Some(symbol) = loader.find_symbol(lookup_pc)
    {
        function_name = Some(strip_rust_hash_suffix(symbol).to_owned());
    }

    let Some(source_file_path) = source_file else {
        return unresolved(format!(
            "no source location in debug info for '{}' +0x{:x}",
            job.module_path,
            job.rel_pc.get()
        ));
    };
    let function_name = function_name.unwrap_or_else(|| {
        format!(
            "{}+0x{:x}",
            job.module_path
                .rsplit('/')
                .next()
                .unwrap_or(job.module_path.as_str()),
            job.rel_pc.get()
        )
    });
    let crate_name = function_name
        .split("::")
        .next()
        .map(|name| name.to_string())
        .filter(|name| !name.trim().is_empty());
    let crate_module_path = if function_name.contains("::") {
        Some(function_name.clone())
    } else {
        None
    };
    SymbolicationCacheEntry {
        status: String::from("resolved"),
        function_name: Some(function_name),
        crate_name,
        crate_module_path,
        source_file_path: Some(source_file_path),
        source_line,
        source_col,
        unresolved_reason: None,
    }
}

fn strip_rust_hash_suffix(name: &str) -> &str {
    if let Some(index) = name.rfind("::h") {
        let suffix = &name[index + 3..];
        if !suffix.is_empty()
            && suffix
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return &name[..index];
        }
    }
    name
}

fn linked_image_base_for_file(path: &FsPath) -> Result<u64, String> {
    let data = std::fs::read(path)
        .map_err(|error| format!("read debug object '{}': {error}", path.display()))?;
    let object = object::File::parse(&*data)
        .map_err(|error| format!("parse debug object '{}': {error}", path.display()))?;
    object
        .segments()
        .filter_map(|segment| {
            let (_, file_size) = segment.file_range();
            if file_size == 0 {
                return None;
            }
            Some(segment.address())
        })
        .min()
        .ok_or_else(|| {
            format!(
                "no file-backed segments in debug object '{}'",
                path.display()
            )
        })
}

fn lookup_symbolication_cache(
    tx: &Transaction<'_>,
    module_identity: &str,
    rel_pc: RelPc,
) -> Result<Option<SymbolicationCacheEntry>, String> {
    let mut stmt = tx
        .prepare(
            "SELECT status, function_name, crate_name, crate_module_path,
                    source_file_path, source_line, source_col, unresolved_reason
             FROM symbolication_cache
             WHERE module_identity = :module_identity AND rel_pc = :rel_pc",
        )
        .map_err(|error| format!("prepare symbolication_cache lookup: {error}"))?;
    stmt.facet_query_optional_ref::<SymbolicationCacheEntry, _>(&CacheLookupParams {
        module_identity,
        rel_pc,
    })
    .map_err(|error| format!("query symbolication_cache: {error}"))
}

fn upsert_symbolication_cache(
    tx: &Transaction<'_>,
    module_identity: &str,
    rel_pc: RelPc,
    cache: &SymbolicationCacheEntry,
) -> Result<(), String> {
    let mut stmt = tx
        .prepare(
            "INSERT INTO symbolication_cache (
            module_identity, rel_pc, status, function_name, crate_name, crate_module_path,
            source_file_path, source_line, source_col, unresolved_reason, updated_at_ns
         ) VALUES (
            :module_identity, :rel_pc, :status, :function_name, :crate_name, :crate_module_path,
            :source_file_path, :source_line, :source_col, :unresolved_reason, :updated_at_ns
         )
         ON CONFLICT(module_identity, rel_pc) DO UPDATE SET
            status = excluded.status,
            function_name = excluded.function_name,
            crate_name = excluded.crate_name,
            crate_module_path = excluded.crate_module_path,
            source_file_path = excluded.source_file_path,
            source_line = excluded.source_line,
            source_col = excluded.source_col,
            unresolved_reason = excluded.unresolved_reason,
            updated_at_ns = excluded.updated_at_ns",
        )
        .map_err(|error| format!("prepare symbolication_cache upsert: {error}"))?;
    stmt.facet_execute_ref(&UpsertSymbolicationCacheParams {
        module_identity: module_identity.to_string(),
        rel_pc,
        status: cache.status.clone(),
        function_name: cache.function_name.clone(),
        crate_name: cache.crate_name.clone(),
        crate_module_path: cache.crate_module_path.clone(),
        source_file_path: cache.source_file_path.clone(),
        source_line: cache.source_line,
        source_col: cache.source_col,
        unresolved_reason: cache.unresolved_reason.clone(),
        updated_at_ns: now_nanos(),
    })
    .map_err(|error| format!("upsert symbolication_cache: {error}"))?;
    Ok(())
}

fn upsert_symbolicated_frame(
    tx: &Transaction<'_>,
    conn_id: u64,
    backtrace_id: u64,
    frame_index: u32,
    module_path: &str,
    rel_pc: RelPc,
    cache: &SymbolicationCacheEntry,
) -> Result<(), String> {
    let mut stmt = tx
        .prepare(
            "INSERT INTO symbolicated_frames (
            conn_id, backtrace_id, frame_index, module_path, rel_pc, status, function_name,
            crate_name, crate_module_path, source_file_path, source_line, source_col,
            unresolved_reason, updated_at_ns
         ) VALUES (
            :conn_id, :backtrace_id, :frame_index, :module_path, :rel_pc, :status, :function_name,
            :crate_name, :crate_module_path, :source_file_path, :source_line, :source_col,
            :unresolved_reason, :updated_at_ns
         )
         ON CONFLICT(conn_id, backtrace_id, frame_index) DO UPDATE SET
            module_path = excluded.module_path,
            rel_pc = excluded.rel_pc,
            status = excluded.status,
            function_name = excluded.function_name,
            crate_name = excluded.crate_name,
            crate_module_path = excluded.crate_module_path,
            source_file_path = excluded.source_file_path,
            source_line = excluded.source_line,
            source_col = excluded.source_col,
            unresolved_reason = excluded.unresolved_reason,
            updated_at_ns = excluded.updated_at_ns",
        )
        .map_err(|error| format!("prepare symbolicated_frames upsert: {error}"))?;
    stmt.facet_execute_ref(&UpsertSymbolicatedFrameParams {
        conn_id,
        backtrace_id,
        frame_index,
        module_path: module_path.to_string(),
        rel_pc,
        status: cache.status.clone(),
        function_name: cache.function_name.clone(),
        crate_name: cache.crate_name.clone(),
        crate_module_path: cache.crate_module_path.clone(),
        source_file_path: cache.source_file_path.clone(),
        source_line: cache.source_line,
        source_col: cache.source_col,
        unresolved_reason: cache.unresolved_reason.clone(),
        updated_at_ns: now_nanos(),
    })
    .map_err(|error| format!("upsert symbolicated_frame[{frame_index}]: {error}"))?;
    Ok(())
}

fn update_top_application_frame(
    tx: &Transaction<'_>,
    conn_id: u64,
    backtrace_id: u64,
) -> Result<(), String> {
    let mut stmt = tx
        .prepare(
            "SELECT frame_index, function_name, crate_name, crate_module_path, source_file_path, source_line, source_col
             FROM symbolicated_frames
             WHERE conn_id = :conn_id
               AND backtrace_id = :backtrace_id
               AND status = 'resolved'
               AND crate_name IS NOT NULL
               AND crate_name NOT IN (
                    :exclude_1, :exclude_2, :exclude_3, :exclude_4, :exclude_5,
                    :exclude_6, :exclude_7, :exclude_8, :exclude_9, :exclude_10
               )
             ORDER BY frame_index ASC
             LIMIT 1",
        )
        .map_err(|error| format!("prepare top frame query: {error}"))?;
    let selected = stmt
        .facet_query_optional_ref::<TopFrameCandidate, _>(&TopFrameLookupParams {
            conn_id,
            backtrace_id,
            exclude_1: TOP_FRAME_CRATE_EXCLUSIONS[0],
            exclude_2: TOP_FRAME_CRATE_EXCLUSIONS[1],
            exclude_3: TOP_FRAME_CRATE_EXCLUSIONS[2],
            exclude_4: TOP_FRAME_CRATE_EXCLUSIONS[3],
            exclude_5: TOP_FRAME_CRATE_EXCLUSIONS[4],
            exclude_6: TOP_FRAME_CRATE_EXCLUSIONS[5],
            exclude_7: TOP_FRAME_CRATE_EXCLUSIONS[6],
            exclude_8: TOP_FRAME_CRATE_EXCLUSIONS[7],
            exclude_9: TOP_FRAME_CRATE_EXCLUSIONS[8],
            exclude_10: TOP_FRAME_CRATE_EXCLUSIONS[9],
        })
        .map_err(|error| format!("query top frame: {error}"))?;

    match selected {
        Some(candidate) => {
            let mut upsert_stmt = tx
                .prepare(
                "INSERT INTO top_application_frames (
                    conn_id, backtrace_id, frame_index, function_name, crate_name, crate_module_path,
                    source_file_path, source_line, source_col, updated_at_ns
                 ) VALUES (
                    :conn_id, :backtrace_id, :frame_index, :function_name, :crate_name, :crate_module_path,
                    :source_file_path, :source_line, :source_col, :updated_at_ns
                 )
                 ON CONFLICT(conn_id, backtrace_id) DO UPDATE SET
                    frame_index = excluded.frame_index,
                    function_name = excluded.function_name,
                    crate_name = excluded.crate_name,
                    crate_module_path = excluded.crate_module_path,
                    source_file_path = excluded.source_file_path,
                    source_line = excluded.source_line,
                    source_col = excluded.source_col,
                    updated_at_ns = excluded.updated_at_ns",
            )
            .map_err(|error| format!("prepare top frame upsert: {error}"))?;
            upsert_stmt
                .facet_execute_ref(&TopFrameUpsertParams {
                    conn_id,
                    backtrace_id,
                    frame_index: candidate.frame_index,
                    function_name: candidate.function_name,
                    crate_name: candidate.crate_name,
                    crate_module_path: candidate.crate_module_path,
                    source_file_path: candidate.source_file_path,
                    source_line: candidate.source_line,
                    source_col: candidate.source_col,
                    updated_at_ns: now_nanos(),
                })
                .map_err(|error| format!("upsert top_application_frame: {error}"))?;
        }
        None => {
            let mut delete_stmt = tx
                .prepare(
                    "DELETE FROM top_application_frames
                     WHERE conn_id = :conn_id AND backtrace_id = :backtrace_id",
                )
                .map_err(|error| format!("prepare top frame delete: {error}"))?;
            delete_stmt
                .facet_execute_ref(&PendingFrameLookupParams {
                    conn_id,
                    backtrace_id,
                })
                .map_err(|error| format!("delete top_application_frame: {error}"))?;
        }
    }
    Ok(())
}
