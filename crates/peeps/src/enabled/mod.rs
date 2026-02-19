use compact_str::{CompactString, ToCompactString};
use peeps_types::infer_krate_from_source_with_manifest_dir;
use peeps_types::{EntityBody, EntityId, Event, ScopeBody, ScopeId};
use std::cell::RefCell;
use std::future::Future;
use std::panic::Location;
use std::path::PathBuf;
use std::sync::OnceLock;

pub(crate) const MAX_EVENTS: usize = 16_384;
pub(crate) const MAX_CHANGES_BEFORE_COMPACT: usize = 65_536;
pub(crate) const COMPACT_TARGET_CHANGES: usize = 8_192;
pub(crate) const DEFAULT_STREAM_ID_PREFIX: &str = "proc";
pub(crate) const DASHBOARD_PUSH_MAX_CHANGES: u32 = 2048;
pub(crate) const DASHBOARD_PUSH_INTERVAL_MS: u64 = 100;
pub(crate) const DASHBOARD_RECONNECT_DELAY_MS: u64 = 500;

tokio::task_local! {
    pub(crate) static FUTURE_CAUSAL_STACK: RefCell<Vec<EntityId>>;
}
thread_local! {
    pub(crate) static HELD_MUTEX_STACK: RefCell<Vec<EntityId>> = const { RefCell::new(Vec::new()) };
}

pub(crate) mod api;
pub(crate) mod channels;
pub(crate) mod dashboard;
pub(crate) mod db;
pub(crate) mod futures;
pub(crate) mod handles;
pub(crate) mod joinset;
pub(crate) mod process;
pub(crate) mod rpc;
pub(crate) mod sync;

pub(crate) mod source;

pub use self::api::*;
pub use self::channels::*;
pub use self::futures::*;
pub use self::handles::*;
pub use self::process::*;
pub use self::rpc::*;
pub use self::sync::*;

static PROCESS_SCOPE: OnceLock<ScopeHandle> = OnceLock::new();

// facade! expands to a call to this
#[doc(hidden)]
pub fn __init_from_macro(manifest_dir: &str) {
    let process_name = std::env::current_exe()
        .unwrap()
        .display()
        .to_compact_string();
    PROCESS_SCOPE.get_or_init(|| {
        ScopeHandle::new(
            process_name.clone(),
            ScopeBody::Process,
            UnqualSource::caller(),
        )
    });
    dashboard::init_dashboard_push_loop(&process_name);
}

pub(super) fn current_process_scope_id() -> Option<ScopeId> {
    PROCESS_SCOPE
        .get()
        .map(|scope| ScopeId::new(scope.id().as_str()))
}

pub(super) fn current_tokio_task_key() -> Option<CompactString> {
    tokio::task::try_id().map(|id| CompactString::from(id.to_string()))
}

pub(super) struct TaskScopeRegistration {
    task_key: CompactString,
    scope: ScopeHandle,
}

impl Drop for TaskScopeRegistration {
    fn drop(&mut self) {
        if let Ok(mut db) = db::runtime_db().lock() {
            db.unregister_task_scope_id(&self.task_key, self.scope.id());
        }
    }
}

pub(super) fn register_current_task_scope(
    task_name: &str,
    source: UnqualSource,
) -> Option<TaskScopeRegistration> {
    let task_key = current_tokio_task_key()?;
    let scope = ScopeHandle::new(
        format!("task.{task_name}#{task_key}"),
        ScopeBody::Task,
        source,
    );
    if let Ok(mut db) = db::runtime_db().lock() {
        db.register_task_scope_id(&task_key, scope.id());
    }
    Some(TaskScopeRegistration { task_key, scope })
}

#[track_caller]
pub fn spawn_tracked<F>(
    name: impl Into<CompactString>,
    fut: F,
    source: UnqualSource,
) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let name: CompactString = name.into();
    tokio::spawn(
        FUTURE_CAUSAL_STACK.scope(RefCell::new(Vec::new()), async move {
            let _task_scope = register_current_task_scope(name.as_str(), source);
            instrument_future_named(name, fut, source).await
        }),
    )
}

#[track_caller]
pub fn spawn_blocking_tracked<F, T>(
    name: impl Into<CompactString>,
    f: F,
    source: UnqualSource,
) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let handle = EntityHandle::new(name, EntityBody::Future, source);
    tokio::task::spawn_blocking(move || {
        let _hold = handle;
        f()
    })
}

pub(super) fn record_event_with_source(mut event: Event, source: UnqualSource, cx: CrateContext) {
    event.source = source.into_compact_string();
    event.krate =
        infer_krate_from_source_with_manifest_dir(event.source.as_str(), Some(cx.manifest_dir()));
    if let Ok(mut db) = db::runtime_db().lock() {
        db.record_event(event);
    }
}

pub(super) fn record_event_with_entity_source(mut event: Event, entity_id: &EntityId) {
    if let Ok(mut db) = db::runtime_db().lock() {
        if let Some(entity) = db.entities.get(entity_id) {
            event.source = CompactString::from(entity.source.as_str());
            event.krate = entity
                .krate
                .as_ref()
                .map(|k| CompactString::from(k.as_str()));
        }
        db.record_event(event);
    }
}

#[macro_export]
macro_rules! facade {
    () => {
        $crate::__init_from_macro(env!("CARGO_MANIFEST_DIR"));

        pub mod peeps {
            pub const PEEPS_CX: $crate::PeepsContext =
                $crate::PeepsContext::new(env!("CARGO_MANIFEST_DIR"));

            pub trait MutexExt<T> {
                fn lock(&self) -> $crate::MutexGuard<'_, T>;
                fn try_lock(&self) -> Option<$crate::MutexGuard<'_, T>>;
            }

            impl<T> MutexExt<T> for $crate::Mutex<T> {
                #[track_caller]
                fn lock(&self) -> $crate::MutexGuard<'_, T> {
                    self.lock_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn try_lock(&self) -> Option<$crate::MutexGuard<'_, T>> {
                    self.try_lock_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait RwLockExt<T> {
                fn read(&self) -> $crate::parking_lot::RwLockReadGuard<'_, T>;
                fn write(&self) -> $crate::parking_lot::RwLockWriteGuard<'_, T>;
                fn try_read(&self) -> Option<$crate::parking_lot::RwLockReadGuard<'_, T>>;
                fn try_write(&self) -> Option<$crate::parking_lot::RwLockWriteGuard<'_, T>>;
            }

            impl<T> RwLockExt<T> for $crate::RwLock<T> {
                #[track_caller]
                fn read(&self) -> $crate::parking_lot::RwLockReadGuard<'_, T> {
                    self.read_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn write(&self) -> $crate::parking_lot::RwLockWriteGuard<'_, T> {
                    self.write_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn try_read(&self) -> Option<$crate::parking_lot::RwLockReadGuard<'_, T>> {
                    self.try_read_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn try_write(&self) -> Option<$crate::parking_lot::RwLockWriteGuard<'_, T>> {
                    self.try_write_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait SenderExt<T> {
                fn send(
                    &self,
                    value: T,
                ) -> impl core::future::Future<
                    Output = Result<(), $crate::tokio::sync::mpsc::error::SendError<T>>,
                > + '_;
            }

            impl<T> SenderExt<T> for $crate::Sender<T> {
                #[track_caller]
                fn send(
                    &self,
                    value: T,
                ) -> impl core::future::Future<
                    Output = Result<(), $crate::tokio::sync::mpsc::error::SendError<T>>,
                > + '_ {
                    self.send_with_source(value, $crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait ReceiverExt<T> {
                fn recv(&mut self) -> impl core::future::Future<Output = Option<T>> + '_;
            }

            impl<T> ReceiverExt<T> for $crate::Receiver<T> {
                #[track_caller]
                fn recv(&mut self) -> impl core::future::Future<Output = Option<T>> + '_ {
                    self.recv_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait UnboundedSenderExt<T> {
                fn send(
                    &self,
                    value: T,
                ) -> Result<(), $crate::tokio::sync::mpsc::error::SendError<T>>;
            }

            impl<T> UnboundedSenderExt<T> for $crate::UnboundedSender<T> {
                #[track_caller]
                fn send(
                    &self,
                    value: T,
                ) -> Result<(), $crate::tokio::sync::mpsc::error::SendError<T>> {
                    self.send_with_source(value, $crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait UnboundedReceiverExt<T> {
                fn recv(&mut self) -> impl core::future::Future<Output = Option<T>> + '_;
            }

            impl<T> UnboundedReceiverExt<T> for $crate::UnboundedReceiver<T> {
                #[track_caller]
                fn recv(&mut self) -> impl core::future::Future<Output = Option<T>> + '_ {
                    self.recv_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait OneshotSenderExt<T> {
                fn send(self, value: T) -> Result<(), T>;
            }

            impl<T> OneshotSenderExt<T> for $crate::OneshotSender<T> {
                #[track_caller]
                fn send(self, value: T) -> Result<(), T> {
                    self.send_with_source(value, $crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait OneshotReceiverExt<T> {
                fn recv(
                    self,
                ) -> impl core::future::Future<
                    Output = Result<T, $crate::tokio::sync::oneshot::error::RecvError>,
                >;
            }

            impl<T> OneshotReceiverExt<T> for $crate::OneshotReceiver<T> {
                #[track_caller]
                fn recv(
                    self,
                ) -> impl core::future::Future<
                    Output = Result<T, $crate::tokio::sync::oneshot::error::RecvError>,
                > {
                    self.recv_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait BroadcastSenderExt<T: Clone> {
                fn send(
                    &self,
                    value: T,
                ) -> Result<usize, $crate::tokio::sync::broadcast::error::SendError<T>>;
            }

            impl<T: Clone> BroadcastSenderExt<T> for $crate::BroadcastSender<T> {
                #[track_caller]
                fn send(
                    &self,
                    value: T,
                ) -> Result<usize, $crate::tokio::sync::broadcast::error::SendError<T>> {
                    self.send_with_source(value, $crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait BroadcastReceiverExt<T: Clone> {
                fn recv(
                    &mut self,
                ) -> impl core::future::Future<
                    Output = Result<T, $crate::tokio::sync::broadcast::error::RecvError>,
                > + '_;
            }

            impl<T: Clone> BroadcastReceiverExt<T> for $crate::BroadcastReceiver<T> {
                #[track_caller]
                fn recv(
                    &mut self,
                ) -> impl core::future::Future<
                    Output = Result<T, $crate::tokio::sync::broadcast::error::RecvError>,
                > + '_ {
                    self.recv_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait WatchSenderExt<T: Clone> {
                fn send(
                    &self,
                    value: T,
                ) -> Result<(), $crate::tokio::sync::watch::error::SendError<T>>;
                fn send_replace(&self, value: T) -> T;
            }

            impl<T: Clone> WatchSenderExt<T> for $crate::WatchSender<T> {
                #[track_caller]
                fn send(
                    &self,
                    value: T,
                ) -> Result<(), $crate::tokio::sync::watch::error::SendError<T>> {
                    self.send_with_source(value, $crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn send_replace(&self, value: T) -> T {
                    self.send_replace_with_source(value, $crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait WatchReceiverExt<T: Clone> {
                fn changed(
                    &mut self,
                ) -> impl core::future::Future<
                    Output = Result<(), $crate::tokio::sync::watch::error::RecvError>,
                > + '_;
                fn borrow(&self) -> $crate::tokio::sync::watch::Ref<'_, T>;
                fn borrow_and_update(&mut self) -> $crate::tokio::sync::watch::Ref<'_, T>;
            }

            impl<T: Clone> WatchReceiverExt<T> for $crate::WatchReceiver<T> {
                #[track_caller]
                fn changed(
                    &mut self,
                ) -> impl core::future::Future<
                    Output = Result<(), $crate::tokio::sync::watch::error::RecvError>,
                > + '_ {
                    self.changed_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn borrow(&self) -> $crate::tokio::sync::watch::Ref<'_, T> {
                    self.borrow()
                }

                #[track_caller]
                fn borrow_and_update(&mut self) -> $crate::tokio::sync::watch::Ref<'_, T> {
                    self.borrow_and_update()
                }
            }

            pub trait NotifyExt {
                fn notified(&self) -> impl core::future::Future<Output = ()> + '_;
                fn notify_one(&self);
                fn notify_waiters(&self);
            }

            impl NotifyExt for $crate::Notify {
                #[track_caller]
                fn notified(&self) -> impl core::future::Future<Output = ()> + '_ {
                    self.notified_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn notify_one(&self) {
                    self.notify_one()
                }

                #[track_caller]
                fn notify_waiters(&self) {
                    self.notify_waiters()
                }
            }

            pub trait OnceCellExt<T> {
                fn get_or_init<'a, F, Fut>(&'a self, f: F) -> impl core::future::Future<Output = &'a T> + 'a
                where
                    T: 'a,
                    F: FnOnce() -> Fut + 'a,
                    Fut: core::future::Future<Output = T> + 'a;

                fn get_or_try_init<'a, F, Fut, E>(
                    &'a self,
                    f: F,
                ) -> impl core::future::Future<Output = Result<&'a T, E>> + 'a
                where
                    T: 'a,
                    F: FnOnce() -> Fut + 'a,
                    Fut: core::future::Future<Output = Result<T, E>> + 'a;
            }

            impl<T> OnceCellExt<T> for $crate::OnceCell<T> {
                #[track_caller]
                fn get_or_init<'a, F, Fut>(&'a self, f: F) -> impl core::future::Future<Output = &'a T> + 'a
                where
                    T: 'a,
                    F: FnOnce() -> Fut + 'a,
                    Fut: core::future::Future<Output = T> + 'a,
                {
                    self.get_or_init_with_source(f, $crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn get_or_try_init<'a, F, Fut, E>(
                    &'a self,
                    f: F,
                ) -> impl core::future::Future<Output = Result<&'a T, E>> + 'a
                where
                    T: 'a,
                    F: FnOnce() -> Fut + 'a,
                    Fut: core::future::Future<Output = Result<T, E>> + 'a,
                {
                    self.get_or_try_init_with_source(f, $crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait SemaphoreExt {
                fn acquire(
                    &self,
                ) -> impl core::future::Future<
                    Output = Result<$crate::SemaphorePermit<'_>, $crate::tokio::sync::AcquireError>,
                > + '_;
                fn acquire_many(
                    &self,
                    n: u32,
                ) -> impl core::future::Future<
                    Output = Result<$crate::SemaphorePermit<'_>, $crate::tokio::sync::AcquireError>,
                > + '_;
                fn acquire_owned(
                    &self,
                ) -> impl core::future::Future<
                    Output = Result<$crate::OwnedSemaphorePermit, $crate::tokio::sync::AcquireError>,
                > + '_;
                fn acquire_many_owned(
                    &self,
                    n: u32,
                ) -> impl core::future::Future<
                    Output = Result<$crate::OwnedSemaphorePermit, $crate::tokio::sync::AcquireError>,
                > + '_;
            }

            impl SemaphoreExt for $crate::Semaphore {
                #[track_caller]
                fn acquire(
                    &self,
                ) -> impl core::future::Future<
                    Output = Result<$crate::SemaphorePermit<'_>, $crate::tokio::sync::AcquireError>,
                > + '_ {
                    self.acquire_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn acquire_many(
                    &self,
                    n: u32,
                ) -> impl core::future::Future<
                    Output = Result<$crate::SemaphorePermit<'_>, $crate::tokio::sync::AcquireError>,
                > + '_ {
                    self.acquire_many_with_source(n, $crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn acquire_owned(
                    &self,
                ) -> impl core::future::Future<
                    Output = Result<$crate::OwnedSemaphorePermit, $crate::tokio::sync::AcquireError>,
                > + '_ {
                    self.acquire_owned_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn acquire_many_owned(
                    &self,
                    n: u32,
                ) -> impl core::future::Future<
                    Output = Result<$crate::OwnedSemaphorePermit, $crate::tokio::sync::AcquireError>,
                > + '_ {
                    self.acquire_many_owned_with_source(n, $crate::Source::caller(), PEEPS_CX)
                }
            }

            pub trait JoinSetExt<T>
            where
                T: Send + 'static,
            {
                fn spawn<F>(&mut self, label: &'static str, future: F)
                where
                    F: core::future::Future<Output = T> + Send + 'static;

                fn join_next(
                    &mut self,
                ) -> impl core::future::Future<
                    Output = Option<Result<T, $crate::tokio::task::JoinError>>,
                > + '_;
            }

            impl<T> JoinSetExt<T> for $crate::JoinSet<T>
            where
                T: Send + 'static,
            {
                #[track_caller]
                fn spawn<F>(&mut self, label: &'static str, future: F)
                where
                    F: core::future::Future<Output = T> + Send + 'static,
                {
                    self.spawn_with_source(label, future, $crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn join_next(
                    &mut self,
                ) -> impl core::future::Future<
                    Output = Option<Result<T, $crate::tokio::task::JoinError>>,
                > + '_ {
                    self.join_next_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            #[cfg(not(target_arch = "wasm32"))]
            pub trait CommandExt {
                fn spawn(&mut self) -> std::io::Result<$crate::Child>;
                fn status(
                    &mut self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::ExitStatus>> + '_;
                fn output(
                    &mut self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::Output>> + '_;
            }

            #[cfg(not(target_arch = "wasm32"))]
            impl CommandExt for $crate::Command {
                #[track_caller]
                fn spawn(&mut self) -> std::io::Result<$crate::Child> {
                    self.spawn_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn status(
                    &mut self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::ExitStatus>> + '_ {
                    self.status_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn output(
                    &mut self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::Output>> + '_ {
                    self.output_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            #[cfg(not(target_arch = "wasm32"))]
            pub trait ChildExt {
                fn wait(
                    &mut self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::ExitStatus>> + '_;
                fn wait_with_output(
                    self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::Output>>;
            }

            #[cfg(not(target_arch = "wasm32"))]
            impl ChildExt for $crate::Child {
                #[track_caller]
                fn wait(
                    &mut self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::ExitStatus>> + '_ {
                    self.wait_with_source($crate::Source::caller(), PEEPS_CX)
                }

                #[track_caller]
                fn wait_with_output(
                    self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::Output>> {
                    self.wait_with_output_with_source($crate::Source::caller(), PEEPS_CX)
                }
            }

            pub mod prelude {
                pub use super::BroadcastReceiverExt;
                pub use super::BroadcastSenderExt;
                #[cfg(not(target_arch = "wasm32"))]
                pub use super::ChildExt;
                #[cfg(not(target_arch = "wasm32"))]
                pub use super::CommandExt;
                pub use super::JoinSetExt;
                pub use super::MutexExt;
                pub use super::NotifyExt;
                pub use super::OneshotReceiverExt;
                pub use super::OneshotSenderExt;
                pub use super::OnceCellExt;
                pub use super::ReceiverExt;
                pub use super::RwLockExt;
                pub use super::SemaphoreExt;
                pub use super::SenderExt;
                pub use super::UnboundedReceiverExt;
                pub use super::UnboundedSenderExt;
                pub use super::WatchReceiverExt;
                pub use super::WatchSenderExt;
            }
        }
    };
}
