use std::future::IntoFuture;

#[inline]
pub fn connect<F: IntoFuture>(future: F, _endpoint: &str, _transport: &str) -> F::IntoFuture {
    future.into_future()
}

#[inline]
pub fn accept<F: IntoFuture>(future: F, _endpoint: &str, _transport: &str) -> F::IntoFuture {
    future.into_future()
}

#[inline]
pub fn readable<F: IntoFuture>(future: F, _endpoint: &str, _transport: &str) -> F::IntoFuture {
    future.into_future()
}

#[inline]
pub fn writable<F: IntoFuture>(future: F, _endpoint: &str, _transport: &str) -> F::IntoFuture {
    future.into_future()
}
