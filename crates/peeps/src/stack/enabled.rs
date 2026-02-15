use std::cell::RefCell;
use std::future::Future;

thread_local! {
    static STACK: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Push a node onto the thread-local stack.
///
/// Called by `PeepableFuture::poll` before polling the inner future.
pub(crate) fn push(node_id: &str) {
    STACK.with(|stack| {
        stack.borrow_mut().push(node_id.to_string());
    });
}

/// Pop the top node from the thread-local stack.
///
/// Called by `PeepableFuture::poll` after polling the inner future.
pub(crate) fn pop() {
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
}

/// Call `f` with the current top of the stack, if any.
///
/// Used by wrappers to emit canonical edges:
/// `stack::with_top(|src| registry::edge(src, resource_endpoint_id))`
///
/// No-op if the stack is empty.
pub fn with_top(f: impl FnOnce(&str)) {
    STACK.with(|stack| {
        let s = stack.borrow();
        if let Some(top) = s.last() {
            f(top);
        }
    });
}

/// Run `future` with the thread-local stack.
///
/// Kept for API compatibility. The stack is always available via thread-local
/// storage, so this just awaits the future directly.
pub async fn with_stack<F: Future>(future: F) -> F::Output {
    future.await
}
