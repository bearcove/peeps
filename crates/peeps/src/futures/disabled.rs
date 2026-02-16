use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};

use peeps_types::GraphSnapshot;

// ── PeepableFuture (zero-cost wrapper) ───────────────────

pub struct PeepableFuture<F> {
    inner: F,
}

impl<F> Future for PeepableFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: we never move `inner` after pinning `Self`.
        #[allow(unsafe_code)]
        unsafe {
            let this = self.get_unchecked_mut();
            Pin::new_unchecked(&mut this.inner).poll(cx)
        }
    }
}

// ── Construction ─────────────────────────────────────────

#[inline]
pub fn peepable<F>(future: F, _resource: impl Into<String>) -> PeepableFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    PeepableFuture {
        inner: future.into_future(),
    }
}

#[inline]
pub fn peepable_with_meta<F, const N: usize>(
    future: F,
    _resource: impl Into<String>,
    _meta: peeps_types::MetaBuilder<'_, N>,
) -> PeepableFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    PeepableFuture {
        inner: future.into_future(),
    }
}

#[inline]
pub fn peepable_with_meta_kind<F, const N: usize>(
    future: F,
    _kind: peeps_types::NodeKind,
    _resource: impl Into<String>,
    _meta: peeps_types::MetaBuilder<'_, N>,
) -> PeepableFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    PeepableFuture {
        inner: future.into_future(),
    }
}

#[inline]
pub fn peepable_with_meta_kind_level<F, const N: usize>(
    future: F,
    _kind: peeps_types::NodeKind,
    _resource: impl Into<String>,
    _level: peeps_types::InstrumentationLevel,
    _meta: peeps_types::MetaBuilder<'_, N>,
) -> PeepableFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    PeepableFuture {
        inner: future.into_future(),
    }
}

// ── spawn_tracked ────────────────────────────────────────

#[inline]
pub fn spawn_tracked<F>(_name: impl Into<String>, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future)
}

// ── spawn_blocking_tracked ───────────────────────────────

#[inline]
pub fn spawn_blocking_tracked<F, T>(_name: impl Into<String>, f: F) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
}

// ── Wait helpers (zero-cost) ─────────────────────────────

#[inline]
pub fn timeout<F: Future>(
    duration: std::time::Duration,
    future: F,
    _label: impl Into<String>,
) -> PeepableFuture<tokio::time::Timeout<F>> {
    PeepableFuture {
        inner: tokio::time::timeout(duration, future),
    }
}

#[inline]
pub fn sleep(
    duration: std::time::Duration,
    _label: impl Into<String>,
) -> PeepableFuture<tokio::time::Sleep> {
    PeepableFuture {
        inner: tokio::time::sleep(duration),
    }
}

// ── Graph emission (no-op) ───────────────────────────────

#[inline(always)]
pub(crate) fn emit_into_graph(_graph: &mut GraphSnapshot) {}
