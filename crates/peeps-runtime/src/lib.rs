use peeps_types::{
    EntityBody, EntityId, Event, FutureEntity, ProcessScopeBody, ScopeBody, ScopeId, TaskScopeBody,
};
use std::cell::RefCell;
use std::future::Future;
use std::sync::OnceLock;

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
pub use peeps_source::*;

const RUNTIME_SOURCE_LEFT: SourceLeft =
    SourceLeft::new(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"));

pub(crate) fn local_source(right: SourceRight) -> Source {
    RUNTIME_SOURCE_LEFT.join(right)
}

static PROCESS_SCOPE: OnceLock<ScopeHandle> = OnceLock::new();

pub fn init_runtime_from_macro() {
    let process_name = std::env::current_exe().unwrap().display().to_string();
    PROCESS_SCOPE.get_or_init(|| {
        ScopeHandle::new(
            process_name.clone(),
            ScopeBody::Process(ProcessScopeBody {
                pid: std::process::id(),
            }),
            local_source(SourceRight::caller()),
        )
    });
    dashboard::init_dashboard_push_loop(&process_name);
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
    source: Source,
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

#[track_caller]
pub fn spawn_tracked<F>(
    name: impl Into<String>,
    fut: F,
    source: Source,
) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let name: String = name.into();
    tokio::spawn(
        FUTURE_CAUSAL_STACK.scope(RefCell::new(Vec::new()), async move {
            let _task_scope = register_current_task_scope(name.as_str(), source.clone());
            instrument_future(name, fut, source, None, None).await
        }),
    )
}

#[track_caller]
pub fn spawn_blocking_tracked<F, T>(
    name: impl Into<String>,
    f: F,
    source: Source,
) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let handle = EntityHandle::new(name, EntityBody::Future(FutureEntity {}), source);
    tokio::task::spawn_blocking(move || {
        let _hold = handle;
        f()
    })
}

pub fn record_event_with_source(mut event: Event, source: &Source) {
    event.source = source.clone().into();
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
