pub(crate) mod channels;
pub(crate) mod joinset;
pub(crate) mod process;
pub(crate) mod rpc;
pub(crate) mod sync;

pub use self::channels::*;
pub use self::process::*;
pub use self::rpc::*;
pub use self::sync::*;
pub use peeps_runtime::*;

// facade! expands to a call to this
#[doc(hidden)]
pub fn __init_from_macro() {
    peeps_runtime::init_runtime_from_macro();
}

#[macro_export]
macro_rules! facade {
    () => {

        pub mod peeps {
            pub const PEEPS_SOURCE_LEFT: $crate::SourceLeft =
                $crate::SourceLeft::new(env!("CARGO_MANIFEST_DIR"), env!("CARGO_PKG_NAME"));

            #[track_caller]
            fn __source() -> $crate::SourceId {
                $crate::__init_from_macro();
                PEEPS_SOURCE_LEFT.resolve().into()
            }

            #[track_caller]
            pub fn mutex<T>(name: &'static str, value: T) -> $crate::Mutex<T> {
                $crate::Mutex::new_with_source(name, value, __source())
            }

            #[track_caller]
            pub fn rwlock<T>(name: &'static str, value: T) -> $crate::RwLock<T> {
                $crate::RwLock::new_with_source(name, value, __source())
            }

            #[track_caller]
            pub fn notify(name: impl Into<String>) -> $crate::Notify {
                $crate::Notify::new_with_source(name, __source())
            }

            #[track_caller]
            pub fn once_cell<T>(name: impl Into<String>) -> $crate::OnceCell<T> {
                $crate::OnceCell::new_with_source(name, __source())
            }

            #[track_caller]
            pub fn semaphore(name: impl Into<String>, permits: usize) -> $crate::Semaphore {
                $crate::Semaphore::new_with_source(name, permits, __source())
            }

            #[track_caller]
            pub fn channel<T>(
                name: impl Into<String>,
                capacity: usize,
            ) -> ($crate::Sender<T>, $crate::Receiver<T>) {
                $crate::channel(name, capacity, __source())
            }

            #[track_caller]
            pub fn unbounded_channel<T>(
                name: impl Into<String>,
            ) -> ($crate::UnboundedSender<T>, $crate::UnboundedReceiver<T>) {
                $crate::unbounded_channel(name, __source())
            }

            #[track_caller]
            pub fn oneshot<T>(
                name: impl Into<String>,
            ) -> ($crate::OneshotSender<T>, $crate::OneshotReceiver<T>) {
                $crate::oneshot(name, __source())
            }

            #[track_caller]
            pub fn spawn_tracked<F>(
                name: impl Into<String>,
                fut: F,
            ) -> $crate::tokio::task::JoinHandle<F::Output>
            where
                F: core::future::Future + Send + 'static,
                F::Output: Send + 'static,
            {
                $crate::spawn_tracked(name, fut, __source())
            }

            #[track_caller]
            pub fn instrument_future<F>(
                name: impl Into<String>,
                fut: F,
                on: Option<$crate::EntityRef>,
                meta: Option<$crate::facet_value::Value>,
            ) -> $crate::InstrumentedFuture<F::IntoFuture>
            where
                F: core::future::IntoFuture,
            {
                $crate::instrument_future(name, fut, __source(), on, meta)
            }

            pub trait FutureExt: core::future::Future + Sized {
                fn tracked(
                    self,
                    name: impl Into<String>,
                ) -> $crate::InstrumentedFuture<Self::IntoFuture>
                where
                    Self: core::future::IntoFuture;
            }

            impl<F> FutureExt for F
            where
                F: core::future::Future + Sized,
            {
                #[track_caller]
                fn tracked(
                    self,
                    name: impl Into<String>,
                ) -> $crate::InstrumentedFuture<Self::IntoFuture>
                where
                    Self: core::future::IntoFuture,
                {
                    $crate::instrument_future(name, self, __source(), None, None)
                }
            }

            pub trait MutexExt<T> {
                fn lock(&self) -> $crate::MutexGuard<'_, T>;
                fn try_lock(&self) -> Option<$crate::MutexGuard<'_, T>>;
            }

            impl<T> MutexExt<T> for $crate::Mutex<T> {
                #[track_caller]
                fn lock(&self) -> $crate::MutexGuard<'_, T> {
                    self.lock_with_source(__source())
                }

                #[track_caller]
                fn try_lock(&self) -> Option<$crate::MutexGuard<'_, T>> {
                    self.try_lock_with_source(__source())
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
                    self.read_with_source(__source())
                }

                #[track_caller]
                fn write(&self) -> $crate::parking_lot::RwLockWriteGuard<'_, T> {
                    self.write_with_source(__source())
                }

                #[track_caller]
                fn try_read(&self) -> Option<$crate::parking_lot::RwLockReadGuard<'_, T>> {
                    self.try_read_with_source(__source())
                }

                #[track_caller]
                fn try_write(&self) -> Option<$crate::parking_lot::RwLockWriteGuard<'_, T>> {
                    self.try_write_with_source(__source())
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
                    self.send_with_source(value, __source())
                }
            }

            pub trait ReceiverExt<T> {
                fn recv(&mut self) -> impl core::future::Future<Output = Option<T>> + '_;
            }

            impl<T> ReceiverExt<T> for $crate::Receiver<T> {
                #[track_caller]
                fn recv(&mut self) -> impl core::future::Future<Output = Option<T>> + '_ {
                    self.recv_with_source(__source())
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
                    self.send_with_source(value, __source())
                }
            }

            pub trait UnboundedReceiverExt<T> {
                fn recv(&mut self) -> impl core::future::Future<Output = Option<T>> + '_;
            }

            impl<T> UnboundedReceiverExt<T> for $crate::UnboundedReceiver<T> {
                #[track_caller]
                fn recv(&mut self) -> impl core::future::Future<Output = Option<T>> + '_ {
                    self.recv_with_source(__source())
                }
            }

            pub trait OneshotSenderExt<T> {
                fn send(self, value: T) -> Result<(), T>;
            }

            impl<T> OneshotSenderExt<T> for $crate::OneshotSender<T> {
                #[track_caller]
                fn send(self, value: T) -> Result<(), T> {
                    self.send_with_source(value, __source())
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
                    self.recv_with_source(__source())
                }
            }

            pub trait BroadcastSenderExt<T: Clone> {
                fn subscribe(&self) -> $crate::BroadcastReceiver<T>;
                fn send(
                    &self,
                    value: T,
                ) -> Result<usize, $crate::tokio::sync::broadcast::error::SendError<T>>;
            }

            impl<T: Clone> BroadcastSenderExt<T> for $crate::BroadcastSender<T> {
                #[track_caller]
                fn subscribe(&self) -> $crate::BroadcastReceiver<T> {
                    self.subscribe_with_source(__source())
                }

                #[track_caller]
                fn send(
                    &self,
                    value: T,
                ) -> Result<usize, $crate::tokio::sync::broadcast::error::SendError<T>> {
                    self.send_with_source(value, __source())
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
                    self.recv_with_source(__source())
                }
            }

            pub trait WatchSenderExt<T: Clone> {
                fn subscribe(&self) -> $crate::WatchReceiver<T>;
                fn send(
                    &self,
                    value: T,
                ) -> Result<(), $crate::tokio::sync::watch::error::SendError<T>>;
                fn send_replace(&self, value: T) -> T;
            }

            impl<T: Clone> WatchSenderExt<T> for $crate::WatchSender<T> {
                #[track_caller]
                fn subscribe(&self) -> $crate::WatchReceiver<T> {
                    self.subscribe_with_source(__source())
                }

                #[track_caller]
                fn send(
                    &self,
                    value: T,
                ) -> Result<(), $crate::tokio::sync::watch::error::SendError<T>> {
                    self.send_with_source(value, __source())
                }

                #[track_caller]
                fn send_replace(&self, value: T) -> T {
                    self.send_replace_with_source(value, __source())
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
                    self.changed_with_source(__source())
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
                    self.notified_with_source(__source())
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
                    E: 'a,
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
                    self.get_or_init_with_source(f, __source())
                }

                #[track_caller]
                fn get_or_try_init<'a, F, Fut, E>(
                    &'a self,
                    f: F,
                ) -> impl core::future::Future<Output = Result<&'a T, E>> + 'a
                where
                    T: 'a,
                    E: 'a,
                    F: FnOnce() -> Fut + 'a,
                    Fut: core::future::Future<Output = Result<T, E>> + 'a,
                {
                    self.get_or_try_init_with_source(f, __source())
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
                    self.acquire_with_source(__source())
                }

                #[track_caller]
                fn acquire_many(
                    &self,
                    n: u32,
                ) -> impl core::future::Future<
                    Output = Result<$crate::SemaphorePermit<'_>, $crate::tokio::sync::AcquireError>,
                > + '_ {
                    self.acquire_many_with_source(n, __source())
                }

                #[track_caller]
                fn acquire_owned(
                    &self,
                ) -> impl core::future::Future<
                    Output = Result<$crate::OwnedSemaphorePermit, $crate::tokio::sync::AcquireError>,
                > + '_ {
                    self.acquire_owned_with_source(__source())
                }

                #[track_caller]
                fn acquire_many_owned(
                    &self,
                    n: u32,
                ) -> impl core::future::Future<
                    Output = Result<$crate::OwnedSemaphorePermit, $crate::tokio::sync::AcquireError>,
                > + '_ {
                    self.acquire_many_owned_with_source(n, __source())
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
                    self.spawn_with_source(label, future, __source())
                }

                #[track_caller]
                fn join_next(
                    &mut self,
                ) -> impl core::future::Future<
                    Output = Option<Result<T, $crate::tokio::task::JoinError>>,
                > + '_ {
                    self.join_next_with_source(__source())
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
                    self.spawn_with_source(__source())
                }

                #[track_caller]
                fn status(
                    &mut self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::ExitStatus>> + '_ {
                    self.status_with_source(__source())
                }

                #[track_caller]
                fn output(
                    &mut self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::Output>> + '_ {
                    self.output_with_source(__source())
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
                    self.wait_with_source(__source())
                }

                #[track_caller]
                fn wait_with_output(
                    self,
                ) -> impl core::future::Future<Output = std::io::Result<std::process::Output>> {
                    self.wait_with_output_with_source(__source())
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
                pub use super::FutureExt;
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
