//! Shared diagnostic snapshot types for peeps.
//!
//! All snapshot types live here so they can be used for both serialization
//! (producing dumps) and deserialization (reading dumps) without circular
//! dependencies between peeps subcrates and instrumented libraries.

use std::sync::OnceLock;

use facet::Facet;

// ── Global process name ─────────────────────────────────────────

static PROCESS_NAME: OnceLock<String> = OnceLock::new();

/// Set the global process name for this process.
///
/// Should be called once at startup (e.g. from `peeps::init_named`).
/// Subsequent calls are ignored (first write wins).
pub fn set_process_name(name: impl Into<String>) {
    let _ = PROCESS_NAME.set(name.into());
}

/// Get the global process name, if set.
pub fn process_name() -> Option<&'static str> {
    PROCESS_NAME.get().map(|s| s.as_str())
}

// ── Reserved metadata keys for context propagation ──────────────

/// Metadata key for the caller's process name.
pub const PEEPS_CALLER_PROCESS_KEY: &str = "peeps.caller_process";

/// Metadata key for the caller's connection name.
pub const PEEPS_CALLER_CONNECTION_KEY: &str = "peeps.caller_connection";

/// Metadata key for the caller's request ID.
pub const PEEPS_CALLER_REQUEST_ID_KEY: &str = "peeps.caller_request_id";

/// Metadata key for the span ID (ULID) assigned to an outgoing request.
pub const PEEPS_SPAN_ID_KEY: &str = "peeps.span_id";

/// Metadata key for the parent span ID (ULID) when propagating across requests.
pub const PEEPS_PARENT_SPAN_ID_KEY: &str = "peeps.parent_span_id";

/// Metadata key for the chain ID used to derive cross-process channel IDs.
pub const PEEPS_CHAIN_ID_KEY: &str = "peeps.chain_id";

// ── Roam session snapshot types ──────────────────────────────────

/// Direction of an RPC request (serializable).
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum Direction {
    /// We sent the request, waiting for response.
    Outgoing,
    /// We received the request, processing it.
    Incoming,
}

/// Direction of a channel (serializable).
#[derive(Debug, Clone, Facet)]
#[repr(u8)]
pub enum ChannelDir {
    Tx,
    Rx,
}

// ── Canonical graph emission API (wrapper crates) ───────────────

/// Canonical node row emitted by instrumentation wrappers.
///
/// Common contract for all resources (`task`, `future`, `lock`,
/// `mpsc_tx`, `semaphore`, `oncecell`, `request`, `response`, etc.).
/// Type-specific fields belong in `attrs_json`.
/// Shared cross-resource context belongs in `attrs_json.meta`.
#[derive(Debug, Clone, Facet)]
pub struct Node {
    /// Globally unique node ID within a snapshot.
    ///
    /// Format: `{kind}:{ulid}` for local-only nodes (task, future, lock, sync).
    /// For cross-process-referenceable nodes:
    /// - request: `request:{span_id}` (span_id is a ULID from caller metadata)
    /// - response: `response:{ulid}`
    /// - roam channels: `roam_channel_{tx|rx}:{chain_id}:{channel_id}:{tx|rx}`
    pub id: String,

    /// Node kind (e.g. `task`, `future`, `lock`, `mpsc_tx`, `request`).
    pub kind: NodeKind,

    /// Optional human-readable label.
    pub label: Option<String>,

    /// JSON-encoded type-specific attributes. Contains a `meta` sub-object
    /// for shared cross-resource metadata.
    pub attrs_json: String,
}

/// Node kind enumeration for canonical graph nodes.
///
/// Corresponds to the `kind` field in [`GraphNodeSnapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum NodeKind {
    /// Canonical ID format: `future:{ulid}`
    Future,
    /// Canonical ID format: `lock:{ulid}`
    Lock,
    /// Canonical ID format: `tx:{ulid}`
    Tx,
    /// Canonical ID format: `rx:{ulid}`
    Rx,
    /// Canonical ID format: `remote_tx:{mother_request_ulid}:{channel_idx}:{dir}`
    RemoteTx,
    /// Canonical ID format: `remote_rx:{mother_request_ulid}:{channel_idx}:{dir}`
    RemoteRx,
    /// Canonical ID format: `request:{span_id}`
    Request,
    /// Canonical ID format: `response:{span_id}`
    Response,
    /// Canonical ID format: `semaphore:{ulid}`
    Semaphore,
    /// Canonical ID format: `oncecell:{ulid}`
    OnceCell,
}

impl NodeKind {
    /// Return a string representation suitable for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Future => "future",
            NodeKind::Lock => "lock",
            NodeKind::Tx => "tx",
            NodeKind::Rx => "rx",
            NodeKind::RemoteTx => "remote_tx",
            NodeKind::RemoteRx => "remote_rx",
            NodeKind::Request => "request",
            NodeKind::Response => "response",
            NodeKind::Semaphore => "semaphore",
            NodeKind::OnceCell => "oncecell",
        }
    }
}

/// Canonical edge row emitted by instrumentation wrappers.
///
/// All edges use kind `"needs"`. No inferred/derived/heuristic edges.
#[derive(Debug, Clone, Facet)]
pub struct Edge {
    /// Source node ID.
    pub src: String,

    /// Destination node ID.
    pub dst: String,

    /// JSON-encoded edge attributes (reserved for future use).
    pub attrs_json: String,
}

/// Per-process canonical graph snapshot envelope.
#[derive(Debug, Clone, Default, Facet)]
pub struct GraphSnapshot {
    pub process_name: String,
    pub proc_key: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Shared helper used by wrapper crates to emit canonical rows.
pub struct GraphSnapshotBuilder {
    graph: GraphSnapshot,
}

impl GraphSnapshotBuilder {
    pub fn new() -> Self {
        Self {
            graph: GraphSnapshot::default(),
        }
    }

    pub fn set_process_info(
        &mut self,
        process_name: impl Into<String>,
        proc_key: impl Into<String>,
    ) {
        self.graph.process_name = process_name.into();
        self.graph.proc_key = proc_key.into();
    }

    pub fn push_node(&mut self, node: Node) {
        self.graph.nodes.push(node);
    }

    pub fn push_edge(&mut self, edge: Edge) {
        self.graph.edges.push(edge);
    }

    pub fn finish(self) -> GraphSnapshot {
        self.graph
    }
}

impl Default for GraphSnapshotBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Shared metadata system ──────────────────────────────────────

/// Maximum number of metadata pairs per node.
pub const META_MAX_PAIRS: usize = 16;

/// Maximum key length in bytes.
pub const META_MAX_KEY_LEN: usize = 48;

/// Maximum value length in bytes.
pub const META_MAX_VALUE_LEN: usize = 256;

/// Metadata value for the graph metadata system.
///
/// All variants serialize as strings in `attrs_json.meta`.
pub enum MetaValue<'a> {
    Static(&'static str),
    Str(&'a str),
    U64(u64),
    I64(i64),
    Bool(bool),
}

pub trait IntoMetaValue<'a> {
    fn into_meta_value(self) -> MetaValue<'a>;
}

impl<'a> IntoMetaValue<'a> for &'a str {
    fn into_meta_value(self) -> MetaValue<'a> {
        MetaValue::Str(self)
    }
}

impl IntoMetaValue<'_> for u64 {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::U64(self)
    }
}

impl IntoMetaValue<'_> for i64 {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::I64(self)
    }
}

impl IntoMetaValue<'_> for u32 {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::U64(self as u64)
    }
}

impl IntoMetaValue<'_> for usize {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::U64(self as u64)
    }
}

impl IntoMetaValue<'_> for bool {
    fn into_meta_value(self) -> MetaValue<'static> {
        MetaValue::Bool(self)
    }
}

impl<'a> IntoMetaValue<'a> for MetaValue<'a> {
    fn into_meta_value(self) -> MetaValue<'a> {
        self
    }
}

impl MetaValue<'_> {
    /// Write this value as a string into the provided buffer.
    /// Returns the number of bytes written, or None if the buffer is too small.
    fn write_to(&self, buf: &mut [u8]) -> Option<usize> {
        use std::io::Write;
        match self {
            MetaValue::Static(s) | MetaValue::Str(s) => {
                let bytes = s.as_bytes();
                if bytes.len() > buf.len() {
                    return None;
                }
                buf[..bytes.len()].copy_from_slice(bytes);
                Some(bytes.len())
            }
            MetaValue::U64(v) => {
                let mut cursor = std::io::Cursor::new(&mut buf[..]);
                write!(cursor, "{v}").ok()?;
                Some(cursor.position() as usize)
            }
            MetaValue::I64(v) => {
                let mut cursor = std::io::Cursor::new(&mut buf[..]);
                write!(cursor, "{v}").ok()?;
                Some(cursor.position() as usize)
            }
            MetaValue::Bool(v) => {
                let s = if *v { "true" } else { "false" };
                let bytes = s.as_bytes();
                if bytes.len() > buf.len() {
                    return None;
                }
                buf[..bytes.len()].copy_from_slice(bytes);
                Some(bytes.len())
            }
        }
    }
}

/// Validate a metadata key: `[a-z0-9_.-]+`, max 48 bytes.
fn is_valid_meta_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= META_MAX_KEY_LEN
        && bytes.iter().all(|&b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.' || b == b'-'
        })
}

/// A validated metadata entry stored on the stack.
struct MetaEntry<'a> {
    key: &'a str,
    /// Value rendered as a string, stored in a stack buffer.
    value_buf: [u8; META_MAX_VALUE_LEN],
    value_len: usize,
}

/// Stack-based metadata builder for canonical graph nodes.
///
/// Validates keys/values per the spec and drops invalid pairs silently.
/// No heap allocation until `to_json_object()` is called.
pub struct MetaBuilder<'a, const N: usize = META_MAX_PAIRS> {
    entries: [std::mem::MaybeUninit<MetaEntry<'a>>; N],
    len: usize,
}

impl<'a, const N: usize> MetaBuilder<'a, N> {
    /// Create an empty metadata builder.
    pub fn new() -> Self {
        Self {
            // SAFETY: MaybeUninit doesn't require initialization
            entries: unsafe { std::mem::MaybeUninit::uninit().assume_init() },
            len: 0,
        }
    }

    /// Push a key-value pair. Invalid keys/values are silently dropped.
    pub fn push(&mut self, key: &'a str, value: MetaValue<'_>) -> &mut Self {
        if self.len >= N {
            return self;
        }
        if !is_valid_meta_key(key) {
            return self;
        }
        let mut value_buf = [0u8; META_MAX_VALUE_LEN];
        let Some(value_len) = value.write_to(&mut value_buf) else {
            return self;
        };
        if value_len > META_MAX_VALUE_LEN {
            return self;
        }
        self.entries[self.len] = std::mem::MaybeUninit::new(MetaEntry {
            key,
            value_buf,
            value_len,
        });
        self.len += 1;
        self
    }

    /// Serialize the metadata as a JSON object string: `{"key":"value",...}`.
    ///
    /// Returns an empty string if no entries are present.
    pub fn to_json_object(&self) -> String {
        if self.len == 0 {
            return String::new();
        }
        let mut out = String::with_capacity(self.len * 32);
        out.push('{');
        for i in 0..self.len {
            // SAFETY: entries[0..self.len] are initialized
            let entry = unsafe { self.entries[i].assume_init_ref() };
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            json_escape_into(&mut out, entry.key);
            out.push_str("\":\"");
            let value_str = std::str::from_utf8(&entry.value_buf[..entry.value_len]).unwrap_or("");
            json_escape_into(&mut out, value_str);
            out.push('"');
        }
        out.push('}');
        out
    }
}

impl<'a, const N: usize> Default for MetaBuilder<'a, N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape a string for JSON (handles `"`, `\`, and control chars).
pub fn json_escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

/// Build a [`MetaBuilder`] on the stack from key-value literal pairs.
///
/// When the `diagnostics` feature is disabled in wrapper crates, the
/// calling macro (`peepable_with_meta!`) should compile this away entirely.
///
/// ```ignore
/// use peeps_types::{peep_meta, MetaValue};
/// let meta = peep_meta! {
///     "request.id" => MetaValue::U64(42),
///     "request.method" => MetaValue::Static("GetUser"),
/// };
/// ```
#[macro_export]
macro_rules! peep_meta {
    ($($k:literal => $v:expr),* $(,)?) => {{
        const _COUNT: usize = $crate::peep_meta!(@count $($k),*);
        let mut builder = $crate::MetaBuilder::<_COUNT>::new();
        $(builder.push($k, $v);)*
        builder
    }};
    (@count $($k:literal),*) => {
        0usize $(+ { let _ = $k; 1usize })*
    };
}

// ── Canonical ID construction ───────────────────────────────────

/// Sanitize a string segment for use in canonical IDs.
///
/// Replaces any character not in `[a-z0-9._-]` with `-`.
/// Colons are forbidden in proc_key segments.
pub fn sanitize_id_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-' {
                c
            } else if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Construct a canonical `proc_key` from process name and PID.
///
/// Format: `{sanitized_process_name}-{pid}`
pub fn make_proc_key(process_name: &str, pid: u32) -> String {
    let slug = sanitize_id_segment(process_name);
    format!("{slug}-{pid}")
}

/// Generate a fresh ULID-based node ID with a kind prefix.
///
/// Format: `{kind}:{ulid}`
pub fn new_node_id(kind: &str) -> String {
    format!("{kind}:{}", ulid::Ulid::new())
}

/// Helpers for ID construction. Node IDs are ULID-based, not structured.
pub mod canonical_id {
    /// Construct a sanitized connection token: `conn_{id}`.
    pub fn connection(id: u64) -> String {
        format!("conn_{id}")
    }

    /// Construct a correlation key for request/response pairing.
    pub fn correlation_key(connection: &str, request_id: u64) -> String {
        format!("{connection}:{request_id}")
    }

    /// Construct a roam channel node ID from chain_id, channel_id, and endpoint.
    ///
    /// Both sides of a cross-process channel derive the same ID from shared metadata.
    pub fn roam_channel(chain_id: &str, channel_id: u64, endpoint: &str) -> String {
        format!("roam_channel_{endpoint}:{chain_id}:{channel_id}:{endpoint}")
    }

    /// Construct a request node ID from the span_id (ULID from caller metadata).
    ///
    /// The caller generates the span_id and both sides use it to link
    /// the outgoing request to the incoming response.
    pub fn request_from_span_id(span_id: &str) -> String {
        format!("request:{span_id}")
    }
}

// ── Canonical metadata keys ─────────────────────────────────────

/// Well-known metadata keys for `attrs_json.meta`.
pub mod meta_key {
    pub const REQUEST_ID: &str = "request.id";
    pub const REQUEST_METHOD: &str = "request.method";
    pub const REQUEST_CORRELATION_KEY: &str = "request.correlation_key";
    pub const RPC_CONNECTION: &str = "rpc.connection";
    pub const RPC_PEER: &str = "rpc.peer";
    pub const TASK_ID: &str = "task.id";
    pub const FUTURE_ID: &str = "future.id";
    pub const CHANNEL_ID: &str = "channel.id";
    pub const RESOURCE_PATH: &str = "resource.path";
    pub const CTX_MODULE_PATH: &str = "ctx.module_path";
    pub const CTX_FILE: &str = "ctx.file";
    pub const CTX_LINE: &str = "ctx.line";
    pub const CTX_CRATE_NAME: &str = "ctx.crate_name";
    pub const CTX_CRATE_VERSION: &str = "ctx.crate_version";
    pub const CTX_CALLSITE: &str = "ctx.callsite";
}

// ── Snapshot protocol types ──────────────────────────────────────

/// Server-to-client: request a snapshot from a connected process.
#[derive(Debug, Clone, Facet)]
pub struct SnapshotRequest {
    pub r#type: String,
    pub snapshot_id: i64,
    pub timeout_ms: i64,
}

/// Client-to-server: lightweight reply carrying only the canonical graph.
#[derive(Debug, Clone, Facet)]
pub struct GraphReply {
    pub r#type: String,
    pub snapshot_id: i64,
    pub process: String,
    pub pid: u32,
    pub graph: Option<GraphSnapshot>,
}
