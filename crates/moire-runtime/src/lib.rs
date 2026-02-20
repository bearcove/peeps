use core::sync::atomic::{AtomicU64, Ordering};
use ctor::ctor;
use moire_trace_capture::{capture_current, validate_frame_pointers_or_panic, CaptureOptions, CapturedBacktrace};
use moire_trace_types::{BacktraceId, FrameKey, ModuleId};
use moire_types::{
    process_prefix_u16, EntityId, Event, EventKind, EventTarget, ProcessScopeBody, ScopeBody,
    ScopeId, TaskScopeBody,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Mutex as StdMutex, OnceLock};

pub(crate) const MAX_EVENTS: usize = 16_384;
pub(crate) const MAX_CHANGES_BEFORE_COMPACT: usize = 65_536;
pub(crate) const COMPACT_TARGET_CHANGES: usize = 8_192;
pub(crate) const DEFAULT_STREAM_ID_PREFIX: &str = "proc";
pub(crate) const DASHBOARD_PUSH_MAX_CHANGES: u32 = 2048;
pub(crate) const DASHBOARD_PUSH_INTERVAL_MS: u64 = 100;
pub(crate) const DASHBOARD_RECONNECT_DELAY_MS: u64 = 500;

tokio::task_local! {
    pub static FUTURE_CAUSAL_STACK: RefCell<Vec<EntityId>>;
}
thread_local! {
    pub static HELD_MUTEX_STACK: RefCell<Vec<EntityId>> = const { RefCell::new(Vec::new()) };
}

pub(crate) mod api;
pub(crate) mod dashboard;
pub(crate) mod db;
pub(crate) mod futures;
pub(crate) mod handles;

pub use self::api::*;
pub use self::futures::*;
pub use self::handles::*;

static NEXT_BACKTRACE_ID: AtomicU64 = AtomicU64::new(1);
static BACKTRACE_ID_PROCESS_PREFIX: OnceLock<u16> = OnceLock::new();
static PROCESS_SCOPE: OnceLock<ScopeHandle> = OnceLock::new();
static BACKTRACE_RECORDS: OnceLock<StdMutex<BTreeMap<u64, moire_wire::BacktraceRecord>>> =
    OnceLock::new();
static MODULE_STATE: OnceLock<StdMutex<ModuleState>> = OnceLock::new();

#[derive(Default)]
struct ModuleState {
    next_module_id: u64,
    revision: u64,
    by_key: BTreeMap<(u64, String), ModuleId>,
    by_id: BTreeMap<u64, moire_wire::ModuleManifestEntry>,
}

// r[impl process.auto-init]
#[ctor]
fn init_diagnostics_runtime() {
    validate_frame_pointers_or_panic();
    init_runtime_from_macro();
}

pub fn init_runtime_from_macro() {
    let process_name = std::env::current_exe().unwrap().display().to_string();
    PROCESS_SCOPE.get_or_init(|| {
        ScopeHandle::new(
            process_name.clone(),
            ScopeBody::Process(ProcessScopeBody {
                pid: std::process::id(),
            }),
        )
    });
    dashboard::init_dashboard_push_loop(&process_name);
}

pub(crate) fn capture_backtrace_id() -> BacktraceId {
    let raw = next_backtrace_id_raw();
    let backtrace_id = BacktraceId::new(raw)
        .expect("backtrace id invariant violated: generated id must be non-zero");

    let captured = capture_current(backtrace_id, CaptureOptions::default()).unwrap_or_else(|err| {
        panic!("failed to capture backtrace for enabled API boundary: {err}")
    });
    // r[impl wire.backtrace-record]
    let remapped = remap_and_register_backtrace(captured);
    remember_backtrace_record(remapped);

    backtrace_id
}

fn next_backtrace_id_raw() -> u64 {
    // r[impl model.backtrace.id-layout]
    const BACKTRACE_COUNTER_BITS: u32 = 37;
    const BACKTRACE_COUNTER_MAX: u64 = (1u64 << BACKTRACE_COUNTER_BITS) - 1;

    let prefix = *BACKTRACE_ID_PROCESS_PREFIX.get_or_init(process_prefix_u16);
    let counter = NEXT_BACKTRACE_ID.fetch_add(1, Ordering::Relaxed);
    if counter > BACKTRACE_COUNTER_MAX {
        panic!(
            "backtrace id invariant violated: per-process counter overflow (counter={counter})"
        );
    }
    // JS-safe numeric ID layout:
    // high 16 bits: process prefix, low 37 bits: per-process monotonic counter.
    ((prefix as u64) << BACKTRACE_COUNTER_BITS) | counter
}

fn module_state() -> &'static StdMutex<ModuleState> {
    MODULE_STATE.get_or_init(|| {
        StdMutex::new(ModuleState {
            next_module_id: 1,
            ..ModuleState::default()
        })
    })
}

fn module_identity_for(path: &str, runtime_base: u64) -> moire_wire::ModuleIdentity {
    // Deterministic runtime identity until build-id/debug-id extraction is wired.
    moire_wire::ModuleIdentity::DebugId(format!("runtime:{runtime_base:x}:{path}"))
}

fn remap_and_register_backtrace(captured: CapturedBacktrace) -> moire_wire::BacktraceRecord {
    let Ok(mut modules) = module_state().lock() else {
        panic!("module state mutex poisoned; cannot continue");
    };

    let mut local_to_global: BTreeMap<u64, ModuleId> = BTreeMap::new();
    for module in &captured.modules {
        let key = (module.runtime_base, module.path.as_str().to_string());
        let global = if let Some(existing) = modules.by_key.get(&key).copied() {
            existing
        } else {
            let raw_id = modules.next_module_id;
            modules.next_module_id = modules
                .next_module_id
                .checked_add(1)
                .expect("module id overflow");
            let global = ModuleId::new(raw_id)
                .expect("invariant violated: generated module id must be non-zero");
            modules.by_key.insert(key.clone(), global);
            modules.by_id.insert(
                global.get(),
                moire_wire::ModuleManifestEntry {
                    module_path: key.1.clone(),
                    runtime_base: key.0,
                    identity: module_identity_for(&key.1, key.0),
                    arch: std::env::consts::ARCH.to_string(),
                },
            );
            modules.revision = modules.revision.saturating_add(1);
            global
        };
        local_to_global.insert(module.id.get(), global);
    }

    let remapped_frames = captured
        .backtrace
        .frames
        .iter()
        .map(|frame| {
            let module_id = local_to_global.get(&frame.module_id.get()).copied().unwrap_or_else(|| {
                panic!(
                    "invariant violated: missing local module mapping for module_id {}",
                    frame.module_id.get()
                )
            });
            FrameKey {
                module_id,
                rel_pc: frame.rel_pc,
            }
        })
        .collect();

    moire_wire::BacktraceRecord::new(captured.backtrace.id, remapped_frames)
        .expect("invariant violated: remapped backtrace must be valid")
}

pub(crate) fn module_manifest_snapshot() -> (u64, Vec<moire_wire::ModuleManifestEntry>) {
    let Ok(modules) = module_state().lock() else {
        panic!("module state mutex poisoned; cannot continue");
    };
    (
        modules.revision,
        modules.by_id.values().cloned().collect::<Vec<_>>(),
    )
}

fn backtrace_records() -> &'static StdMutex<BTreeMap<u64, moire_wire::BacktraceRecord>> {
    BACKTRACE_RECORDS.get_or_init(|| StdMutex::new(BTreeMap::new()))
}

// r[impl wire.backtrace-record]
pub(crate) fn remember_backtrace_record(record: moire_wire::BacktraceRecord) {
    let Ok(mut records) = backtrace_records().lock() else {
        panic!("backtrace record mutex poisoned; cannot continue");
    };
    let record_id = record.id.get();
    match records.get(&record_id) {
        Some(existing) if existing == &record => {}
        Some(_) => panic!(
            "backtrace record invariant violated: conflicting payload for id {}",
            record_id
        ),
        None => {
            records.insert(record_id, record);
        }
    }
}

pub(crate) fn backtrace_records_after(
    last_sent_backtrace_id: u64,
) -> Vec<moire_wire::BacktraceRecord> {
    let Ok(records) = backtrace_records().lock() else {
        panic!("backtrace record mutex poisoned; cannot continue");
    };
    records
        .range((last_sent_backtrace_id + 1)..)
        .map(|(_, record)| record.clone())
        .collect()
}

pub fn current_process_scope_id() -> Option<ScopeId> {
    PROCESS_SCOPE
        .get()
        .map(|scope| ScopeId::new(scope.id().as_str()))
}

pub fn current_tokio_task_key() -> Option<String> {
    tokio::task::try_id().map(|id| String::from(id.to_string()))
}

pub struct TaskScopeRegistration {
    task_key: String,
    scope: ScopeHandle,
}

impl Drop for TaskScopeRegistration {
    fn drop(&mut self) {
        if let Ok(mut db) = db::runtime_db().lock() {
            db.unregister_task_scope_id(&self.task_key, self.scope.id());
        }
    }
}

pub fn register_current_task_scope(task_name: &str) -> Option<TaskScopeRegistration> {
    let task_key = current_tokio_task_key()?;
    let scope = ScopeHandle::new(
        format!("task.{task_name}#{task_key}"),
        ScopeBody::Task(TaskScopeBody {
            task_key: task_key.clone(),
        }),
    );
    if let Ok(mut db) = db::runtime_db().lock() {
        db.register_task_scope_id(&task_key, scope.id());
    }
    Some(TaskScopeRegistration { task_key, scope })
}

pub fn new_event(target: EventTarget, kind: EventKind) -> Event {
    Event::new(target, kind, capture_backtrace_id())
}

pub fn record_event(event: Event) {
    if let Ok(mut db) = db::runtime_db().lock() {
        db.record_event(event);
    }
}

pub fn record_event_with_entity_source(mut event: Event, entity_id: &EntityId) {
    if let Ok(mut db) = db::runtime_db().lock() {
        if let Some(entity) = db.entities.get(entity_id) {
            event.backtrace = entity.backtrace;
        }
        db.record_event(event);
    }
}

pub fn init_dashboard_push_loop(process_name: &str) {
    dashboard::init_dashboard_push_loop(process_name)
}
