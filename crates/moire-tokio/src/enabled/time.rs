// r[impl api.time]
//! Instrumented time utilities, mirroring [`tokio::time`].
//!
//! This module mirrors the structure of `tokio::time` and can be used as a
//! drop-in replacement. Sleeps and intervals are registered as named entities
//! in the Moiré runtime graph so the dashboard can show which tasks are
//! suspended waiting for a timer to fire.
//!
//! # Available items
//!
//! | Item | Tokio equivalent |
//! |---|---|
//! | [`sleep`] | `tokio::time::sleep` |
//! | [`timeout`] | `tokio::time::timeout` |
//! | [`interval`] | `tokio::time::interval` |
//! | [`Interval`] | `tokio::time::Interval` |
use std::future::Future;
use std::time::Duration;

use moire_runtime::EntityHandle;
use moire_types::FutureEntity;

use super::task::FutureExt as _;

/// Instrumented equivalent of [`tokio::time::sleep`].
pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    let handle = EntityHandle::new("time.sleep", FutureEntity {});
    tokio::time::sleep(duration)
        .named("time.sleep")
        .on(handle.entity_ref())
}

/// Instrumented equivalent of [`tokio::time::Interval`].
pub struct Interval {
    inner: tokio::time::Interval,
    handle: EntityHandle<FutureEntity>,
}

impl Interval {
    /// Waits for the next tick, equivalent to [`tokio::time::Interval::tick`].
    pub fn tick(&mut self) -> impl Future<Output = tokio::time::Instant> + '_ {
        self.inner
            .tick()
            .named("time.interval.tick")
            .on(self.handle.entity_ref())
    }
}

/// Creates an instrumented interval, matching [`tokio::time::interval`].
pub fn interval(period: Duration) -> Interval {
    Interval {
        inner: tokio::time::interval(period),
        handle: EntityHandle::new("time.interval", FutureEntity {}),
    }
}

/// Run a future with a timeout.
///
/// Equivalent to `tokio::time::timeout`.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Result<T, tokio::time::error::Elapsed>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future).await
}
