use std::collections::{HashMap, hash_map::Entry};
use std::path::Path as FsPath;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use facet::Facet;
use moire_trace_types::{BacktraceId, RelPc};
use moire_types::ProcessId;
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

#[derive(Facet, Clone)]
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
    process_id: ProcessId,
    backtrace_id: BacktraceId,
    frame_index: u32,
    module_path: String,
    module_identity: String,
    rel_pc: RelPc,
}

#[derive(Facet)]
struct PendingFrameLookupParams {
    backtrace_id: BacktraceId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SymbolicationCacheKey {
    module_identity: String,
    rel_pc_raw: u64,
}

impl SymbolicationCacheKey {
    fn new(module_identity: String, rel_pc: RelPc) -> Self {
        Self {
            module_identity,
            rel_pc_raw: rel_pc.get(),
        }
    }

    fn rel_pc(&self) -> RelPc {
        RelPc::new(self.rel_pc_raw).unwrap_or_else(|_| {
            panic!(
                "invariant violated: cached rel_pc must be JS-safe, got {}",
                self.rel_pc_raw
            )
        })
    }
}

#[derive(Clone)]
enum PlannedFrameResolution {
    Cached(SymbolicationCacheEntry),
    ResolveKnown(SymbolicationCacheKey),
    ResolveDirect,
}

#[derive(Clone)]
struct PlannedFrameJob {
    job: PendingFrameJob,
    resolution: PlannedFrameResolution,
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
    process_id: ProcessId,
    backtrace_id: BacktraceId,
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
    process_id: ProcessId,
    backtrace_id: BacktraceId,
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
    process_id: ProcessId,
    backtrace_id: BacktraceId,
    frame_index: i64,
    function_name: Option<String>,
    crate_name: String,
    crate_module_path: Option<String>,
    source_file_path: Option<String>,
    source_line: Option<i64>,
    source_col: Option<i64>,
    updated_at_ns: i64,
}

#[derive(Facet)]
struct TopFrameDeleteParams {
    process_id: ProcessId,
    backtrace_id: BacktraceId,
}

enum ModuleSymbolizerState {
    Ready {
        loader: Box<addr2line::Loader>,
        linked_image_base: u64,
    },
    Failed(String),
}

pub async fn symbolicate_pending_frames_for_backtraces(
    db: Arc<Db>,
    backtrace_ids: &[BacktraceId],
) -> Result<usize, String> {
    if backtrace_ids.is_empty() {
        return Ok(0);
    }
    let backtrace_ids = backtrace_ids.to_vec();
    tokio::task::spawn_blocking(move || {
        symbolicate_pending_frames_for_backtraces_blocking(&db, &backtrace_ids)
    })
    .await
    .map_err(|error| format!("join symbolication worker: {error}"))?
}

fn resolve_frame_jobs_parallel(jobs: &[PendingFrameJob]) -> Vec<SymbolicationCacheEntry> {
    if jobs.is_empty() {
        return Vec::new();
    }

    let worker_count = std::thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
        .min(jobs.len());

    if worker_count <= 1 {
        let mut module_cache: HashMap<String, ModuleSymbolizerState> = HashMap::new();
        return jobs
            .iter()
            .map(|job| resolve_frame_symbolication(job, &mut module_cache))
            .collect();
    }

    let jobs = Arc::new(jobs.to_vec());
    let next_index = Arc::new(AtomicUsize::new(0));
    let produced = Arc::new(Mutex::new(
        Vec::<(usize, SymbolicationCacheEntry)>::with_capacity(jobs.len()),
    ));

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let jobs = Arc::clone(&jobs);
            let next_index = Arc::clone(&next_index);
            let produced = Arc::clone(&produced);
            scope.spawn(move || {
                let mut module_cache: HashMap<String, ModuleSymbolizerState> = HashMap::new();
                let mut local = Vec::<(usize, SymbolicationCacheEntry)>::new();
                loop {
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    if index >= jobs.len() {
                        break;
                    }
                    let resolved = resolve_frame_symbolication(&jobs[index], &mut module_cache);
                    local.push((index, resolved));
                }
                if !local.is_empty() {
                    let Ok(mut sink) = produced.lock() else {
                        panic!("symbolication result sink mutex poisoned");
                    };
                    sink.extend(local);
                }
            });
        }
    });

    let mut slots: Vec<Option<SymbolicationCacheEntry>> = vec![None; jobs.len()];
    let Ok(mut resolved_pairs) = produced.lock() else {
        panic!("symbolication result sink mutex poisoned");
    };
    for (index, resolved) in resolved_pairs.drain(..) {
        slots[index] = Some(resolved);
    }

    slots
        .into_iter()
        .enumerate()
        .map(|(index, maybe_resolved)| {
            maybe_resolved.unwrap_or_else(|| {
                panic!("invariant violated: parallel symbolication missing result for job index {index}")
            })
        })
        .collect()
}

fn symbolicate_pending_frames_for_backtraces_blocking(
    db: &Db,
    backtrace_ids: &[BacktraceId],
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
            "SELECT bf.process_id, bf.backtrace_id, bf.frame_index, bf.module_path, bf.module_identity, bf.rel_pc
             FROM backtrace_frames bf
             LEFT JOIN symbolicated_frames sf
                ON sf.process_id = bf.process_id
               AND sf.backtrace_id = bf.backtrace_id
               AND sf.frame_index = bf.frame_index
             WHERE bf.backtrace_id = :backtrace_id
               AND (
                    sf.process_id IS NULL
                    OR (
                        sf.status = 'unresolved'
                        AND sf.unresolved_reason LIKE 'symbolication engine not wired:%'
                    )
               )
             ORDER BY bf.frame_index ASC",
        )
        .map_err(|error| format!("prepare pending frame query: {error}"))?;

    let mut cached_by_key: HashMap<SymbolicationCacheKey, SymbolicationCacheEntry> = HashMap::new();
    let mut resolve_job_by_key: HashMap<SymbolicationCacheKey, PendingFrameJob> = HashMap::new();
    let mut pending_jobs_by_backtrace = Vec::<(BacktraceId, Vec<PlannedFrameJob>)>::new();
    let mut unknown_module_jobs = 0usize;

    for backtrace_id in backtrace_ids {
        let params = PendingFrameLookupParams {
            backtrace_id: *backtrace_id,
        };
        let jobs = pending_stmt
            .facet_query_ref::<PendingFrameJob, _>(&params)
            .map_err(|error| format!("query pending frame rows: {error}"))?;

        let mut planned_jobs = Vec::with_capacity(jobs.len());
        for job in jobs {
            if job.module_path.starts_with("<unknown-module-id:") {
                unknown_module_jobs = unknown_module_jobs.saturating_add(1);
            }
            if job.module_identity == "unknown" {
                planned_jobs.push(PlannedFrameJob {
                    job,
                    resolution: PlannedFrameResolution::ResolveDirect,
                });
                continue;
            }

            let cache_key = SymbolicationCacheKey::new(job.module_identity.clone(), job.rel_pc);
            if let Some(hit) = cached_by_key.get(&cache_key).cloned() {
                planned_jobs.push(PlannedFrameJob {
                    job,
                    resolution: PlannedFrameResolution::Cached(hit),
                });
                continue;
            }
            if resolve_job_by_key.contains_key(&cache_key) {
                planned_jobs.push(PlannedFrameJob {
                    job,
                    resolution: PlannedFrameResolution::ResolveKnown(cache_key),
                });
                continue;
            }

            if let Some(hit) = lookup_symbolication_cache(
                &tx,
                cache_key.module_identity.as_str(),
                cache_key.rel_pc(),
            )? {
                if hit.status == "unresolved"
                    && hit
                        .unresolved_reason
                        .as_deref()
                        .is_some_and(should_retry_unresolved_reason)
                {
                    debug!(
                        process_id = %job.process_id.as_str(),
                        backtrace_id = %job.backtrace_id,
                        frame_index = job.frame_index,
                        "retrying previously scaffolded unresolved cache entry"
                    );
                    resolve_job_by_key.insert(cache_key.clone(), job.clone());
                    planned_jobs.push(PlannedFrameJob {
                        job,
                        resolution: PlannedFrameResolution::ResolveKnown(cache_key),
                    });
                } else {
                    cached_by_key.insert(cache_key.clone(), hit.clone());
                    planned_jobs.push(PlannedFrameJob {
                        job,
                        resolution: PlannedFrameResolution::Cached(hit),
                    });
                }
            } else {
                resolve_job_by_key.insert(cache_key.clone(), job.clone());
                planned_jobs.push(PlannedFrameJob {
                    job,
                    resolution: PlannedFrameResolution::ResolveKnown(cache_key),
                });
            }
        }
        pending_jobs_by_backtrace.push((*backtrace_id, planned_jobs));
    }

    drop(pending_stmt);

    let jobs_to_resolve_by_key: Vec<(SymbolicationCacheKey, PendingFrameJob)> =
        resolve_job_by_key.into_iter().collect();
    let resolution_inputs: Vec<PendingFrameJob> = jobs_to_resolve_by_key
        .iter()
        .map(|(_, job)| job.clone())
        .collect();
    let resolved_entries = resolve_frame_jobs_parallel(&resolution_inputs);
    for ((cache_key, _), resolved) in jobs_to_resolve_by_key.into_iter().zip(resolved_entries) {
        upsert_symbolication_cache(
            &tx,
            cache_key.module_identity.as_str(),
            cache_key.rel_pc(),
            &resolved,
        )?;
        cached_by_key.insert(cache_key, resolved);
    }

    let mut processed = 0usize;
    let mut direct_module_cache: HashMap<String, ModuleSymbolizerState> = HashMap::new();
    for (backtrace_id, planned_jobs) in &pending_jobs_by_backtrace {
        for planned in planned_jobs {
            let cache = match &planned.resolution {
                PlannedFrameResolution::Cached(cached) => cached.clone(),
                PlannedFrameResolution::ResolveKnown(cache_key) => {
                    cached_by_key
                        .get(cache_key)
                        .cloned()
                        .unwrap_or_else(|| {
                            panic!(
                                "invariant violated: missing resolved cache entry for module_identity={} rel_pc=0x{:x}",
                                cache_key.module_identity,
                                cache_key.rel_pc_raw
                            )
                        })
                }
                PlannedFrameResolution::ResolveDirect => {
                    resolve_frame_symbolication(&planned.job, &mut direct_module_cache)
                }
            };
            upsert_symbolicated_frame(
                &tx,
                planned.job.process_id.clone(),
                planned.job.backtrace_id,
                planned.job.frame_index,
                planned.job.module_path.as_str(),
                planned.job.rel_pc,
                &cache,
            )?;
            processed = processed.saturating_add(1);
        }
        if !planned_jobs.is_empty() {
            let owner_process_id = planned_jobs[0].job.process_id.clone();
            if planned_jobs
                .iter()
                .any(|planned| planned.job.process_id != owner_process_id)
            {
                return Err(format!(
                    "invariant violated: backtrace {} spans multiple process_id values in pending frame rows",
                    backtrace_id
                ));
            }
            update_top_application_frame(&tx, owner_process_id, *backtrace_id)?;
        }
    }

    tx.commit()
        .map_err(|error| format!("commit symbolication pass: {error}"))?;
    if processed > 0 {
        info!(
            processed_frames = processed,
            unknown_module_jobs,
            cache_resolved_entries = resolution_inputs.len(),
            cache_hits_reused = cached_by_key.len().saturating_sub(resolution_inputs.len()),
            elapsed_ms = started.elapsed().as_millis(),
            "symbolication pass completed"
        );
        if resolution_inputs.is_empty() && unknown_module_jobs > 0 {
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
    process_id: ProcessId,
    backtrace_id: BacktraceId,
    frame_index: u32,
    module_path: &str,
    rel_pc: RelPc,
    cache: &SymbolicationCacheEntry,
) -> Result<(), String> {
    let mut stmt = tx
        .prepare(
            "INSERT INTO symbolicated_frames (
            process_id, backtrace_id, frame_index, module_path, rel_pc, status, function_name,
            crate_name, crate_module_path, source_file_path, source_line, source_col,
            unresolved_reason, updated_at_ns
         ) VALUES (
            :process_id, :backtrace_id, :frame_index, :module_path, :rel_pc, :status, :function_name,
            :crate_name, :crate_module_path, :source_file_path, :source_line, :source_col,
            :unresolved_reason, :updated_at_ns
         )
         ON CONFLICT(backtrace_id, frame_index) DO UPDATE SET
            process_id = excluded.process_id,
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
        process_id,
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
    process_id: ProcessId,
    backtrace_id: BacktraceId,
) -> Result<(), String> {
    let mut stmt = tx
        .prepare(
            "SELECT frame_index, function_name, crate_name, crate_module_path, source_file_path, source_line, source_col
             FROM symbolicated_frames
             WHERE process_id = :process_id
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
            process_id: process_id.clone(),
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
                    process_id, backtrace_id, frame_index, function_name, crate_name, crate_module_path,
                    source_file_path, source_line, source_col, updated_at_ns
                 ) VALUES (
                    :process_id, :backtrace_id, :frame_index, :function_name, :crate_name, :crate_module_path,
                    :source_file_path, :source_line, :source_col, :updated_at_ns
                 )
                 ON CONFLICT(backtrace_id) DO UPDATE SET
                    process_id = excluded.process_id,
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
                    process_id: process_id.clone(),
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
                     WHERE process_id = :process_id AND backtrace_id = :backtrace_id",
                )
                .map_err(|error| format!("prepare top frame delete: {error}"))?;
            delete_stmt
                .facet_execute_ref(&TopFrameDeleteParams {
                    process_id,
                    backtrace_id,
                })
                .map_err(|error| format!("delete top_application_frame: {error}"))?;
        }
    }
    Ok(())
}
