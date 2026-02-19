use peeps_types::EntityBody;
use std::future::Future;
use std::time::Duration;

use super::super::futures::instrument_future_on;
use super::super::handles::EntityHandle;
use super::super::Source;

pub struct DiagnosticInterval {
    inner: tokio::time::Interval,
    handle: EntityHandle,
}

pub type Interval = DiagnosticInterval;

impl DiagnosticInterval {
    #[track_caller]
    pub fn tick(&mut self) -> impl Future<Output = tokio::time::Instant> + '_ {
        instrument_future_on(
            "interval.tick",
            &self.handle,
            self.inner.tick(),
            Source::caller(),
        )
    }

    #[track_caller]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[track_caller]
    pub fn period(&self) -> Duration {
        self.inner.period()
    }

    #[track_caller]
    pub fn set_missed_tick_behavior(&mut self, behavior: tokio::time::MissedTickBehavior) {
        self.inner.set_missed_tick_behavior(behavior);
    }
}

pub fn interval(period: Duration, source: Source) -> DiagnosticInterval {
    let label = format!("interval({}ms)", period.as_millis());
    DiagnosticInterval {
        inner: tokio::time::interval(period),
        handle: EntityHandle::new(label, EntityBody::Future, source),
    }
}

pub fn interval_at(
    start: tokio::time::Instant,
    period: Duration,
    source: Source,
) -> DiagnosticInterval {
    let label = format!("interval({}ms)", period.as_millis());
    DiagnosticInterval {
        inner: tokio::time::interval_at(start, period),
        handle: EntityHandle::new(label, EntityBody::Future, source),
    }
}
