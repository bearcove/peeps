use std::cell::RefCell;
use std::future::Future;

tokio::task_local! {
    static STACK: RefCell<Vec<String>>;
}

/// Push a node onto the task-local stack.
///
/// Called by `PeepableFuture::poll` before polling the inner future.
/// No-op if called outside a tracked task.
pub(crate) fn push(node_id: &str) {
    let _ = STACK.try_with(|stack| {
        stack.borrow_mut().push(node_id.to_string());
    });
}

/// Pop the top node from the task-local stack.
///
/// Called by `PeepableFuture::poll` after polling the inner future.
/// No-op if called outside a tracked task.
pub(crate) fn pop() {
    let _ = STACK.try_with(|stack| {
        stack.borrow_mut().pop();
    });
}

/// Call `f` with the current top of the stack, if any.
///
/// Used by wrappers to emit canonical edges:
/// `stack::with_top(|src| registry::edge(src, resource_endpoint_id))`
///
/// No-op if the stack is empty or if called outside a tracked task.
pub fn with_top(f: impl FnOnce(&str)) {
    let _ = STACK.try_with(|stack| {
        let s = stack.borrow();
        if let Some(top) = s.last() {
            f(top);
        }
    });
}

/// Run `future` with a fresh task-local stack.
///
/// Called by `spawn_tracked` to initialize the stack for each spawned task.
pub(crate) async fn with_stack<F: Future>(future: F) -> F::Output {
    STACK.scope(RefCell::new(Vec::new()), future).await
}
