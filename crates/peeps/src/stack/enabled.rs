use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

tokio::task_local! {
    static STACK: RefCell<Vec<String>>;
}

/// Returns true if a stack scope is active in this task.
pub fn is_active() -> bool {
    STACK.try_with(|_| ()).is_ok()
}

/// Returns the current top-of-stack node id, if any.
pub fn capture_top() -> Option<String> {
    STACK
        .try_with(|stack| stack.borrow().last().cloned())
        .ok()
        .flatten()
}

/// Push a node onto the task-local stack.
///
/// Called by `PeepableFuture::poll` before polling the inner future.
/// No-op if called outside a stack scope.
pub(crate) fn push(node_id: &str) {
    let _ = STACK.try_with(|stack| {
        stack.borrow_mut().push(node_id.to_string());
    });
}

/// Pop the top node from the task-local stack.
///
/// Called by `PeepableFuture::poll` after polling the inner future.
/// No-op if called outside a stack scope.
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
/// No-op if the stack is empty or if called outside a stack scope.
pub fn with_top(f: impl FnOnce(&str)) {
    let _ = STACK.try_with(|stack| {
        let s = stack.borrow();
        if let Some(top) = s.last() {
            f(top);
        }
    });
}

/// Run `future` with a fresh task-local stack scope.
///
/// Use this at async entrypoints where we want canonical edge emission
/// to work even if the caller wasn't spawned via `spawn_tracked`.
pub async fn with_stack<F: Future>(future: F) -> F::Output {
    STACK.scope(RefCell::new(Vec::new()), future).await
}

/// A small future wrapper that pushes a stable node id onto the stack
/// for the duration of each `poll()` call of `inner`.
pub struct Scoped<F> {
    node_id: String,
    inner: F,
}

/// Wrap `future` so `node_id` is on the top-of-stack while it is being polled.
///
/// If there is no active stack scope, this becomes a no-op wrapper (push/pop are no-ops).
pub fn scope<F: Future>(node_id: &str, future: F) -> Scoped<F> {
    Scoped {
        node_id: node_id.to_string(),
        inner: future,
    }
}

impl<F: Future> Future for Scoped<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: `inner` is structurally pinned and we never move it.
        #[allow(unsafe_code)]
        let this = unsafe { self.get_unchecked_mut() };
        #[allow(unsafe_code)]
        let inner = unsafe { Pin::new_unchecked(&mut this.inner) };

        push(&this.node_id);
        let out = inner.poll(cx);
        pop();
        out
    }
}
