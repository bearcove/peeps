use std::collections::{HashMap, hash_map::Entry};
use std::path::Path as FsPath;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use object::{Object, ObjectSegment};
use rusqlite::{Transaction, params};
use tracing::{debug, info, warn};

use crate::db::Db;
use crate::util::time::{now_nanos, to_i64_u64};

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

#[derive(Clone)]
struct PendingFrameJob {
    conn_id: u64,
    backtrace_id: u64,
    frame_index: u32,
    module_path: String,
    module_identity: String,
    rel_pc: u64,
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
             WHERE bf.conn_id = ?1
               AND bf.backtrace_id = ?2
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
        let jobs = pending_stmt
            .query_map(
                params![to_i64_u64(*conn_id), to_i64_u64(*backtrace_id)],
                |row| {
                    Ok(PendingFrameJob {
                        conn_id: row.get::<_, i64>(0)? as u64,
                        backtrace_id: row.get::<_, i64>(1)? as u64,
                        frame_index: row.get::<_, i64>(2)? as u32,
                        module_path: row.get(3)?,
                        module_identity: row.get(4)?,
                        rel_pc: row.get::<_, i64>(5)? as u64,
                    })
                },
            )
            .map_err(|error| format!("query pending frame rows: {error}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("read pending frame row: {error}"))?;

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
    let lookup_pc = match linked_image_base.checked_add(job.rel_pc) {
        Some(pc) => pc,
        None => {
            return unresolved(format!(
                "address overflow combining linked image base 0x{:x} with rel_pc 0x{:x} for '{}'",
                linked_image_base, job.rel_pc, job.module_path
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
                job.module_path, job.rel_pc
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
                                    job.module_path, job.rel_pc
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
                    job.module_path, job.rel_pc
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
            job.module_path, job.rel_pc
        ));
    };
    let function_name = function_name.unwrap_or_else(|| {
        format!(
            "{}+0x{:x}",
            job.module_path
                .rsplit('/')
                .next()
                .unwrap_or(job.module_path.as_str()),
            job.rel_pc
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
    rel_pc: u64,
) -> Result<Option<SymbolicationCacheEntry>, String> {
    let mut stmt = tx
        .prepare(
            "SELECT status, function_name, crate_name, crate_module_path,
                    source_file_path, source_line, source_col, unresolved_reason
             FROM symbolication_cache
             WHERE module_identity = ?1 AND rel_pc = ?2",
        )
        .map_err(|error| format!("prepare symbolication_cache lookup: {error}"))?;
    match stmt.query_row(params![module_identity, to_i64_u64(rel_pc)], |row| {
        Ok(SymbolicationCacheEntry {
            status: row.get::<_, String>(0)?,
            function_name: row.get(1)?,
            crate_name: row.get(2)?,
            crate_module_path: row.get(3)?,
            source_file_path: row.get(4)?,
            source_line: row.get(5)?,
            source_col: row.get(6)?,
            unresolved_reason: row.get(7)?,
        })
    }) {
        Ok(entry) => Ok(Some(entry)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(format!("query symbolication_cache: {error}")),
    }
}

fn upsert_symbolication_cache(
    tx: &Transaction<'_>,
    module_identity: &str,
    rel_pc: u64,
    cache: &SymbolicationCacheEntry,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO symbolication_cache (
            module_identity, rel_pc, status, function_name, crate_name, crate_module_path,
            source_file_path, source_line, source_col, unresolved_reason, updated_at_ns
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
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
        params![
            module_identity,
            to_i64_u64(rel_pc),
            cache.status.as_str(),
            cache.function_name.as_ref(),
            cache.crate_name.as_ref(),
            cache.crate_module_path.as_ref(),
            cache.source_file_path.as_ref(),
            cache.source_line,
            cache.source_col,
            cache.unresolved_reason.as_ref(),
            now_nanos(),
        ],
    )
    .map_err(|error| format!("upsert symbolication_cache: {error}"))?;
    Ok(())
}

fn upsert_symbolicated_frame(
    tx: &Transaction<'_>,
    conn_id: u64,
    backtrace_id: u64,
    frame_index: u32,
    module_path: &str,
    rel_pc: u64,
    cache: &SymbolicationCacheEntry,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO symbolicated_frames (
            conn_id, backtrace_id, frame_index, module_path, rel_pc, status, function_name,
            crate_name, crate_module_path, source_file_path, source_line, source_col,
            unresolved_reason, updated_at_ns
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
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
        params![
            to_i64_u64(conn_id),
            to_i64_u64(backtrace_id),
            i64::from(frame_index),
            module_path,
            to_i64_u64(rel_pc),
            cache.status.as_str(),
            cache.function_name.as_ref(),
            cache.crate_name.as_ref(),
            cache.crate_module_path.as_ref(),
            cache.source_file_path.as_ref(),
            cache.source_line,
            cache.source_col,
            cache.unresolved_reason.as_ref(),
            now_nanos(),
        ],
    )
    .map_err(|error| format!("upsert symbolicated_frame[{frame_index}]: {error}"))?;
    Ok(())
}

fn update_top_application_frame(
    tx: &Transaction<'_>,
    conn_id: u64,
    backtrace_id: u64,
) -> Result<(), String> {
    let mut query = String::from(
        "SELECT frame_index, function_name, crate_name, crate_module_path, source_file_path, source_line, source_col
         FROM symbolicated_frames
         WHERE conn_id = ?1 AND backtrace_id = ?2
           AND status = 'resolved'
           AND crate_name IS NOT NULL
           AND crate_name NOT IN (",
    );
    for (index, _) in TOP_FRAME_CRATE_EXCLUSIONS.iter().enumerate() {
        if index > 0 {
            query.push_str(", ");
        }
        query.push('?');
        query.push_str(&(index + 3).to_string());
    }
    query.push_str(") ORDER BY frame_index ASC LIMIT 1");

    let params = rusqlite::params_from_iter(
        std::iter::once(rusqlite::types::Value::from(to_i64_u64(conn_id)))
            .chain(std::iter::once(rusqlite::types::Value::from(to_i64_u64(
                backtrace_id,
            ))))
            .chain(
                TOP_FRAME_CRATE_EXCLUSIONS
                    .iter()
                    .map(|name| rusqlite::types::Value::from((*name).to_string())),
            ),
    );

    let mut stmt = tx
        .prepare(query.as_str())
        .map_err(|error| format!("prepare top frame query: {error}"))?;
    #[allow(clippy::type_complexity)]
    let selected: Result<
        Option<(
            i64,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
        )>,
        String,
    > = match stmt.query_row(params, |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<i64>>(6)?,
        ))
    }) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(format!("query top frame: {error}")),
    };

    match selected? {
        Some((
            frame_index,
            function_name,
            crate_name,
            crate_module_path,
            source_file_path,
            source_line,
            source_col,
        )) => {
            tx.execute(
                "INSERT INTO top_application_frames (
                    conn_id, backtrace_id, frame_index, function_name, crate_name, crate_module_path,
                    source_file_path, source_line, source_col, updated_at_ns
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(conn_id, backtrace_id) DO UPDATE SET
                    frame_index = excluded.frame_index,
                    function_name = excluded.function_name,
                    crate_name = excluded.crate_name,
                    crate_module_path = excluded.crate_module_path,
                    source_file_path = excluded.source_file_path,
                    source_line = excluded.source_line,
                    source_col = excluded.source_col,
                    updated_at_ns = excluded.updated_at_ns",
                params![
                    to_i64_u64(conn_id),
                    to_i64_u64(backtrace_id),
                    frame_index,
                    function_name,
                    crate_name,
                    crate_module_path,
                    source_file_path,
                    source_line,
                    source_col,
                    now_nanos(),
                ],
            )
            .map_err(|error| format!("upsert top_application_frame: {error}"))?;
        }
        None => {
            tx.execute(
                "DELETE FROM top_application_frames WHERE conn_id = ?1 AND backtrace_id = ?2",
                params![to_i64_u64(conn_id), to_i64_u64(backtrace_id)],
            )
            .map_err(|error| format!("delete top_application_frame: {error}"))?;
        }
    }
    Ok(())
}
