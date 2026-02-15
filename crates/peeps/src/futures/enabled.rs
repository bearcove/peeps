use std::collections::HashMap;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::sync::{LazyLock, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use peeps_types::{GraphSnapshot, Node, NodeKind};

// ── Storage ──────────────────────────────────────────────

static FUTURE_WAIT_REGISTRY: LazyLock<Mutex<HashMap<String, FutureWaitInfo>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static FUTURE_SPAWN_EDGE_REGISTRY: LazyLock<Mutex<Vec<FutureSpawnEdge>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

struct FutureWaitInfo {
    resource: String,
    created_at: Instant,
    pending_count: u64,
    ready_count: u64,
    total_pending: Duration,
    last_seen: Instant,
    meta_json: String,
}

struct FutureSpawnEdge {
    parent_node_id: String,
    child_node_id: String,
    created_at: Instant,
}

// ── Registration ─────────────────────────────────────────

fn register_future(node_id: String, resource: String, meta_json: String) {
    FUTURE_WAIT_REGISTRY.lock().unwrap().insert(
        node_id,
        FutureWaitInfo {
            resource,
            created_at: Instant::now(),
            pending_count: 0,
            ready_count: 0,
            total_pending: Duration::ZERO,
            last_seen: Instant::now(),
            meta_json,
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

fn record_spawn_edge(parent_node_id: &str, child_node_id: &str) {
    FUTURE_SPAWN_EDGE_REGISTRY
        .lock()
        .unwrap()
        .push(FutureSpawnEdge {
            parent_node_id: parent_node_id.to_string(),
            child_node_id: child_node_id.to_string(),
            created_at: Instant::now(),
        });
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
        // Clean up any await edge to this future.
        if let Some(prev) = self.await_edge_src.take() {
            crate::registry::remove_edge(&prev, &self.node_id);
        }
        // Clean up spawn edges referencing this future as child.
        FUTURE_SPAWN_EDGE_REGISTRY
            .lock()
            .unwrap()
            .retain(|e| e.child_node_id != self.node_id);
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

fn inject_location_meta_json(meta_json: String, location: &str) -> String {
    let mut out = String::with_capacity(meta_json.len() + location.len() + 24);
    out.push('{');
    out.push('"');
    peeps_types::json_escape_into(&mut out, peeps_types::meta_key::CTX_LOCATION);
    out.push_str("\":\"");
    peeps_types::json_escape_into(&mut out, location);
    out.push('"');
    if !meta_json.is_empty() {
        // meta_json is a JSON object string like {"k":"v"}; splice its contents after our entry.
        if meta_json.starts_with('{') && meta_json.ends_with('}') && meta_json.len() > 2 {
            out.push(',');
            out.push_str(&meta_json[1..meta_json.len() - 1]);
        }
    }
    out.push('}');
    out
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
    let location = format!("{}:{}", caller.file(), caller.line());
    let meta_json = inject_location_meta_json(meta.to_json_object(), &location);

    register_future(node_id.clone(), resource.clone(), meta_json);

    // If created during another PeepableFuture's poll, record a spawn edge.
    let child_id = node_id.clone();
    crate::stack::with_top(|parent_node_id| {
        record_spawn_edge(parent_node_id, &child_id);
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
/// The `name` parameter is accepted for API compatibility but does not
/// create task nodes (tasks are not part of the canonical graph model).
#[track_caller]
pub fn spawn_tracked<F>(_name: impl Into<String>, future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Propagate the current top-of-stack into the spawned task, so work that
    // is logically a continuation of a request/response keeps a causal parent.
    let parent = crate::stack::capture_top();
    tokio::spawn(async move {
        if let Some(parent) = parent {
            let fut = crate::stack::scope(&parent, future);
            crate::stack::with_stack(fut).await
        } else {
            crate::stack::with_stack(future).await
        }
    })
}

// ── Graph emission ───────────────────────────────────────

/// Emit future nodes into the canonical graph.
///
/// Called by `registry::emit_graph()` to include future diagnostics.
pub(crate) fn emit_into_graph(graph: &mut GraphSnapshot) {
    let registry = FUTURE_WAIT_REGISTRY.lock().unwrap();

    for (node_id, info) in registry.iter() {
        let total_pending_ns = info.total_pending.as_nanos() as u64;

        let mut attrs = String::with_capacity(256);
        attrs.push('{');
        write_json_kv_str(&mut attrs, "label", &info.resource, true);
        write_json_kv_u64(&mut attrs, "pending_count", info.pending_count, false);
        write_json_kv_u64(&mut attrs, "ready_count", info.ready_count, false);
        if total_pending_ns > 0 {
            write_json_kv_u64(&mut attrs, "total_pending_ns", total_pending_ns, false);
        }
        attrs.push_str(",\"meta\":");
        if info.meta_json.is_empty() {
            attrs.push_str("{}");
        } else {
            attrs.push_str(&info.meta_json);
        }
        attrs.push('}');

        graph.nodes.push(Node {
            id: node_id.clone(),
            kind: NodeKind::Future,
            label: Some(info.resource.clone()),
            attrs_json: attrs,
        });
    }
}

// ── JSON helpers ─────────────────────────────────────────

fn write_json_kv_str(out: &mut String, key: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":\"");
    peeps_types::json_escape_into(out, value);
    out.push('"');
}

fn write_json_kv_u64(out: &mut String, key: &str, value: u64, first: bool) {
    use std::io::Write;
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    let mut buf = [0u8; 20];
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = write!(cursor, "{value}");
    let len = cursor.position() as usize;
    out.push_str(std::str::from_utf8(&buf[..len]).unwrap_or("0"));
}
