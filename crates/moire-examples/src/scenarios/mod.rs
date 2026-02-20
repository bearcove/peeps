pub mod channel_full_stall;
pub mod mutex_lock_order_inversion;
pub mod oneshot_sender_lost_in_map;
#[cfg(feature = "roam")]
pub mod roam_rpc_stuck_request;
#[cfg(feature = "roam")]
pub mod roam_rust_swift_stuck_request;
pub mod semaphore_starvation;

use std::future::Future;

use moire::task::{JoinHandle, TaskFutureExt};

pub fn spawn_tracked<T>(
    name: impl Into<String>,
    task: impl Future<Output = T> + Send + 'static,
) -> JoinHandle<T>
where
    T: Send + 'static,
{
    moire::task::spawn(task.named(name))
}
