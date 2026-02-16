use std::collections::HashMap;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::sync::{LazyLock, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use facet::Facet;
use facet_json::RawJson;
use peeps_types::{GraphSnapshot, Node, NodeKind};

// ── Storage ──────────────────────────────────────────────

static FUTURE_WAIT_REGISTRY: LazyLock<Mutex<HashMap<String, FutureWaitInfo>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct FutureWaitInfo {
    resource: String,
    created_at: Instant,
    pending_count: u64,
    ready_count: u64,
    total_pending: Duration,
    last_seen: Instant,
    location: String,
    user_meta_json: String,
}

// ── Registration ─────────────────────────────────────────

fn register_future(node_id: String, resource: String, location: String, user_meta_json: String) {
    FUTURE_WAIT_REGISTRY.lock().unwrap().insert(
        node_id,
        FutureWaitInfo {
            resource,
            created_at: Instant::now(),
            pending_count: 0,
            ready_count: 0,
            total_pending: Duration::ZERO,
            last_seen: Instant::now(),
            location,
            user_meta_json,
        },
    );
}

fn unregister_future(node_id: &str) {
    FUTURE_WAIT_REGISTRY.lock().unwrap().remove(node_id);
}

fn record_pending(node_id: &str) {
    if let Some(info) = FUTURE_WAIT_REGISTRY.lock().unwrap().get_mut(node_id) {
        info.pending_count += 1;
        info.last_seen = Instant::now();
    }
}

fn record_ready(node_id: &str, pending_duration: Duration) {
    if let Some(info) = FUTURE_WAIT_REGISTRY.lock().unwrap().get_mut(node_id) {
        info.ready_count += 1;
        info.total_pending += pending_duration;
        info.last_seen = Instant::now();
    }
}

// ── PeepableFuture ───────────────────────────────────────

pub struct PeepableFuture<F> {
    node_id: String,
    #[allow(dead_code)]
    resource: String,
    inner: F,
    pending_since: Option<Instant>,
    // If we're being polled from within another peepable frame, we record
    // a canonical "await" edge: parent --needs--> this future (only while pending).
    await_edge_src: Option<String>,
}

impl<F> Future for PeepableFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: we never move fields out of `self` after pinning.
        #[allow(unsafe_code)]
        let this = unsafe { self.get_unchecked_mut() };
        // SAFETY: `inner` is pinned together with `self`.
        #[allow(unsafe_code)]
        let inner = unsafe { Pin::new_unchecked(&mut this.inner) };

        // Capture parent before pushing ourselves.
        let mut parent: Option<String> = None;
        crate::stack::with_top(|p| parent = Some(p.to_string()));

        // Push onto the async node stack before polling.
        crate::stack::push(&this.node_id);

        let result = inner.poll(cx);

        // Pop from the stack after polling.
        crate::stack::pop();

        match result {
            Poll::Pending => {
                // Only emit the parent->child "await" edge while we're actually pending.
                if let Some(parent_id) = parent {
                    if this.await_edge_src.as_deref() != Some(parent_id.as_str()) {
                        if let Some(prev) = this.await_edge_src.take() {
                            crate::registry::remove_edge(&prev, &this.node_id);
                        }
                        crate::registry::edge(&parent_id, &this.node_id);
                        this.await_edge_src = Some(parent_id);
                    }
                }
                if this.pending_since.is_none() {
                    this.pending_since = Some(Instant::now());
                }
                record_pending(&this.node_id);
                Poll::Pending
            }
            Poll::Ready(value) => {
                if let Some(prev) = this.await_edge_src.take() {
                    crate::registry::remove_edge(&prev, &this.node_id);
                }
                let pending_duration = this
                    .pending_since
                    .take()
                    .map(|t| t.elapsed())
                    .unwrap_or_default();
                record_ready(&this.node_id, pending_duration);
                Poll::Ready(value)
            }
        }
    }
}

impl<F> Drop for PeepableFuture<F> {
    fn drop(&mut self) {
        unregister_future(&self.node_id);
        // Clean up any canonical edges this future emitted.
        crate::registry::remove_edges_from(&self.node_id);
        // Clean up any touch edges from this future.
        crate::registry::remove_touch_edges_from(&self.node_id);
        // Clean up any await edge to this future.
        if let Some(prev) = self.await_edge_src.take() {
            crate::registry::remove_edge(&prev, &self.node_id);
        }
        // Clean up spawn edges referencing this future as child.
        crate::registry::remove_spawn_edges_to(&self.node_id);
    }
}

// ── Construction ─────────────────────────────────────────

#[track_caller]
pub fn peepable<F>(future: F, resource: impl Into<String>) -> PeepableFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    peepable_with_meta(future, resource, peeps_types::MetaBuilder::<0>::new())
}

#[track_caller]
pub fn peepable_with_meta<F, const N: usize>(
    future: F,
    resource: impl Into<String>,
    meta: peeps_types::MetaBuilder<'_, N>,
) -> PeepableFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let node_id = peeps_types::new_node_id("future");
    let resource = resource.into();
    let caller = std::panic::Location::caller();
    let location = crate::caller_location(caller);
    let user_meta_json = meta.to_json_object();

    register_future(node_id.clone(), resource.clone(), location, user_meta_json);

    // If created during another PeepableFuture's poll, record a spawned edge.
    let child_id = node_id.clone();
    crate::stack::with_top(|parent_node_id| {
        crate::registry::spawn_edge(parent_node_id, &child_id);
    });

    PeepableFuture {
        node_id,
        resource,
        inner: future.into_future(),
        pending_since: None,
        await_edge_src: None,
    }
}

// ── spawn_tracked ────────────────────────────────────────

/// Spawn a future with a task-local stack for canonical edge tracking.
///
/// The `name` parameter is used for diagnostics when no parent context exists.
/// Tasks are not part of the canonical graph model; only the futures they
/// contain appear as nodes.
#[track_caller]
pub fn spawn_tracked<F>(name: impl Into<String>, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Propagate the current top-of-stack into the spawned task, so work that
    // is logically a continuation of a request/response keeps a causal parent.
    let parent = crate::stack::capture_top();
    if parent.is_none() {
        let name = name.into();
        let caller = std::panic::Location::caller();
        tracing::debug!(
            task = %name,
            location = %caller,
            "spawn_tracked: no parent context, spawned task will have empty stack"
        );
    }
    tokio::spawn(async move {
        if let Some(parent) = parent {
            let fut = crate::stack::scope(&parent, future);
            crate::stack::ensure(fut).await
        } else {
            crate::stack::ensure(future).await
        }
    })
}

// ── spawn_blocking_tracked ───────────────────────────────

/// Spawn a blocking closure on the tokio blocking threadpool with context tracking.
///
/// Captures the current stack context before spawning, registers a future node
/// for the blocking task, and emits edges (spawned + touch) from the parent.
/// The node is cleaned up when the blocking closure completes.
///
/// Unlike `spawn_tracked`, the blocking closure cannot participate in the
/// async task-local stack — this only provides lineage tracking.
#[track_caller]
pub fn spawn_blocking_tracked<F, T>(
    name: impl Into<String>,
    f: F,
) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let name = name.into();
    let node_id = peeps_types::new_node_id("future");
    let caller = std::panic::Location::caller();
    let location = crate::caller_location(caller);

    let user_meta = BlockingUserMeta {
        blocking: "true".to_string(),
    };
    let user_meta_json = facet_json::to_string(&user_meta).unwrap();

    register_future(node_id.clone(), name, location, user_meta_json);

    // Emit edges from parent context.
    let child_id = node_id.clone();
    crate::stack::with_top(|parent_node_id| {
        crate::registry::spawn_edge(parent_node_id, &child_id);
        crate::registry::touch_edge(parent_node_id, &child_id);
    });

    let cleanup_id = node_id;
    tokio::task::spawn_blocking(move || {
        let result = f();
        // Clean up node — registry operations are thread-safe (std::sync::Mutex).
        unregister_future(&cleanup_id);
        crate::registry::remove_spawn_edges_to(&cleanup_id);
        crate::registry::remove_touch_edges_to(&cleanup_id);
        result
    })
}

// ── Wait helpers ─────────────────────────────────────────

/// Wrap `tokio::time::timeout` with instrumentation.
///
/// Creates a `PeepableFuture` node labeled `timeout:{label}`, making
/// the timeout visible in the graph as a child of the current stack frame.
///
/// ```ignore
/// let result = peeps::timeout(Duration::from_secs(5), rpc_call(), "rpc").await;
/// ```
#[track_caller]
pub fn timeout<F: Future>(
    duration: std::time::Duration,
    future: F,
    label: impl Into<String>,
) -> PeepableFuture<tokio::time::Timeout<F>> {
    let label = format!("timeout:{}", label.into());
    let node_id = peeps_types::new_node_id("future");
    let caller = std::panic::Location::caller();
    let location = crate::caller_location(caller);

    let user_meta = TimeoutUserMeta {
        timeout_duration_ms: format!("{}", duration.as_millis()),
    };
    let user_meta_json = facet_json::to_string(&user_meta).unwrap();

    register_future(node_id.clone(), label.clone(), location, user_meta_json);

    let child_id = node_id.clone();
    crate::stack::with_top(|parent_node_id| {
        crate::registry::spawn_edge(parent_node_id, &child_id);
    });

    PeepableFuture {
        node_id,
        resource: label,
        inner: tokio::time::timeout(duration, future),
        pending_since: None,
        await_edge_src: None,
    }
}

/// Wrap `tokio::time::sleep` with instrumentation.
///
/// Creates a `PeepableFuture` node labeled `sleep:{label}`, making
/// the sleep visible in the graph as a child of the current stack frame.
///
/// ```ignore
/// peeps::sleep(Duration::from_secs(30), "heartbeat_interval").await;
/// ```
#[track_caller]
pub fn sleep(
    duration: std::time::Duration,
    label: impl Into<String>,
) -> PeepableFuture<tokio::time::Sleep> {
    let label = format!("sleep:{}", label.into());
    let node_id = peeps_types::new_node_id("future");
    let caller = std::panic::Location::caller();
    let location = crate::caller_location(caller);

    let user_meta = SleepUserMeta {
        sleep_duration_ms: format!("{}", duration.as_millis()),
    };
    let user_meta_json = facet_json::to_string(&user_meta).unwrap();

    register_future(node_id.clone(), label.clone(), location, user_meta_json);

    let child_id = node_id.clone();
    crate::stack::with_top(|parent_node_id| {
        crate::registry::spawn_edge(parent_node_id, &child_id);
    });

    PeepableFuture {
        node_id,
        resource: label,
        inner: tokio::time::sleep(duration),
        pending_since: None,
        await_edge_src: None,
    }
}

// ── Attrs structs ────────────────────────────────────────

#[derive(Facet)]
struct FutureAttrs<'a> {
    label: &'a str,
    pending_count: u64,
    ready_count: u64,
    #[facet(skip_unless_truthy)]
    total_pending_ns: Option<u64>,
    /// Nanoseconds since the future was created.
    age_ns: u64,
    /// Nanoseconds since the future was last polled.
    idle_ns: u64,
    #[facet(rename = "ctx.location")]
    ctx_location: &'a str,
    meta: RawJson<'a>,
}

#[derive(Facet)]
struct TimeoutUserMeta {
    #[facet(rename = "timeout.duration_ms")]
    timeout_duration_ms: String,
}

#[derive(Facet)]
struct SleepUserMeta {
    #[facet(rename = "sleep.duration_ms")]
    sleep_duration_ms: String,
}

#[derive(Facet)]
struct BlockingUserMeta {
    blocking: String,
}

// ── Graph emission ───────────────────────────────────────

/// Emit future nodes into the canonical graph.
///
/// Called by `registry::emit_graph()` to include future diagnostics.
pub(crate) fn emit_into_graph(graph: &mut GraphSnapshot) {
    let registry = FUTURE_WAIT_REGISTRY.lock().unwrap();

    let now = Instant::now();
    for (node_id, info) in registry.iter() {
        let total_pending_ns = info.total_pending.as_nanos() as u64;
        let age_ns = now.duration_since(info.created_at).as_nanos() as u64;
        let idle_ns = now.duration_since(info.last_seen).as_nanos() as u64;

        let meta_str = if info.user_meta_json.is_empty() {
            "{}"
        } else {
            &info.user_meta_json
        };

        let attrs = FutureAttrs {
            label: &info.resource,
            pending_count: info.pending_count,
            ready_count: info.ready_count,
            total_pending_ns: if total_pending_ns > 0 {
                Some(total_pending_ns)
            } else {
                None
            },
            age_ns,
            idle_ns,
            ctx_location: &info.location,
            meta: RawJson::new(meta_str),
        };

        graph.nodes.push(Node {
            id: node_id.clone(),
            kind: NodeKind::Future,
            label: Some(info.resource.clone()),
            attrs_json: facet_json::to_string(&attrs).unwrap(),
        });
    }
}
