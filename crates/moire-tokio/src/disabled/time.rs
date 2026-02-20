use std::future::Future;
use std::time::Duration;

pub use tokio::time::Instant;

pub fn sleep(duration: Duration) -> impl Future<Output = ()> {
    tokio::time::sleep(duration)
}

pub struct Interval(tokio::time::Interval);

impl Interval {
    pub fn tick(&mut self) -> impl Future<Output = tokio::time::Instant> + '_ {
        self.0.tick()
    }
}

pub fn interval(period: Duration) -> Interval {
    Interval(tokio::time::interval(period))
}

/// Run a future with a timeout. Returns `None` if the timeout fires first.
pub async fn timeout<F, T>(duration: Duration, future: F) -> Option<T>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(duration, future).await.ok()
}
