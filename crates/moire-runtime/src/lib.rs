use core::sync::atomic::{AtomicU64, Ordering};
use ctor::ctor;
use moire_trace_capture::{capture_current, validate_frame_pointers_or_panic, CaptureOptions};
use moire_trace_types::BacktraceId;
use moire_types::{
    EntityBody, EntityId, Event, FutureEntity, ProcessScopeBody, ScopeBody, ScopeId, TaskScopeBody,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
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
static PROCESS_SCOPE: OnceLock<ScopeHandle> = OnceLock::new();
static BACKTRACE_RECORDS: OnceLock<StdMutex<BTreeMap<u64, moire_wire::BacktraceRecord>>> =
    OnceLock::new();

// r[impl process.auto-init]
#[ctor]
fn init_diagnostics_runtime() {
    validate_frame_pointers_or_panic();
    init_runtime_from_macro(capture_backtrace_id());
}

pub fn init_runtime_from_macro(backtrace: BacktraceId) {
    let process_name = std::env::current_exe().unwrap().display().to_string();
    PROCESS_SCOPE.get_or_init(|| {
        ScopeHandle::new(
            process_name.clone(),
            ScopeBody::Process(ProcessScopeBody {
                pid: std::process::id(),
            }),
            backtrace,
        )
    });
    dashboard::init_dashboard_push_loop(&process_name);
}

pub fn capture_backtrace_id() -> BacktraceId {
    let raw = NEXT_BACKTRACE_ID.fetch_add(1, Ordering::Relaxed);
    let backtrace_id = BacktraceId::new(raw)
        .expect("backtrace id invariant violated: generated id must be non-zero");

    let captured = capture_current(backtrace_id, CaptureOptions::default()).unwrap_or_else(|err| {
        panic!("failed to capture backtrace for enabled API boundary: {err}")
    });
    // r[impl wire.backtrace-record]
    remember_backtrace_record(moire_wire::BacktraceRecord {
        id: captured.backtrace.id.get(),
        frames: captured
            .backtrace
            .frames
            .into_iter()
            .map(|frame| moire_wire::BacktraceFrameKey {
                module_id: frame.module_id.get(),
                rel_pc: frame.rel_pc,
            })
            .collect(),
    });

    backtrace_id
}

fn backtrace_records() -> &'static StdMutex<BTreeMap<u64, moire_wire::BacktraceRecord>> {
    BACKTRACE_RECORDS.get_or_init(|| StdMutex::new(BTreeMap::new()))
}

// r[impl wire.backtrace-record]
pub fn remember_backtrace_record(record: moire_wire::BacktraceRecord) {
    let Ok(mut records) = backtrace_records().lock() else {
        panic!("backtrace record mutex poisoned; cannot continue");
    };
    match records.get(&record.id) {
        Some(existing) if existing == &record => {}
        Some(_) => panic!(
            "backtrace record invariant violated: conflicting payload for id {}",
            record.id
        ),
        None => {
            records.insert(record.id, record);
        }
    }
}

pub fn backtrace_records_after(last_sent_backtrace_id: u64) -> Vec<moire_wire::BacktraceRecord> {
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

pub fn register_current_task_scope(
    task_name: &str,
    source: BacktraceId,
) -> Option<TaskScopeRegistration> {
    let task_key = current_tokio_task_key()?;
    let scope = ScopeHandle::new(
        format!("task.{task_name}#{task_key}"),
        ScopeBody::Task(TaskScopeBody {
            task_key: task_key.clone(),
        }),
        source,
    );
    if let Ok(mut db) = db::runtime_db().lock() {
        db.register_task_scope_id(&task_key, scope.id());
    }
    Some(TaskScopeRegistration { task_key, scope })
}

pub fn record_event_with_source(mut event: Event, source: BacktraceId) {
    event.source = source;
    if let Ok(mut db) = db::runtime_db().lock() {
        db.record_event(event);
    }
}

pub fn record_event_with_entity_source(mut event: Event, entity_id: &EntityId) {
    if let Ok(mut db) = db::runtime_db().lock() {
        if let Some(entity) = db.entities.get(entity_id) {
            event.source = entity.source;
        }
        db.record_event(event);
    }
}

pub fn init_dashboard_push_loop(process_name: &str) {
    dashboard::init_dashboard_push_loop(process_name)
}
