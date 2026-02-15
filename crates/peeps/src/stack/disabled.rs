use std::future::Future;

#[inline(always)]
pub(crate) fn push(_node_id: &str) {}

#[inline(always)]
pub(crate) fn pop() {}

#[inline(always)]
pub fn with_top(_f: impl FnOnce(&str)) {}

#[inline(always)]
pub(crate) async fn with_stack<F: Future>(future: F) -> F::Output {
    future.await
}
