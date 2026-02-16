use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::Instant;

use facet::Facet;
use peeps_types::{Node, NodeKind};

// ── Attrs structs ─────────────────────────────────────

#[derive(Facet)]
struct OnceCellAttrs<'a> {
    name: &'a str,
    state: &'a str,
    age_ns: u64,
    #[facet(skip_unless_truthy)]
    init_duration_ns: Option<u64>,
    meta: OnceCellMeta<'a>,
}

#[derive(Facet)]
struct OnceCellMeta<'a> {
    #[facet(rename = "ctx.location")]
    ctx_location: &'a str,
}

const ONCE_EMPTY: u8 = 0;
const ONCE_INITIALIZING: u8 = 1;
const ONCE_INITIALIZED: u8 = 2;

// ── Info type ───────────────────────────────────────────

pub(super) struct OnceCellInfo {
    pub(super) name: String,
    pub(super) node_id: String,
    pub(super) location: String,
    pub(super) state: AtomicU8,
    pub(super) created_at: Instant,
    pub(super) init_duration: Mutex<Option<std::time::Duration>>,
}

// ── Storage ─────────────────────────────────────────────

static ONCECELL_REGISTRY: LazyLock<Mutex<Vec<Weak<OnceCellInfo>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

fn prune_and_register_once_cell(info: &Arc<OnceCellInfo>) {
    let mut reg = ONCECELL_REGISTRY.lock().unwrap();
    reg.retain(|w| w.strong_count() > 0);
    reg.push(Arc::downgrade(info));
}

// ── OnceCell ────────────────────────────────────────────

pub struct OnceCell<T> {
    inner: tokio::sync::OnceCell<T>,
    info: Arc<OnceCellInfo>,
}

impl<T> OnceCell<T> {
    #[track_caller]
    pub fn new(name: impl Into<String>) -> Self {
        let caller = std::panic::Location::caller();
        let location = crate::caller_location(caller);
        let info = Arc::new(OnceCellInfo {
            name: name.into(),
            node_id: peeps_types::new_node_id("oncecell"),
            location,
            state: AtomicU8::new(ONCE_EMPTY),
            created_at: Instant::now(),
            init_duration: Mutex::new(None),
        });
        prune_and_register_once_cell(&info);
        Self {
            inner: tokio::sync::OnceCell::new(),
            info,
        }
    }

    pub fn get(&self) -> Option<&T> {
        self.inner.get()
    }

    pub fn initialized(&self) -> bool {
        self.inner.initialized()
    }

    pub async fn get_or_init<F, Fut>(&self, f: F) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        if self.inner.initialized() {
            return self.inner.get().unwrap();
        }

        self.info
            .state
            .compare_exchange(
                ONCE_EMPTY,
                ONCE_INITIALIZING,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .ok();
        let start = Instant::now();

        let mut edge_src: Option<String> = None;
        crate::stack::with_top(|src| {
            edge_src = Some(src.to_string());
            crate::registry::edge(src, &self.info.node_id);
        });

        let result = self.inner.get_or_init(f).await;

        if let Some(ref src) = edge_src {
            crate::registry::remove_edge(src, &self.info.node_id);
        }

        if self
            .info
            .state
            .compare_exchange(
                ONCE_INITIALIZING,
                ONCE_INITIALIZED,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            *self.info.init_duration.lock().unwrap() = Some(start.elapsed());
        }

        result
    }

    pub async fn get_or_try_init<F, Fut, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        if self.inner.initialized() {
            return Ok(self.inner.get().unwrap());
        }

        self.info
            .state
            .compare_exchange(
                ONCE_EMPTY,
                ONCE_INITIALIZING,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .ok();
        let start = Instant::now();

        let mut edge_src: Option<String> = None;
        crate::stack::with_top(|src| {
            edge_src = Some(src.to_string());
            crate::registry::edge(src, &self.info.node_id);
        });

        let result = self.inner.get_or_try_init(f).await;

        if let Some(ref src) = edge_src {
            crate::registry::remove_edge(src, &self.info.node_id);
        }

        if result.is_ok() {
            if self
                .info
                .state
                .compare_exchange(
                    ONCE_INITIALIZING,
                    ONCE_INITIALIZED,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                *self.info.init_duration.lock().unwrap() = Some(start.elapsed());
            }
        } else {
            // Failed init — revert to empty
            self.info
                .state
                .compare_exchange(
                    ONCE_INITIALIZING,
                    ONCE_EMPTY,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .ok();
        }

        result
    }

    pub fn set(&self, value: T) -> Result<(), T> {
        let start = Instant::now();
        self.info
            .state
            .compare_exchange(
                ONCE_EMPTY,
                ONCE_INITIALIZING,
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .ok();
        match self.inner.set(value) {
            Ok(()) => {
                self.info
                    .state
                    .store(ONCE_INITIALIZED, Ordering::Relaxed);
                *self.info.init_duration.lock().unwrap() = Some(start.elapsed());
                Ok(())
            }
            Err(e) => {
                // Already initialized, revert our state change
                self.info
                    .state
                    .compare_exchange(
                        ONCE_INITIALIZING,
                        ONCE_INITIALIZED,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .ok();
                match e {
                    tokio::sync::SetError::AlreadyInitializedError(v) => Err(v),
                    tokio::sync::SetError::InitializingError(v) => Err(v),
                }
            }
        }
    }
}

// ── Graph emission ──────────────────────────────────────

pub(super) fn emit_oncecell_nodes(graph: &mut peeps_types::GraphSnapshot) {
    let now = Instant::now();
    let reg = ONCECELL_REGISTRY.lock().unwrap();

    for info in reg.iter().filter_map(|w| w.upgrade()) {
        let name = &info.name;
        let age_ns = now.duration_since(info.created_at).as_nanos() as u64;
        let state_val = info.state.load(Ordering::Relaxed);
        let state_str = match state_val {
            ONCE_INITIALIZING => "initializing",
            ONCE_INITIALIZED => "initialized",
            _ => "empty",
        };
        let init_duration_ns = info
            .init_duration
            .lock()
            .unwrap()
            .map(|d| d.as_nanos() as u64);

        let attrs = OnceCellAttrs {
            name,
            state: state_str,
            age_ns,
            init_duration_ns,
            meta: OnceCellMeta {
                ctx_location: &info.location,
            },
        };

        graph.nodes.push(Node {
            id: info.node_id.clone(),
            kind: NodeKind::OnceCell,
            label: Some(name.clone()),
            attrs_json: facet_json::to_string(&attrs).unwrap(),
        });
    }
}
