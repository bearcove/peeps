// r[impl api.time]
use std::future::Future;
use std::time::Duration;

use moire_types::EntityBody;
use moire_runtime::{instrument_future, EntityHandle};

use super::capture_backtrace_id;

/// Instrumented equivalent of [`tokio::time::sleep`].
pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    let source = capture_backtrace_id();
    let handle = EntityHandle::new(
        "time.sleep",
        EntityBody::Future(moire_types::FutureEntity {}),
        source,
    );

    instrument_future(
        "time.sleep",
        tokio::time::sleep(duration),
        source,
        Some(handle.entity_ref()),
        None,
    )
}

/// Instrumented equivalent of [`tokio::time::Interval`].
pub struct Interval {
    inner: tokio::time::Interval,
    handle: EntityHandle,
}

impl Interval {
    /// Creates an instrumented interval, matching [`tokio::time::interval`].
    pub fn new(period: Duration) -> Self {
        let source = capture_backtrace_id();
        Self {
            inner: tokio::time::interval(period),
            handle: EntityHandle::new(
                "time.interval",
                EntityBody::Future(moire_types::FutureEntity {}),
                source,
            ),
        }
    }

    /// Waits for the next tick, equivalent to [`tokio::time::Interval::tick`].
    pub fn tick(&mut self) -> impl Future<Output = tokio::time::Instant> + '_ {
        let source = capture_backtrace_id();
        instrument_future(
            "time.interval.tick",
            self.inner.tick(),
            source,
            Some(self.handle.entity_ref()),
            None,
        )
    }
}
