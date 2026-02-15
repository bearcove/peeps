//! Task instrumentation for Tokio spawned tasks.
//!
//! When the `diagnostics` feature is enabled, wraps spawned tasks to capture
//! timing, poll events, and backtraces. When disabled, `spawn_tracked` is
//! a zero-cost wrapper around `tokio::spawn`.

use std::future::{Future, IntoFuture};

mod futures;
mod snapshot;
mod tasks;
mod wakes;

pub use peeps_types::{
    meta_key, FutureId, FuturePollEdgeSnapshot, FutureResumeEdgeSnapshot, FutureSpawnEdgeSnapshot,
    FutureWaitSnapshot, FutureWakeEdgeSnapshot, GraphSnapshot, IntoMetaValue, MetaBuilder,
    MetaValue, PollEvent, PollResult, TaskId, TaskSnapshot, TaskState, WakeEdgeSnapshot,
};

// ── Public API (delegates to modules) ────────────────────

/// Initialize the task tracking registry. No-op without `diagnostics`.
pub fn init_task_tracking() {
    tasks::init();
    wakes::init();
    futures::init();
}

/// Returns the current peeps task ID, if running inside a tracked task.
/// Returns `None` outside of a tracked task or without `diagnostics`.
pub fn current_task_id() -> Option<TaskId> {
    tasks::current_task_id()
}

/// Look up a task's name by ID. Returns `None` if not found or without `diagnostics`.
pub fn task_name(id: TaskId) -> Option<String> {
    tasks::task_name(id)
}

/// Spawn a tracked task with the given name.
///
/// With `diagnostics`: captures spawn backtrace and records poll events.
/// Without `diagnostics`: zero-cost wrapper around `tokio::spawn`.
#[track_caller]
pub fn spawn_tracked<F>(name: impl Into<String>, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tasks::spawn_tracked(name, future)
}

/// Collect snapshots of all tracked tasks. Empty without `diagnostics`.
pub fn snapshot_all_tasks() -> Vec<TaskSnapshot> {
    snapshot::snapshot_all_tasks()
}

/// Collect snapshots of wake/dependency edges between tasks.
pub fn snapshot_wake_edges() -> Vec<WakeEdgeSnapshot> {
    snapshot::snapshot_wake_edges()
}

/// Collect snapshots of wake/dependency edges from tasks to instrumented futures.
pub fn snapshot_future_wake_edges() -> Vec<FutureWakeEdgeSnapshot> {
    snapshot::snapshot_future_wake_edges()
}

/// Collect snapshots of annotated future wait states.
pub fn snapshot_future_waits() -> Vec<FutureWaitSnapshot> {
    snapshot::snapshot_future_waits()
}

/// Collect snapshots of future-to-future spawn/composition edges.
pub fn snapshot_future_spawn_edges() -> Vec<FutureSpawnEdgeSnapshot> {
    snapshot::snapshot_future_spawn_edges()
}

/// Collect snapshots of task-polls-future edges.
pub fn snapshot_future_poll_edges() -> Vec<FuturePollEdgeSnapshot> {
    snapshot::snapshot_future_poll_edges()
}

/// Collect snapshots of future-resumes-task edges.
pub fn snapshot_future_resume_edges() -> Vec<FutureResumeEdgeSnapshot> {
    snapshot::snapshot_future_resume_edges()
}

/// Emit canonical graph nodes and edges for tasks and futures.
pub fn emit_graph(proc_key: &str) -> GraphSnapshot {
    let process_name = peeps_types::process_name().unwrap_or(proc_key);
    snapshot::emit_graph(process_name, proc_key)
}

/// Wrapper future produced by [`peepable`] or [`PeepableFutureExt::peepable`].
pub struct PeepableFuture<F> {
    inner: futures::PeepableFuture<F>,
}

impl<F> Future for PeepableFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        #[allow(unsafe_code)]
        unsafe {
            let this = self.get_unchecked_mut();
            std::pin::Pin::new_unchecked(&mut this.inner).poll(cx)
        }
    }
}

/// Mark a future as an instrumented wait on a named resource.
pub fn peepable<F>(future: F, resource: impl Into<String>) -> PeepableFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    PeepableFuture {
        inner: futures::peepable(future.into_future(), resource),
    }
}

/// Mark a future as an instrumented wait with metadata.
pub fn peepable_with_meta<F, const N: usize>(
    future: F,
    resource: impl Into<String>,
    meta: MetaBuilder<'_, N>,
) -> PeepableFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    PeepableFuture {
        inner: futures::peepable_with_meta(future.into_future(), resource, meta),
    }
}

/// Build a `MetaBuilder` on the stack from key-value pairs.
///
/// ```ignore
/// peep_meta!("request.id" => MetaValue::U64(42), "request.method" => MetaValue::Static("get"))
/// ```
#[macro_export]
macro_rules! peep_meta {
    ($($k:literal => $v:expr),* $(,)?) => {{
        let mut mb = $crate::MetaBuilder::<16>::new();
        $(mb.push($k, $v);)*
        mb
    }};
}

/// Wrap a future with metadata, compiling away to bare future when diagnostics are disabled.
///
/// ```ignore
/// peepable_with_meta!(
///     stream.read(&mut buf),
///     "socket.read",
///     { "request.id" => MetaValue::U64(id) }
/// ).await?;
/// ```
#[cfg(feature = "diagnostics")]
#[macro_export]
macro_rules! peepable_with_meta {
    ($future:expr, $label:literal, {$($k:literal => $v:expr),* $(,)?}) => {{
        $crate::PeepableFutureExt::peepable_with_meta(
            $future,
            $label,
            $crate::peep_meta!($($k => $v),*),
        )
    }};
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! peepable_with_meta {
    ($future:expr, $label:literal, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
}

/// Wrap a future with auto-injected callsite context and optional custom metadata.
///
/// When `diagnostics` is disabled, expands to the bare future (zero cost).
///
/// ```ignore
/// // Label only (auto context injected):
/// peep!(stream.flush(), "socket.flush").await?;
///
/// // Label + custom keys:
/// peep!(stream.read(&mut buf), "socket.read", {
///     "resource.path" => path.as_str(),
///     "bytes" => buf.len(),
/// }).await?;
/// ```
#[cfg(feature = "diagnostics")]
#[macro_export]
macro_rules! peep {
    // With custom metadata keys
    ($future:expr, $label:literal, {$($k:literal => $v:expr),* $(,)?}) => {{
        let mut mb = $crate::MetaBuilder::<{
            // 6 auto context keys + user keys
            6 $(+ $crate::peep!(@count $k))*
        }>::new();
        mb.push(
            $crate::meta_key::CTX_MODULE_PATH,
            $crate::MetaValue::Static(module_path!()),
        );
        mb.push(
            $crate::meta_key::CTX_FILE,
            $crate::MetaValue::Static(file!()),
        );
        mb.push(
            $crate::meta_key::CTX_LINE,
            $crate::MetaValue::U64(line!() as u64),
        );
        mb.push(
            $crate::meta_key::CTX_CRATE_NAME,
            $crate::MetaValue::Static(env!("CARGO_PKG_NAME")),
        );
        mb.push(
            $crate::meta_key::CTX_CRATE_VERSION,
            $crate::MetaValue::Static(env!("CARGO_PKG_VERSION")),
        );
        mb.push(
            $crate::meta_key::CTX_CALLSITE,
            $crate::MetaValue::Static(concat!(
                $label, "@", file!(), ":", line!(), "::", module_path!()
            )),
        );
        $(mb.push($k, $crate::IntoMetaValue::into_meta_value($v));)*
        $crate::peepable_with_meta($future, $label, mb)
    }};
    // Label only (no custom keys)
    ($future:expr, $label:literal) => {{
        let mut mb = $crate::MetaBuilder::<6>::new();
        mb.push(
            $crate::meta_key::CTX_MODULE_PATH,
            $crate::MetaValue::Static(module_path!()),
        );
        mb.push(
            $crate::meta_key::CTX_FILE,
            $crate::MetaValue::Static(file!()),
        );
        mb.push(
            $crate::meta_key::CTX_LINE,
            $crate::MetaValue::U64(line!() as u64),
        );
        mb.push(
            $crate::meta_key::CTX_CRATE_NAME,
            $crate::MetaValue::Static(env!("CARGO_PKG_NAME")),
        );
        mb.push(
            $crate::meta_key::CTX_CRATE_VERSION,
            $crate::MetaValue::Static(env!("CARGO_PKG_VERSION")),
        );
        mb.push(
            $crate::meta_key::CTX_CALLSITE,
            $crate::MetaValue::Static(concat!(
                $label, "@", file!(), ":", line!(), "::", module_path!()
            )),
        );
        $crate::peepable_with_meta($future, $label, mb)
    }};
    // Internal: counting helper
    (@count $x:literal) => { 1usize };
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! peep {
    ($future:expr, $label:literal, {$($k:literal => $v:expr),* $(,)?}) => {{
        $future
    }};
    ($future:expr, $label:literal) => {{
        $future
    }};
}

pub trait PeepableFutureExt: IntoFuture + Sized {
    fn peepable(self, resource: impl Into<String>) -> PeepableFuture<Self::IntoFuture> {
        peepable(self, resource)
    }
    fn peepable_with_meta<const N: usize>(
        self,
        resource: impl Into<String>,
        meta: MetaBuilder<'_, N>,
    ) -> PeepableFuture<Self::IntoFuture> {
        peepable_with_meta(self, resource, meta)
    }
}

impl<F: IntoFuture> PeepableFutureExt for F {}

/// Remove completed tasks from the registry. No-op without `diagnostics`.
pub fn cleanup_completed_tasks() {
    snapshot::cleanup_completed_tasks()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── MetaBuilder acceptance / clamping / dropping ─────────

    #[test]
    fn meta_builder_accepts_valid_keys() {
        let mut mb = MetaBuilder::<16>::new();
        mb.push("request.id", MetaValue::U64(42));
        mb.push("request.method", MetaValue::Static("get_page"));
        mb.push("rpc.peer", MetaValue::Str("backend-1"));
        let json = mb.to_json_object();
        assert!(json.contains("\"request.id\":\"42\""));
        assert!(json.contains("\"request.method\":\"get_page\""));
        assert!(json.contains("\"rpc.peer\":\"backend-1\""));
    }

    #[test]
    fn meta_builder_drops_invalid_keys() {
        let mut mb = MetaBuilder::<16>::new();
        mb.push("UPPER", MetaValue::Static("nope"));
        mb.push("has space", MetaValue::Static("nope"));
        mb.push("has:colon", MetaValue::Static("nope"));
        mb.push("", MetaValue::Static("nope"));
        assert_eq!(mb.to_json_object(), "");
    }

    #[test]
    fn meta_builder_clamps_at_capacity() {
        let mut mb = MetaBuilder::<2>::new();
        mb.push("a", MetaValue::Static("1"));
        mb.push("b", MetaValue::Static("2"));
        mb.push("c", MetaValue::Static("3")); // dropped: over capacity
        let json = mb.to_json_object();
        assert!(json.contains("\"a\":\"1\""));
        assert!(json.contains("\"b\":\"2\""));
        assert!(!json.contains("\"c\""));
    }

    // ── peepable equivalence ────────────────────────────────

    #[test]
    fn peepable_and_peepable_with_empty_meta_are_equivalent() {
        // Both should produce a PeepableFuture wrapping the same inner future.
        // We verify that both compile and produce the same output.
        let fut_a = peepable(async { 42 }, "test.resource");
        let fut_b = peepable_with_meta(async { 42 }, "test.resource", MetaBuilder::<0>::new());

        // Both are PeepableFuture<impl Future<Output = i32>>
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let val_a = rt.block_on(fut_a);
        let val_b = rt.block_on(fut_b);
        assert_eq!(val_a, 42);
        assert_eq!(val_b, 42);
    }

    #[test]
    fn peepable_ext_trait_works() {
        let fut = async { 7 };
        let peepable_fut = fut.peepable("test.ext");
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        assert_eq!(rt.block_on(peepable_fut), 7);
    }

    #[test]
    fn peepable_ext_with_meta_works() {
        let meta = peep_meta!("request.id" => MetaValue::U64(99));
        let fut = async { "hello" };
        let peepable_fut = fut.peepable_with_meta("test.ext.meta", meta);
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        assert_eq!(rt.block_on(peepable_fut), "hello");
    }

    // ── peepable_with_meta! macro ───────────────────────────

    #[test]
    fn peepable_with_meta_macro_compiles() {
        let fut = peepable_with_meta!(
            async { 123 },
            "socket.read",
            {
                "request.id" => MetaValue::U64(55),
                "request.method" => MetaValue::Static("get"),
            }
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        assert_eq!(rt.block_on(fut), 123);
    }

    // ── Graph emission attrs_json.meta shape ────────────────

    #[cfg(feature = "diagnostics")]
    #[test]
    fn emit_graph_task_node_has_location_meta() {
        init_task_tracking();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let handle = spawn_tracked("test-task", async { 1 + 1 });
            let _ = handle.await;
        });

        let graph = emit_graph("test-proc-1");
        let task_node = graph
            .nodes
            .iter()
            .find(|n| n.kind == "task")
            .expect("should have a task node");

        assert!(task_node.id.starts_with("task:test-proc-1:"));
        let attrs = &task_node.attrs_json;
        assert!(attrs.contains("\"task_id\":"));
        assert!(attrs.contains("\"name\":\"test-task\""));
        assert!(attrs.contains("\"state\":"));
        assert!(attrs.contains("\"ctx.file\":"));
        assert!(attrs.contains("\"ctx.line\":"));
    }

    #[cfg(feature = "diagnostics")]
    #[test]
    fn emit_graph_future_node_has_required_attrs_and_meta() {
        init_task_tracking();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let notify = std::sync::Arc::new(tokio::sync::Notify::new());
            let notify2 = notify.clone();
            let meta = peep_meta!(
                "request.id" => MetaValue::U64(42),
                "request.method" => MetaValue::Static("get_page"),
            );
            let handle = spawn_tracked("meta-task", async move {
                let fut = peepable_with_meta(notify2.notified(), "test.resource", meta);
                fut.await
            });

            // Yield so the task gets polled (future becomes pending)
            tokio::task::yield_now().await;

            let graph = emit_graph("test-proc-2");
            let future_node = graph
                .nodes
                .iter()
                .find(|n| n.kind == "future")
                .expect("should have a future node");

            assert!(future_node.id.starts_with("future:"));

            let attrs = &future_node.attrs_json;
            assert!(attrs.contains("\"future_id\":"));
            assert!(attrs.contains("\"label\":\"test.resource\""));
            assert!(attrs.contains("\"pending_count\":"));
            assert!(attrs.contains("\"ready_count\":"));
            assert!(attrs.contains("\"meta\":{"));
            assert!(attrs.contains("\"request.id\":\"42\""));
            assert!(attrs.contains("\"request.method\":\"get_page\""));

            // Unblock the future so the task completes
            notify.notify_one();
            let _ = handle.await;
        });
    }

    #[cfg(feature = "diagnostics")]
    #[test]
    fn emit_graph_future_node_empty_meta() {
        init_task_tracking();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let notify = std::sync::Arc::new(tokio::sync::Notify::new());
            let notify2 = notify.clone();
            let handle = spawn_tracked("bare-task", async move {
                let fut = peepable(notify2.notified(), "bare.resource");
                fut.await
            });

            tokio::task::yield_now().await;

            let graph = emit_graph("test-proc-3");
            let future_node = graph
                .nodes
                .iter()
                .find(|n| {
                    n.kind == "future" && n.attrs_json.contains("\"label\":\"bare.resource\"")
                })
                .expect("should have a future node for bare.resource");

            assert!(future_node.attrs_json.contains("\"meta\":{}"));

            notify.notify_one();
            let _ = handle.await;
        });
    }

    #[cfg(feature = "diagnostics")]
    #[test]
    fn emit_graph_has_no_task_edges() {
        init_task_tracking();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let notify = std::sync::Arc::new(tokio::sync::Notify::new());
            let notify2 = notify.clone();
            let handle = spawn_tracked("edge-task", async move {
                let fut = peepable(notify2.notified(), "edge.test");
                fut.await
            });

            tokio::task::yield_now().await;

            let graph = emit_graph("test-proc-4");
            assert!(
                graph
                    .edges
                    .iter()
                    .all(|e| !e.src_id.starts_with("task:") && !e.dst_id.starts_with("task:")),
                "canonical graph should not include task-based edges"
            );

            notify.notify_one();
            let _ = handle.await;
        });
    }

    #[cfg(feature = "diagnostics")]
    #[test]
    fn dropped_future_removed_from_graph() {
        init_task_tracking();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let fut = peepable(async { "done" }, "ephemeral.resource");
            let _ = spawn_tracked("ephemeral-task", fut).await;

            let graph = emit_graph("test-proc-drop");
            let future_node = graph.nodes.iter().find(|n| {
                n.kind == "future" && n.attrs_json.contains("\"label\":\"ephemeral.resource\"")
            });

            assert!(
                future_node.is_none(),
                "dropped future should not appear in graph"
            );
        });
    }
}
