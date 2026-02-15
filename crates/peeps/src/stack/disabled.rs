use std::future::Future;

#[inline(always)]
pub(crate) fn push(_node_id: &str) {}

#[inline(always)]
pub(crate) fn pop() {}

#[inline(always)]
pub fn with_top(_f: impl FnOnce(&str)) {}

#[inline(always)]
pub fn is_active() -> bool {
    false
}

#[inline(always)]
pub fn capture_top() -> Option<String> {
    None
}

#[inline(always)]
pub async fn with_stack<F: Future>(future: F) -> F::Output {
    future.await
}

#[inline(always)]
pub fn scope<F: Future>(_node_id: &str, future: F) -> F {
    future
}
