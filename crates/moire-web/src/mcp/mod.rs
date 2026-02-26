use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Uri};
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Router};
use facet::Facet;
use moire_trace_types::{BacktraceId, FrameId};
use moire_types::{
    BacktraceFrameResolved, BacktraceFrameUnresolved, CutId, EdgeKind, Entity, EntityBody,
    EntityId, ProcessId, ProcessSnapshotView, SnapshotBacktrace, SnapshotBacktraceFrame,
    SnapshotCutResponse, TriggerCutResponse,
};
use moire_wire::{ServerMessage, encode_server_message_default};
use rust_mcp_sdk::id_generator::{FastIdGenerator, UuidGenerator};
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::mcp_http::{GenericBody, McpAppState, McpHttpHandler};
use rust_mcp_sdk::mcp_server::error::TransportServerError;
use rust_mcp_sdk::mcp_server::{ServerHandler, ToMcpServerHandler};
use rust_mcp_sdk::schema::{
    CallToolError, CallToolRequestParams, CallToolResult, Implementation, InitializeResult,
    LATEST_PROTOCOL_VERSION, ListToolsResult, PaginatedRequestParams, RpcError, ServerCapabilities,
    ServerCapabilitiesTools,
};
use rust_mcp_sdk::session_store::InMemorySessionStore;
use rust_mcp_sdk::{TransportOptions, tool_box};
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::api::snapshot::take_snapshot_internal;
use crate::api::source::lookup_source_text_location_in_db;
use crate::app::{AppState, CutState, remember_snapshot};
use crate::db::persist_cut_request;
use crate::snapshot::table::{
    is_pending_frame, load_snapshot_backtrace_table, lookup_frame_source_by_raw,
};
use crate::symbolication::symbolicate_pending_frames_for_backtraces;
use crate::util::time::now_nanos;
use moire_source_context::{cut_source_compact, extract_enclosing_fn, extract_target_statement};

const DEFAULT_MCP_ENDPOINT: &str = "/mcp";
const DEFAULT_MCP_PING_INTERVAL: Duration = Duration::from_secs(12);
const DEFAULT_WAIT_CHAIN_MAX_DEPTH: usize = 16;
const DEFAULT_WAIT_CHAIN_MAX_RESULTS: usize = 200;
const DEFAULT_SYMBOLICATION_WAIT_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_SYMBOLICATION_WAIT_TICK: Duration = Duration::from_millis(100);
const MAX_RENDERED_SOURCE_LINES: usize = 24;
const SOURCE_FRAMES_PER_ITEM: usize = 3;
const SYSTEM_CRATES: &[&str] = &[
    "std",
    "core",
    "alloc",
    "tokio",
    "tokio_util",
    "futures",
    "futures_core",
    "futures_util",
    "moire",
    "moire_trace_capture",
    "moire_runtime",
    "moire_tokio",
];

#[mcp_tool(
    name = "moire_help",
    description = "Read this first. Explains deadlock workflow, entity kinds, common hang patterns, and how to use all moire MCP tools effectively."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HelpTool {}

#[mcp_tool(
    name = "moire_cut_fresh",
    description = "Trigger a coordinated cut and capture a fresh snapshot. Returns cut_id + snapshot metadata."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CutFreshTool {}

#[mcp_tool(
    name = "moire_wait_edges",
    description = "Return waiting_on edges for one snapshot with embedded text/plain source context on nodes."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct WaitEdgesTool {
    #[serde(default)]
    pub snapshot_id: Option<i64>,
}

#[mcp_tool(
    name = "moire_wait_chains",
    description = "Return precomputed wait chains over waiting_on edges, including cycle detection and embedded source context."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct WaitChainsTool {
    #[serde(default)]
    pub snapshot_id: Option<i64>,
    #[serde(default)]
    pub roots: Option<Vec<String>>,
    #[serde(default)]
    pub max_depth: Option<u32>,
}

#[mcp_tool(
    name = "moire_deadlock_candidates",
    description = "Return SCC/cycle-based deadlock candidates with confidence and reason tags."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeadlockCandidatesTool {
    #[serde(default)]
    pub snapshot_id: Option<i64>,
}

#[mcp_tool(
    name = "moire_entity",
    description = "Return one entity with incoming/outgoing wait edges, scopes, and embedded source context."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EntityTool {
    #[serde(default)]
    pub snapshot_id: Option<i64>,
    pub entity_id: String,
}

#[mcp_tool(
    name = "moire_channel_state",
    description = "Return channel-oriented state for one channel entity or all channels, including waiter counts and source context."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChannelStateTool {
    #[serde(default)]
    pub snapshot_id: Option<i64>,
    #[serde(default)]
    pub entity_id: Option<String>,
}

#[mcp_tool(
    name = "moire_task_state",
    description = "Return future/task-oriented state, including awaiting target, scopes, and source context."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TaskStateTool {
    #[serde(default)]
    pub snapshot_id: Option<i64>,
    #[serde(default)]
    pub entity_id: Option<String>,
}

#[mcp_tool(
    name = "moire_source_context",
    description = "Lookup frame source context in text/plain format (statement/enclosing fn/compact scope)."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SourceContextTool {
    #[serde(default)]
    pub snapshot_id: Option<i64>,
    pub frame_ids: Vec<u64>,
    pub format: String,
}

#[mcp_tool(
    name = "moire_backtrace",
    description = "Expand a backtrace from one snapshot, optionally embedding source context per frame."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BacktraceTool {
    #[serde(default)]
    pub snapshot_id: Option<i64>,
    pub backtrace_id: u64,
    #[serde(default)]
    pub with_source: Option<bool>,
}

#[mcp_tool(
    name = "moire_diff_snapshots",
    description = "Return entity/edge/channel/task deltas between two snapshot ids."
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DiffSnapshotsTool {
    pub from_snapshot_id: i64,
    pub to_snapshot_id: i64,
}

tool_box!(
    MoireTools,
    [
        HelpTool,
        CutFreshTool,
        WaitEdgesTool,
        WaitChainsTool,
        DeadlockCandidatesTool,
        EntityTool,
        ChannelStateTool,
        TaskStateTool,
        SourceContextTool,
        BacktraceTool,
        DiffSnapshotsTool
    ]
);

#[derive(Facet)]
struct McpCutFreshResponse {
    pub cut_id: CutId,
    pub requested_at_ns: i64,
    pub requested_connections: usize,
    pub snapshot_id: i64,
    pub captured_at_unix_ms: i64,
    pub process_count: usize,
    pub timed_out_count: usize,
}

#[derive(Facet)]
struct McpHelpResponse {
    pub read_this_first: String,
    pub first_steps: Vec<String>,
    pub tool_guide: Vec<McpHelpToolGuide>,
    pub entity_kinds: Vec<McpHelpEntityKind>,
    pub hang_patterns: Vec<McpHelpHangPattern>,
    pub interpretation_notes: Vec<String>,
}

#[derive(Facet)]
struct McpHelpToolGuide {
    pub tool: String,
    pub purpose: String,
    pub when_to_use: String,
    pub typical_args: String,
}

#[derive(Facet)]
struct McpHelpEntityKind {
    pub kind: String,
    pub means: String,
    pub hang_signal: String,
}

#[derive(Facet)]
struct McpHelpHangPattern {
    pub name: String,
    pub signature: String,
    pub likely_cause: String,
    pub next_calls: Vec<String>,
}

#[derive(Facet)]
struct McpWaitEdgesResponse {
    pub snapshot_id: i64,
    pub row_count: usize,
    pub wait_edges: Vec<McpWaitEdge>,
}

#[derive(Facet)]
struct McpWaitEdge {
    pub process_id: String,
    pub waiter_id: String,
    pub waiter_name: String,
    pub waiter_kind: String,
    pub blocked_on_id: String,
    pub blocked_on_name: String,
    pub blocked_on_kind: String,
    pub waiter_birth_ms: u64,
    pub blocked_birth_ms: u64,
    pub edge_kind: String,
    #[facet(skip_unless_truthy)]
    pub waiter_source: Option<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub waiter_sources: Vec<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub blocked_on_source: Option<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub blocked_on_sources: Vec<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub wait_site_source: Option<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub wait_site_sources: Vec<McpSourceContext>,
}

#[derive(Facet)]
struct McpWaitChainsResponse {
    pub snapshot_id: i64,
    pub chain_count: usize,
    pub chains: Vec<McpWaitChain>,
}

#[derive(Facet)]
struct McpWaitChain {
    pub chain_id: String,
    pub is_cycle: bool,
    pub has_external_wake_source: bool,
    pub summary: String,
    pub node_ids: Vec<String>,
    pub edges: Vec<McpChainEdge>,
    pub nodes: Vec<McpNodeSummary>,
}

#[derive(Facet)]
struct McpChainEdge {
    pub src_entity_id: String,
    pub dst_entity_id: String,
    #[facet(skip_unless_truthy)]
    pub wait_site_source: Option<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub wait_site_sources: Vec<McpSourceContext>,
}

#[derive(Facet)]
struct McpNodeSummary {
    pub process_id: String,
    pub entity_id: String,
    pub name: String,
    pub kind: String,
    #[facet(skip_unless_truthy)]
    pub source: Option<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub sources: Vec<McpSourceContext>,
}

#[derive(Facet)]
struct McpDeadlockCandidatesResponse {
    pub snapshot_id: i64,
    pub candidate_count: usize,
    pub candidates: Vec<McpDeadlockCandidate>,
}

#[derive(Facet)]
struct McpDeadlockCandidate {
    pub candidate_id: String,
    pub confidence: String,
    pub reasons: Vec<String>,
    pub entity_ids: Vec<String>,
    #[facet(skip_unless_truthy)]
    pub blocked_duration_hint_ms: Option<u64>,
    pub cycle_nodes: Vec<McpNodeSummary>,
}

#[derive(Facet)]
struct McpEntityResponse {
    pub snapshot_id: i64,
    pub process_id: String,
    pub process_name: String,
    pub pid: u32,
    pub entity_id: String,
    pub entity_name: String,
    pub entity_kind: String,
    pub entity_body_json: String,
    pub incoming_wait_edges: Vec<McpChainEdge>,
    pub outgoing_wait_edges: Vec<McpChainEdge>,
    pub scope_ids: Vec<String>,
    #[facet(skip_unless_truthy)]
    pub source: Option<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub sources: Vec<McpSourceContext>,
}

#[derive(Facet)]
struct McpChannelStateResponse {
    pub snapshot_id: i64,
    pub channels: Vec<McpChannelState>,
}

#[derive(Facet)]
struct McpChannelState {
    pub process_id: String,
    pub entity_id: String,
    pub name: String,
    pub channel_kind: String,
    #[facet(skip_unless_truthy)]
    pub capacity: Option<u32>,
    #[facet(skip_unless_truthy)]
    pub occupancy: Option<u32>,
    pub sender_waiters: u32,
    pub receiver_waiters: u32,
    #[facet(skip_unless_truthy)]
    pub lifecycle_hints: Option<String>,
    #[facet(skip_unless_truthy)]
    pub source: Option<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub sources: Vec<McpSourceContext>,
}

#[derive(Facet)]
struct McpTaskStateResponse {
    pub snapshot_id: i64,
    pub tasks: Vec<McpTaskState>,
}

#[derive(Facet)]
struct McpTaskState {
    pub process_id: String,
    pub entity_id: String,
    pub name: String,
    pub entry_backtrace_id: u64,
    #[facet(skip_unless_truthy)]
    pub entry_frame_id: Option<u64>,
    pub entry_frame_ids: Vec<u64>,
    #[facet(skip_unless_truthy)]
    pub awaiting_on_entity_id: Option<String>,
    pub scope_ids: Vec<String>,
    #[facet(skip_unless_truthy)]
    pub source: Option<McpSourceContext>,
    #[facet(skip_unless_truthy)]
    pub sources: Vec<McpSourceContext>,
}

#[derive(Facet)]
struct McpSourceContextResponse {
    pub snapshot_id: i64,
    pub format: String,
    pub previews: Vec<McpSourceContext>,
    pub unavailable_frame_ids: Vec<u64>,
}

#[derive(Facet)]
struct McpBacktraceResponse {
    pub snapshot_id: i64,
    pub backtrace_id: u64,
    pub frame_count: usize,
    pub frames: Vec<McpBacktraceFrame>,
}

#[derive(Facet)]
struct McpBacktraceFrame {
    pub frame_id: u64,
    pub status: String,
    pub module_path: String,
    #[facet(skip_unless_truthy)]
    pub function_name: Option<String>,
    #[facet(skip_unless_truthy)]
    pub source_file: Option<String>,
    #[facet(skip_unless_truthy)]
    pub line: Option<u32>,
    #[facet(skip_unless_truthy)]
    pub rel_pc: Option<u64>,
    #[facet(skip_unless_truthy)]
    pub reason: Option<String>,
    #[facet(skip_unless_truthy)]
    pub source: Option<McpSourceContext>,
}

#[derive(Facet)]
struct McpDiffSnapshotsResponse {
    pub from_snapshot_id: i64,
    pub to_snapshot_id: i64,
    pub entity_added: Vec<String>,
    pub entity_removed: Vec<String>,
    pub waiting_on_added: Vec<String>,
    pub waiting_on_removed: Vec<String>,
    pub channel_changes: Vec<McpChannelDiff>,
    pub task_changes: Vec<McpTaskDiff>,
}

#[derive(Facet)]
struct McpChannelDiff {
    pub entity_id: String,
    pub before: String,
    pub after: String,
}

#[derive(Facet)]
struct McpTaskDiff {
    pub entity_id: String,
    #[facet(skip_unless_truthy)]
    pub awaiting_before: Option<String>,
    #[facet(skip_unless_truthy)]
    pub awaiting_after: Option<String>,
}

#[derive(Facet, Clone)]
struct McpSourceContext {
    pub format: String,
    pub frame_id: u64,
    pub source_file: String,
    pub target_line: u32,
    #[facet(skip_unless_truthy)]
    pub target_col: Option<u32>,
    pub total_lines: u32,
    #[facet(skip_unless_truthy)]
    pub statement_text: Option<String>,
    #[facet(skip_unless_truthy)]
    pub enclosing_fn_text: Option<String>,
    #[facet(skip_unless_truthy)]
    pub compact_scope_text: Option<String>,
    #[facet(skip_unless_truthy)]
    pub compact_scope_range: Option<McpLineRange>,
}

#[derive(Facet, Clone)]
struct McpLineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Clone)]
struct WaitNode {
    process_id: String,
    ptime_now_ms: u64,
    entity_id: String,
    name: String,
    kind: String,
    birth_ms: u64,
    frame_ids: Vec<FrameId>,
}

#[derive(Clone)]
struct WaitEdgeRuntime {
    process_id: String,
    src_key: String,
    dst_key: String,
    dst_entity_id: String,
    edge_frame_ids: Vec<FrameId>,
}

type WaitGraph = (
    HashMap<String, WaitNode>,
    Vec<WaitEdgeRuntime>,
    HashMap<String, Vec<String>>,
    HashMap<String, usize>,
);

#[derive(Clone)]
struct MoireMcpHandler {
    state: AppState,
}

impl MoireMcpHandler {
    fn new(state: AppState) -> Self {
        Self { state }
    }

    async fn dispatch_tool(
        &self,
        tool_name: &str,
        args: &JsonMap<String, JsonValue>,
    ) -> Result<String, String> {
        match tool_name {
            "moire_help" => self.tool_help().await,
            "moire_cut_fresh" => self.tool_cut_fresh().await,
            "moire_wait_edges" => {
                let snapshot_id = optional_i64(args, "snapshot_id")?;
                self.tool_wait_edges(snapshot_id).await
            }
            "moire_wait_chains" => {
                let snapshot_id = optional_i64(args, "snapshot_id")?;
                let roots = optional_string_list(args, "roots")?;
                let max_depth = optional_u32(args, "max_depth")?;
                self.tool_wait_chains(snapshot_id, roots, max_depth).await
            }
            "moire_deadlock_candidates" => {
                let snapshot_id = optional_i64(args, "snapshot_id")?;
                self.tool_deadlock_candidates(snapshot_id).await
            }
            "moire_entity" => {
                let snapshot_id = optional_i64(args, "snapshot_id")?;
                let entity_id = required_non_empty_string(args, "entity_id")?;
                self.tool_entity(snapshot_id, entity_id).await
            }
            "moire_channel_state" => {
                let snapshot_id = optional_i64(args, "snapshot_id")?;
                let entity_id = optional_non_empty_string(args, "entity_id")?;
                self.tool_channel_state(snapshot_id, entity_id).await
            }
            "moire_task_state" => {
                let snapshot_id = optional_i64(args, "snapshot_id")?;
                let entity_id = optional_non_empty_string(args, "entity_id")?;
                self.tool_task_state(snapshot_id, entity_id).await
            }
            "moire_source_context" => {
                let snapshot_id = optional_i64(args, "snapshot_id")?;
                let frame_ids = required_u64_list(args, "frame_ids")?;
                let format = required_non_empty_string(args, "format")?;
                self.tool_source_context(snapshot_id, frame_ids, format)
                    .await
            }
            "moire_backtrace" => {
                let snapshot_id = optional_i64(args, "snapshot_id")?;
                let backtrace_id = required_u64(args, "backtrace_id")?;
                let with_source = optional_bool(args, "with_source")?.unwrap_or(false);
                self.tool_backtrace(snapshot_id, backtrace_id, with_source)
                    .await
            }
            "moire_diff_snapshots" => {
                let from_snapshot_id = required_i64(args, "from_snapshot_id")?;
                let to_snapshot_id = required_i64(args, "to_snapshot_id")?;
                self.tool_diff_snapshots(from_snapshot_id, to_snapshot_id)
                    .await
            }
            other => Err(format!("unknown tool: {other}")),
        }
    }

    async fn tool_help(&self) -> Result<String, String> {
        let response = McpHelpResponse {
            read_this_first: String::from(
                "Run moire_help first in every new session, then run moire_cut_fresh. \
Use the returned snapshot_id for all follow-up calls to stay on one coherent cut.",
            ),
            first_steps: vec![
                String::from("1) moire_help"),
                String::from("2) moire_cut_fresh"),
                String::from("3) moire_wait_chains { snapshot_id }"),
                String::from("4) moire_deadlock_candidates { snapshot_id }"),
                String::from(
                    "5) moire_entity / moire_channel_state / moire_task_state on interesting nodes",
                ),
                String::from(
                    "6) moire_diff_snapshots { from_snapshot_id, to_snapshot_id } if you need to prove no progress",
                ),
            ],
            tool_guide: vec![
                McpHelpToolGuide {
                    tool: String::from("moire_cut_fresh"),
                    purpose: String::from("Capture a new coordinated cut and snapshot anchor."),
                    when_to_use: String::from("Always first for live debugging."),
                    typical_args: String::from("{}"),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_wait_edges"),
                    purpose: String::from("Flat waiting_on edges with node + wait-site source."),
                    when_to_use: String::from("Need low-level raw wait graph facts."),
                    typical_args: String::from("{ snapshot_id }"),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_wait_chains"),
                    purpose: String::from("Precomputed dependency chains with cycle detection."),
                    when_to_use: String::from("Primary traversal view for hangs."),
                    typical_args: String::from("{ snapshot_id, roots?, max_depth? }"),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_deadlock_candidates"),
                    purpose: String::from("SCC-based deadlock candidates with confidence/reasons."),
                    when_to_use: String::from("Need probable root-cause candidates quickly."),
                    typical_args: String::from("{ snapshot_id }"),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_entity"),
                    purpose: String::from(
                        "Inspect one entity with incoming/outgoing waits + scopes.",
                    ),
                    when_to_use: String::from("Drilling into one suspicious node."),
                    typical_args: String::from("{ snapshot_id, entity_id }"),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_channel_state"),
                    purpose: String::from(
                        "Inspect channel occupancy/capacity and waiter pressure.",
                    ),
                    when_to_use: String::from("Suspected producer/consumer stall."),
                    typical_args: String::from("{ snapshot_id, entity_id? }"),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_task_state"),
                    purpose: String::from("Inspect task/future await target + scope context."),
                    when_to_use: String::from("Suspected task/future parking issue."),
                    typical_args: String::from("{ snapshot_id, entity_id? }"),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_source_context"),
                    purpose: String::from("Direct frame source lookup in text/plain."),
                    when_to_use: String::from("Need ad-hoc source for specific frame_ids."),
                    typical_args: String::from(
                        "{ snapshot_id?, frame_ids, format: \"text/plain\" }",
                    ),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_backtrace"),
                    purpose: String::from("Expand one backtrace, optionally with source snippets."),
                    when_to_use: String::from("Need full call stack context."),
                    typical_args: String::from("{ snapshot_id, backtrace_id, with_source? }"),
                },
                McpHelpToolGuide {
                    tool: String::from("moire_diff_snapshots"),
                    purpose: String::from("Show progress/no-progress across two cuts."),
                    when_to_use: String::from("Need to prove stasis or identify transitions."),
                    typical_args: String::from("{ from_snapshot_id, to_snapshot_id }"),
                },
            ],
            entity_kinds: vec![
                McpHelpEntityKind {
                    kind: String::from("future"),
                    means: String::from("A task/future execution state."),
                    hang_signal: String::from(
                        "Long wait chain roots; waiting_on edges that never clear.",
                    ),
                },
                McpHelpEntityKind {
                    kind: String::from("mpsc_tx / mpsc_rx"),
                    means: String::from("Bounded/unbounded MPSC channel endpoints."),
                    hang_signal: String::from(
                        "tx waits with full buffer or rx waits with no producer progress.",
                    ),
                },
                McpHelpEntityKind {
                    kind: String::from("broadcast_tx / broadcast_rx"),
                    means: String::from("Broadcast channel endpoints."),
                    hang_signal: String::from(
                        "Receivers lagging or waiting while sender path is blocked.",
                    ),
                },
                McpHelpEntityKind {
                    kind: String::from("watch_tx / watch_rx"),
                    means: String::from("Watch channel update/read endpoints."),
                    hang_signal: String::from("rx waiting with no tx updates."),
                },
                McpHelpEntityKind {
                    kind: String::from("oneshot_tx / oneshot_rx"),
                    means: String::from("Single-message synchronization."),
                    hang_signal: String::from("rx waiting and tx never reaches send."),
                },
                McpHelpEntityKind {
                    kind: String::from("lock / semaphore / notify / once_cell"),
                    means: String::from("Synchronization primitives."),
                    hang_signal: String::from(
                        "Cycles through holders/waiters or no external wake source.",
                    ),
                },
                McpHelpEntityKind {
                    kind: String::from("net_* / request / response"),
                    means: String::from("I/O and RPC boundary operations."),
                    hang_signal: String::from(
                        "Can be real external wait; confirm with snapshot diff before calling deadlock.",
                    ),
                },
                McpHelpEntityKind {
                    kind: String::from("custom / aether"),
                    means: String::from("User-defined or synthetic placeholder entities."),
                    hang_signal: String::from(
                        "Use source snippets + neighboring edges for interpretation.",
                    ),
                },
            ],
            hang_patterns: vec![
                McpHelpHangPattern {
                    name: String::from("Pure wait cycle"),
                    signature: String::from(
                        "SCC with >=2 nodes and no clear external wake source.",
                    ),
                    likely_cause: String::from("Logical deadlock or handshake ordering bug."),
                    next_calls: vec![
                        String::from("moire_deadlock_candidates { snapshot_id }"),
                        String::from("moire_wait_chains { snapshot_id }"),
                        String::from("moire_entity { snapshot_id, entity_id }"),
                    ],
                },
                McpHelpHangPattern {
                    name: String::from("Producer starvation"),
                    signature: String::from(
                        "Receivers waiting on channel while upstream producer chain is blocked.",
                    ),
                    likely_cause: String::from(
                        "Missed spawn, gated branch, or upstream await cycle.",
                    ),
                    next_calls: vec![
                        String::from("moire_channel_state { snapshot_id }"),
                        String::from("moire_wait_chains { snapshot_id }"),
                        String::from("moire_task_state { snapshot_id }"),
                    ],
                },
                McpHelpHangPattern {
                    name: String::from("Backpressure stall"),
                    signature: String::from(
                        "Senders blocked with high/at-capacity channel occupancy.",
                    ),
                    likely_cause: String::from(
                        "Consumer slow or consumer blocked on unrelated wait.",
                    ),
                    next_calls: vec![
                        String::from("moire_channel_state { snapshot_id }"),
                        String::from("moire_wait_edges { snapshot_id }"),
                        String::from("moire_task_state { snapshot_id }"),
                    ],
                },
                McpHelpHangPattern {
                    name: String::from("Looks deadlocked but is external wait"),
                    signature: String::from(
                        "Chains terminate in net/request/response-style boundary nodes.",
                    ),
                    likely_cause: String::from(
                        "Remote dependency or I/O latency rather than internal cycle.",
                    ),
                    next_calls: vec![
                        String::from(
                            "moire_backtrace { snapshot_id, backtrace_id, with_source: true }",
                        ),
                        String::from("moire_diff_snapshots { from_snapshot_id, to_snapshot_id }"),
                    ],
                },
                McpHelpHangPattern {
                    name: String::from("No progress across cuts"),
                    signature: String::from(
                        "Repeated snapshots show same waiting_on graph and same hot entities.",
                    ),
                    likely_cause: String::from("Stable deadlock or starvation."),
                    next_calls: vec![
                        String::from("moire_cut_fresh"),
                        String::from("moire_diff_snapshots { from_snapshot_id, to_snapshot_id }"),
                        String::from("moire_deadlock_candidates { snapshot_id }"),
                    ],
                },
            ],
            interpretation_notes: vec![
                String::from(
                    "Prefer snapshot_id-pinned queries. Avoid mixing latest and pinned data in one diagnosis.",
                ),
                String::from(
                    "Waiting_on is the primary deadlock edge. Pairing/ownership edges are contextual but non-blocking by themselves.",
                ),
                String::from(
                    "Source snippets are best-effort from symbolication + tree-sitter extraction; missing snippets are explicit, not fabricated.",
                ),
                String::from(
                    "Do not conclude root cause from graph shape alone. Verify concrete caller code around highlighted wait sites.",
                ),
                String::from(
                    "Treat single-cut deadlock conclusions as provisional; confirm with moire_diff_snapshots when possible.",
                ),
            ],
        };
        Ok(render_help_markdown(&response))
    }

    async fn tool_cut_fresh(&self) -> Result<String, String> {
        let cut = self.trigger_cut().await?;
        let snapshot = take_snapshot_internal(&self.state).await;
        let response = McpCutFreshResponse {
            cut_id: cut.cut_id,
            requested_at_ns: cut.requested_at_ns,
            requested_connections: cut.requested_connections,
            snapshot_id: snapshot.snapshot_id,
            captured_at_unix_ms: snapshot.captured_at_unix_ms,
            process_count: snapshot.processes.len(),
            timed_out_count: snapshot.timed_out_processes.len(),
        };
        Ok(render_cut_fresh_markdown(&response))
    }

    async fn tool_wait_edges(&self, snapshot_id: Option<i64>) -> Result<String, String> {
        let snapshot = self
            .ensure_symbolication_ready(self.load_snapshot(snapshot_id).await?)
            .await?;
        let (nodes, edges, _, _) = self.build_wait_graph(&snapshot)?;
        let sources = self
            .load_source_for_graph(&snapshot, nodes.values(), &edges)
            .await?;

        let mut wait_edges = Vec::with_capacity(edges.len());
        for edge in edges {
            let src = nodes
                .get(&edge.src_key)
                .ok_or_else(|| format!("invariant violated: missing src node {}", edge.src_key))?;
            let dst = nodes
                .get(&edge.dst_key)
                .ok_or_else(|| format!("invariant violated: missing dst node {}", edge.dst_key))?;
            wait_edges.push(McpWaitEdge {
                process_id: edge.process_id,
                waiter_id: src.entity_id.clone(),
                waiter_name: src.name.clone(),
                waiter_kind: src.kind.clone(),
                blocked_on_id: dst.entity_id.clone(),
                blocked_on_name: dst.name.clone(),
                blocked_on_kind: dst.kind.clone(),
                waiter_birth_ms: src.birth_ms,
                blocked_birth_ms: dst.birth_ms,
                edge_kind: String::from("waiting_on"),
                waiter_source: source_for_node(src, &sources),
                waiter_sources: sources_for_node(src, &sources),
                blocked_on_source: source_for_node(dst, &sources),
                blocked_on_sources: sources_for_node(dst, &sources),
                wait_site_source: edge
                    .edge_frame_ids
                    .first()
                    .and_then(|id| sources.get(&id.as_u64()).cloned()),
                wait_site_sources: sources_for_frame_ids(&edge.edge_frame_ids, &sources),
            });
        }

        let response = McpWaitEdgesResponse {
            snapshot_id: snapshot.snapshot_id,
            row_count: wait_edges.len(),
            wait_edges,
        };
        Ok(render_wait_edges_markdown(&response))
    }

    async fn tool_wait_chains(
        &self,
        snapshot_id: Option<i64>,
        roots: Option<Vec<String>>,
        max_depth: Option<u32>,
    ) -> Result<String, String> {
        let snapshot = self
            .ensure_symbolication_ready(self.load_snapshot(snapshot_id).await?)
            .await?;
        let (nodes, edges, adjacency, indegree) = self.build_wait_graph(&snapshot)?;
        let sources = self
            .load_source_for_graph(&snapshot, nodes.values(), &edges)
            .await?;

        let mut edge_wait_source: HashMap<(String, String), Option<McpSourceContext>> =
            HashMap::new();
        let mut edge_wait_sources: HashMap<(String, String), Vec<McpSourceContext>> =
            HashMap::new();
        for edge in &edges {
            let wait_sources = sources_for_frame_ids(&edge.edge_frame_ids, &sources);
            edge_wait_source.insert(
                (edge.src_key.clone(), edge.dst_key.clone()),
                wait_sources.first().cloned(),
            );
            edge_wait_sources.insert((edge.src_key.clone(), edge.dst_key.clone()), wait_sources);
        }

        let max_depth = max_depth
            .map(|v| v as usize)
            .unwrap_or(DEFAULT_WAIT_CHAIN_MAX_DEPTH)
            .max(1);

        let mut start_keys = if let Some(root_ids) = roots {
            self.resolve_roots(&nodes, &root_ids)
        } else {
            Vec::new()
        };
        if start_keys.is_empty() {
            for (key, next) in &adjacency {
                if !next.is_empty() && *indegree.get(key).unwrap_or(&0) == 0 {
                    start_keys.push(key.clone());
                }
            }
        }
        if start_keys.is_empty() {
            start_keys.extend(adjacency.keys().cloned());
        }
        start_keys.sort();
        start_keys.dedup();

        let mut chains: Vec<McpWaitChain> = Vec::new();
        let mut chain_count = 0usize;
        for start in start_keys {
            if chains.len() >= DEFAULT_WAIT_CHAIN_MAX_RESULTS {
                break;
            }
            let mut path: Vec<String> = vec![start.clone()];
            self.walk_wait_paths(
                &adjacency,
                &start,
                max_depth,
                &mut path,
                &mut chains,
                &mut chain_count,
                &nodes,
                &sources,
                &edge_wait_source,
                &edge_wait_sources,
            );
        }

        let response = McpWaitChainsResponse {
            snapshot_id: snapshot.snapshot_id,
            chain_count: chains.len(),
            chains,
        };
        Ok(render_wait_chains_markdown(&response))
    }

    async fn tool_deadlock_candidates(&self, snapshot_id: Option<i64>) -> Result<String, String> {
        let snapshot = self
            .ensure_symbolication_ready(self.load_snapshot(snapshot_id).await?)
            .await?;
        let (nodes, _edges, adjacency, _indegree) = self.build_wait_graph(&snapshot)?;
        let sources = self
            .load_source_for_nodes(&snapshot, nodes.values())
            .await?;

        let mut scc_input = adjacency.keys().cloned().collect::<Vec<_>>();
        scc_input.sort();
        let sccs = strongly_connected_components(scc_input, &adjacency);
        let mut candidates = Vec::new();
        for (idx, scc) in sccs.into_iter().enumerate() {
            if scc.len() <= 1 {
                let Some(node_id) = scc.first() else {
                    continue;
                };
                let self_loop = adjacency
                    .get(node_id)
                    .is_some_and(|outs| outs.iter().any(|dst| dst == node_id));
                if !self_loop {
                    continue;
                }
            }

            let mut reasons = vec![String::from("strongly_connected_wait_cycle")];
            let has_external_wake_source = scc
                .iter()
                .filter_map(|id| nodes.get(id))
                .any(|node| node_has_external_wake_source(node.kind.as_str()));
            if !has_external_wake_source {
                reasons.push(String::from("no_obvious_external_wake_source"));
            }
            let confidence = if !has_external_wake_source {
                String::from("high")
            } else {
                String::from("medium")
            };

            let mut entity_ids = Vec::with_capacity(scc.len());
            let mut cycle_nodes = Vec::with_capacity(scc.len());
            let mut min_age_hint: Option<u64> = None;
            for key in &scc {
                let Some(node) = nodes.get(key) else {
                    continue;
                };
                entity_ids.push(node.entity_id.clone());
                let age_hint = node.ptime_now_ms.saturating_sub(node.birth_ms);
                min_age_hint = Some(min_age_hint.map_or(age_hint, |curr| curr.min(age_hint)));
                cycle_nodes.push(McpNodeSummary {
                    process_id: node.process_id.clone(),
                    entity_id: node.entity_id.clone(),
                    name: node.name.clone(),
                    kind: node.kind.clone(),
                    source: source_for_node(node, &sources),
                    sources: sources_for_node(node, &sources),
                });
            }

            candidates.push(McpDeadlockCandidate {
                candidate_id: format!("candidate-{}", idx + 1),
                confidence,
                reasons,
                entity_ids,
                blocked_duration_hint_ms: min_age_hint,
                cycle_nodes,
            });
        }

        let response = McpDeadlockCandidatesResponse {
            snapshot_id: snapshot.snapshot_id,
            candidate_count: candidates.len(),
            candidates,
        };
        Ok(render_deadlock_candidates_markdown(&response))
    }

    async fn tool_entity(
        &self,
        snapshot_id: Option<i64>,
        entity_id: String,
    ) -> Result<String, String> {
        let snapshot = self
            .ensure_symbolication_ready(self.load_snapshot(snapshot_id).await?)
            .await?;
        let located = self.find_entity(&snapshot, &entity_id)?;
        let backtrace_index = backtrace_index(&snapshot);
        let frame_catalog = frame_catalog(&snapshot);
        let local_entities: HashMap<&str, &Entity> = located
            .0
            .snapshot
            .entities
            .iter()
            .map(|entity| (entity.id.as_str(), entity))
            .collect();
        let entity_frame_ids = selected_frames_for_entity(
            located.1,
            &backtrace_index,
            &frame_catalog,
            SOURCE_FRAMES_PER_ITEM,
        );

        let mut frame_ids = BTreeSet::new();
        for frame_id in &entity_frame_ids {
            frame_ids.insert(frame_id.as_u64());
        }
        for edge in &located.0.snapshot.edges {
            if edge.kind != EdgeKind::WaitingOn {
                continue;
            }
            if edge.dst.as_str() == entity_id || edge.src.as_str() == entity_id {
                for frame_id in selected_frames_for_backtrace_id(
                    edge.backtrace.as_u64(),
                    &backtrace_index,
                    &frame_catalog,
                    local_entities
                        .get(edge.src.as_str())
                        .map(|entity| frame_start_index_for_entity(entity))
                        .unwrap_or(0),
                    SOURCE_FRAMES_PER_ITEM,
                ) {
                    frame_ids.insert(frame_id.as_u64());
                }
            }
        }
        let source_by_frame = if frame_ids.is_empty() {
            HashMap::new()
        } else {
            self.resolve_source_contexts(frame_ids)
                .await?
                .0
                .into_iter()
                .map(|ctx| (ctx.frame_id, ctx))
                .collect::<HashMap<_, _>>()
        };
        let source = entity_frame_ids
            .first()
            .and_then(|frame_id| source_by_frame.get(&frame_id.as_u64()).cloned());
        let sources = entity_frame_ids
            .iter()
            .filter_map(|frame_id| source_by_frame.get(&frame_id.as_u64()).cloned())
            .collect::<Vec<_>>();

        let mut incoming = Vec::new();
        let mut outgoing = Vec::new();
        for edge in &located.0.snapshot.edges {
            if edge.kind != EdgeKind::WaitingOn {
                continue;
            }
            let wait_site_frame_ids = selected_frames_for_backtrace_id(
                edge.backtrace.as_u64(),
                &backtrace_index,
                &frame_catalog,
                local_entities
                    .get(edge.src.as_str())
                    .map(|entity| frame_start_index_for_entity(entity))
                    .unwrap_or(0),
                SOURCE_FRAMES_PER_ITEM,
            );
            if edge.dst.as_str() == entity_id {
                let wait_site_source = wait_site_frame_ids
                    .first()
                    .and_then(|frame_id| source_by_frame.get(&frame_id.as_u64()).cloned());
                let wait_site_sources = wait_site_frame_ids
                    .iter()
                    .filter_map(|frame_id| source_by_frame.get(&frame_id.as_u64()).cloned())
                    .collect::<Vec<_>>();
                incoming.push(McpChainEdge {
                    src_entity_id: edge.src.as_str().to_owned(),
                    dst_entity_id: edge.dst.as_str().to_owned(),
                    wait_site_source,
                    wait_site_sources,
                });
            }
            if edge.src.as_str() == entity_id {
                let wait_site_source = wait_site_frame_ids
                    .first()
                    .and_then(|frame_id| source_by_frame.get(&frame_id.as_u64()).cloned());
                let wait_site_sources = wait_site_frame_ids
                    .iter()
                    .filter_map(|frame_id| source_by_frame.get(&frame_id.as_u64()).cloned())
                    .collect::<Vec<_>>();
                outgoing.push(McpChainEdge {
                    src_entity_id: edge.src.as_str().to_owned(),
                    dst_entity_id: edge.dst.as_str().to_owned(),
                    wait_site_source,
                    wait_site_sources,
                });
            }
        }
        incoming.sort_by(|a, b| {
            a.src_entity_id
                .cmp(&b.src_entity_id)
                .then_with(|| a.dst_entity_id.cmp(&b.dst_entity_id))
        });
        outgoing.sort_by(|a, b| {
            a.src_entity_id
                .cmp(&b.src_entity_id)
                .then_with(|| a.dst_entity_id.cmp(&b.dst_entity_id))
        });

        let scope_ids = located
            .0
            .scope_entity_links
            .iter()
            .filter(|link| link.entity_id == entity_id)
            .map(|link| link.scope_id.clone())
            .collect();

        let response = McpEntityResponse {
            snapshot_id: snapshot.snapshot_id,
            process_id: located.0.process_id.as_str().to_owned(),
            process_name: located.0.process_name.clone(),
            pid: located.0.pid,
            entity_id: located.1.id.as_str().to_owned(),
            entity_name: located.1.name.clone(),
            entity_kind: entity_kind_name(&located.1.body).to_owned(),
            entity_body_json: facet_json::to_string(&located.1.body)
                .map_err(|error| format!("encode entity body json: {error}"))?,
            incoming_wait_edges: incoming,
            outgoing_wait_edges: outgoing,
            scope_ids,
            source,
            sources,
        };
        Ok(render_entity_markdown(&response))
    }

    async fn tool_channel_state(
        &self,
        snapshot_id: Option<i64>,
        entity_id: Option<String>,
    ) -> Result<String, String> {
        let snapshot = self
            .ensure_symbolication_ready(self.load_snapshot(snapshot_id).await?)
            .await?;
        let (nodes, edges, _adjacency, _indegree) = self.build_wait_graph(&snapshot)?;
        let sources = self
            .load_source_for_nodes(&snapshot, nodes.values())
            .await?;

        let mut channels = Vec::new();
        for process in &snapshot.processes {
            for entity in &process.snapshot.entities {
                if !is_channel_entity(&entity.body) {
                    continue;
                }
                if let Some(ref wanted) = entity_id
                    && entity.id.as_str() != wanted
                {
                    continue;
                }

                let (capacity, occupancy, lifecycle_hints, channel_kind) =
                    channel_metrics(&entity.body);
                let (sender_waiters, receiver_waiters) = count_waiters(&edges, &nodes, entity);
                let node_key = compose_node_key(&process.process_id, &entity.id);
                let node = nodes.get(&node_key);
                channels.push(McpChannelState {
                    process_id: process.process_id.as_str().to_owned(),
                    entity_id: entity.id.as_str().to_owned(),
                    name: entity.name.clone(),
                    channel_kind: channel_kind.to_owned(),
                    capacity,
                    occupancy,
                    sender_waiters,
                    receiver_waiters,
                    lifecycle_hints,
                    source: node.and_then(|n| source_for_node(n, &sources)),
                    sources: node
                        .map(|n| sources_for_node(n, &sources))
                        .unwrap_or_default(),
                });
            }
        }

        if let Some(wanted) = entity_id
            && channels.is_empty()
        {
            return Err(format!("unknown or non-channel entity_id `{wanted}`"));
        }
        channels.sort_by(|a, b| {
            a.process_id
                .cmp(&b.process_id)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.entity_id.cmp(&b.entity_id))
        });

        let response = McpChannelStateResponse {
            snapshot_id: snapshot.snapshot_id,
            channels,
        };
        Ok(render_channel_state_markdown(&response))
    }

    async fn tool_task_state(
        &self,
        snapshot_id: Option<i64>,
        entity_id: Option<String>,
    ) -> Result<String, String> {
        let snapshot = self
            .ensure_symbolication_ready(self.load_snapshot(snapshot_id).await?)
            .await?;
        let (nodes, _edges, _adjacency, _indegree) = self.build_wait_graph(&snapshot)?;
        let backtrace_index = backtrace_index(&snapshot);
        let frame_catalog = frame_catalog(&snapshot);
        let sources = self
            .load_source_for_nodes(&snapshot, nodes.values())
            .await?;

        let mut tasks = Vec::new();
        for process in &snapshot.processes {
            for entity in &process.snapshot.entities {
                if !is_task_entity(&entity.body) {
                    continue;
                }
                if let Some(ref wanted) = entity_id
                    && entity.id.as_str() != wanted
                {
                    continue;
                }

                let awaiting = process
                    .snapshot
                    .edges
                    .iter()
                    .find(|edge| {
                        edge.kind == EdgeKind::WaitingOn && edge.src.as_str() == entity.id.as_str()
                    })
                    .map(|edge| edge.dst.as_str().to_owned());

                let scope_ids = process
                    .scope_entity_links
                    .iter()
                    .filter(|link| link.entity_id == entity.id.as_str())
                    .map(|link| link.scope_id.clone())
                    .collect::<Vec<_>>();

                let node_key = compose_node_key(&process.process_id, &entity.id);
                let node = nodes.get(&node_key);
                let entry_frame_ids = selected_frames_for_entity(
                    entity,
                    &backtrace_index,
                    &frame_catalog,
                    SOURCE_FRAMES_PER_ITEM,
                );
                tasks.push(McpTaskState {
                    process_id: process.process_id.as_str().to_owned(),
                    entity_id: entity.id.as_str().to_owned(),
                    name: entity.name.clone(),
                    entry_backtrace_id: entity.backtrace.as_u64(),
                    entry_frame_id: entry_frame_ids.first().map(|frame_id| frame_id.as_u64()),
                    entry_frame_ids: entry_frame_ids
                        .iter()
                        .map(|frame_id| frame_id.as_u64())
                        .collect(),
                    awaiting_on_entity_id: awaiting,
                    scope_ids,
                    source: node.and_then(|n| source_for_node(n, &sources)),
                    sources: node
                        .map(|n| sources_for_node(n, &sources))
                        .unwrap_or_default(),
                });
            }
        }

        if let Some(wanted) = entity_id
            && tasks.is_empty()
        {
            return Err(format!("unknown or non-task entity_id `{wanted}`"));
        }
        tasks.sort_by(|a, b| {
            a.process_id
                .cmp(&b.process_id)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.entity_id.cmp(&b.entity_id))
        });

        let response = McpTaskStateResponse {
            snapshot_id: snapshot.snapshot_id,
            tasks,
        };
        Ok(render_task_state_markdown(&response))
    }

    async fn tool_source_context(
        &self,
        snapshot_id: Option<i64>,
        frame_ids: Vec<u64>,
        format: String,
    ) -> Result<String, String> {
        let snapshot = self
            .ensure_symbolication_ready(self.load_snapshot(snapshot_id).await?)
            .await?;
        if frame_ids.is_empty() {
            return Err("frame_ids must be non-empty".to_string());
        }
        let format = if format == "text/plain" || format == "text" {
            String::from("text/plain")
        } else {
            return Err(format!(
                "unsupported format `{format}`; supported values: text/plain"
            ));
        };

        let (previews, unavailable_frame_ids) = self
            .resolve_source_contexts(frame_ids.into_iter().collect::<BTreeSet<_>>())
            .await?;

        if previews.is_empty() && !unavailable_frame_ids.is_empty() {
            let backtrace_ids = snapshot
                .backtraces
                .iter()
                .map(|bt| bt.backtrace_id.as_u64())
                .collect::<HashSet<_>>();
            let all_look_like_backtrace_ids = unavailable_frame_ids
                .iter()
                .all(|id| backtrace_ids.contains(id));
            if all_look_like_backtrace_ids {
                return Err(
                    "frame_ids expects FRAME ids, but received values look like BACKTRACE ids. \
Call moire_backtrace first to list frame_ids for a backtrace."
                        .to_string(),
                );
            }
        }

        let response = McpSourceContextResponse {
            snapshot_id: snapshot.snapshot_id,
            format,
            previews,
            unavailable_frame_ids,
        };
        Ok(render_source_context_markdown(&response))
    }

    async fn tool_backtrace(
        &self,
        snapshot_id: Option<i64>,
        backtrace_id_raw: u64,
        with_source: bool,
    ) -> Result<String, String> {
        let snapshot = self
            .ensure_symbolication_ready(self.load_snapshot(snapshot_id).await?)
            .await?;
        let Some(backtrace) = snapshot
            .backtraces
            .iter()
            .find(|bt| bt.backtrace_id.as_u64() == backtrace_id_raw)
        else {
            return Err(format!("unknown backtrace_id {backtrace_id_raw}"));
        };

        let frame_map: HashMap<u64, &SnapshotBacktraceFrame> = snapshot
            .frames
            .iter()
            .map(|record| (record.frame_id.as_u64(), &record.frame))
            .collect();

        let source_by_frame = if with_source {
            let frame_ids: BTreeSet<u64> =
                backtrace.frame_ids.iter().map(|id| id.as_u64()).collect();
            self.resolve_source_contexts(frame_ids).await?.0
        } else {
            Vec::new()
        };
        let source_by_frame_map: HashMap<u64, McpSourceContext> = source_by_frame
            .into_iter()
            .map(|src| (src.frame_id, src))
            .collect();

        let mut frames = Vec::with_capacity(backtrace.frame_ids.len());
        for frame_id in &backtrace.frame_ids {
            let raw = frame_id.as_u64();
            let Some(frame) = frame_map.get(&raw) else {
                return Err(format!(
                    "invariant violated: frame {} referenced by backtrace {} is missing",
                    raw, backtrace_id_raw
                ));
            };

            let frame_out = match frame {
                SnapshotBacktraceFrame::Resolved(BacktraceFrameResolved {
                    module_path,
                    function_name,
                    source_file,
                    line,
                }) => McpBacktraceFrame {
                    frame_id: raw,
                    status: String::from("resolved"),
                    module_path: module_path.clone(),
                    function_name: Some(function_name.clone()),
                    source_file: Some(source_file.clone()),
                    line: *line,
                    rel_pc: None,
                    reason: None,
                    source: source_by_frame_map.get(&raw).cloned(),
                },
                SnapshotBacktraceFrame::Unresolved(BacktraceFrameUnresolved {
                    module_path,
                    rel_pc,
                    reason,
                }) => McpBacktraceFrame {
                    frame_id: raw,
                    status: String::from("unresolved"),
                    module_path: module_path.clone(),
                    function_name: None,
                    source_file: None,
                    line: None,
                    rel_pc: Some(rel_pc.get()),
                    reason: Some(reason.clone()),
                    source: source_by_frame_map.get(&raw).cloned(),
                },
            };
            frames.push(frame_out);
        }

        let response = McpBacktraceResponse {
            snapshot_id: snapshot.snapshot_id,
            backtrace_id: backtrace_id_raw,
            frame_count: frames.len(),
            frames,
        };
        Ok(render_backtrace_markdown(&response))
    }

    async fn tool_diff_snapshots(
        &self,
        from_snapshot_id: i64,
        to_snapshot_id: i64,
    ) -> Result<String, String> {
        let from = self.load_snapshot(Some(from_snapshot_id)).await?;
        let to = self.load_snapshot(Some(to_snapshot_id)).await?;

        let from_entities = snapshot_entity_keys(&from);
        let to_entities = snapshot_entity_keys(&to);

        let entity_added = to_entities
            .difference(&from_entities)
            .cloned()
            .collect::<Vec<_>>();
        let entity_removed = from_entities
            .difference(&to_entities)
            .cloned()
            .collect::<Vec<_>>();

        let from_waiting = snapshot_waiting_edges(&from);
        let to_waiting = snapshot_waiting_edges(&to);

        let waiting_on_added = to_waiting
            .difference(&from_waiting)
            .cloned()
            .collect::<Vec<_>>();
        let waiting_on_removed = from_waiting
            .difference(&to_waiting)
            .cloned()
            .collect::<Vec<_>>();

        let from_channel = snapshot_channel_fingerprint(&from);
        let to_channel = snapshot_channel_fingerprint(&to);
        let mut channel_changes = Vec::new();
        for (entity_id, after) in &to_channel {
            if let Some(before) = from_channel.get(entity_id)
                && before != after
            {
                channel_changes.push(McpChannelDiff {
                    entity_id: entity_id.clone(),
                    before: before.clone(),
                    after: after.clone(),
                });
            }
        }

        let from_tasks = snapshot_task_wait_target(&from);
        let to_tasks = snapshot_task_wait_target(&to);
        let mut task_changes = Vec::new();
        for (entity_id, awaiting_after) in &to_tasks {
            let awaiting_before = from_tasks.get(entity_id).cloned().unwrap_or(None);
            if awaiting_before != *awaiting_after {
                task_changes.push(McpTaskDiff {
                    entity_id: entity_id.clone(),
                    awaiting_before,
                    awaiting_after: awaiting_after.clone(),
                });
            }
        }

        let response = McpDiffSnapshotsResponse {
            from_snapshot_id,
            to_snapshot_id,
            entity_added,
            entity_removed,
            waiting_on_added,
            waiting_on_removed,
            channel_changes,
            task_changes,
        };
        Ok(render_diff_snapshots_markdown(&response))
    }

    async fn trigger_cut(&self) -> Result<TriggerCutResponse, String> {
        let (cut_id, cut_id_string, now_ns, requested_connections, outbound) = {
            let mut guard = self.state.inner.lock().await;
            let cut_num = guard.next_cut_id;
            guard.next_cut_id = guard.next_cut_id.next();
            let cut_id = cut_num.to_cut_id();
            let cut_id_string = cut_id.as_str().to_owned();
            let now_ns = now_nanos();
            let mut pending_conn_ids = BTreeSet::new();
            let mut outbound = Vec::new();
            for (conn_id, conn) in &guard.connections {
                pending_conn_ids.insert(*conn_id);
                outbound.push((*conn_id, conn.tx.clone()));
            }

            guard.cuts.insert(
                cut_id.clone(),
                CutState {
                    requested_at_ns: now_ns,
                    pending_conn_ids,
                    acks: BTreeMap::new(),
                },
            );

            (cut_id, cut_id_string, now_ns, outbound.len(), outbound)
        };

        let request = ServerMessage::CutRequest(moire_types::CutRequest {
            cut_id: cut_id.clone(),
        });
        if let Err(error) =
            persist_cut_request(self.state.db.clone(), cut_id_string.clone(), now_ns).await
        {
            warn!(
                %error,
                cut_id = %cut_id_string,
                "failed to persist cut request"
            );
        }
        let payload = encode_server_message_default(&request)
            .map_err(|error| format!("failed to encode cut request: {error}"))?;
        for (conn_id, tx) in outbound {
            if let Err(error) = tx.try_send(payload.clone()) {
                warn!(
                    conn_id = %conn_id,
                    %error,
                    "failed to enqueue cut request"
                );
            }
        }

        Ok(TriggerCutResponse {
            cut_id,
            requested_at_ns: now_ns,
            requested_connections,
        })
    }

    async fn load_snapshot(
        &self,
        requested_snapshot_id: Option<i64>,
    ) -> Result<SnapshotCutResponse, String> {
        let snapshot_json = {
            let guard = self.state.inner.lock().await;
            match requested_snapshot_id {
                Some(snapshot_id) => guard.snapshot_history_json.get(&snapshot_id).cloned(),
                None => guard.last_snapshot_json.clone(),
            }
        };

        let Some(snapshot_json) = snapshot_json else {
            return match requested_snapshot_id {
                Some(snapshot_id) => Err(format!("unknown snapshot_id {snapshot_id}")),
                None => Err("no snapshot available".to_string()),
            };
        };

        facet_json::from_str::<SnapshotCutResponse>(&snapshot_json)
            .map_err(|error| format!("decode cached snapshot json: {error}"))
    }

    async fn ensure_symbolication_ready(
        &self,
        mut snapshot: SnapshotCutResponse,
    ) -> Result<SnapshotCutResponse, String> {
        if snapshot.backtraces.is_empty() || snapshot.frames.is_empty() {
            return Ok(snapshot);
        }

        if snapshot
            .frames
            .iter()
            .all(|record| !is_pending_frame(&record.frame))
        {
            return Ok(snapshot);
        }

        let backtrace_ids: Vec<BacktraceId> = snapshot
            .backtraces
            .iter()
            .map(|bt| bt.backtrace_id)
            .collect();
        let deadline = tokio::time::Instant::now() + DEFAULT_SYMBOLICATION_WAIT_TIMEOUT;

        loop {
            if let Err(error) =
                symbolicate_pending_frames_for_backtraces(self.state.db.clone(), &backtrace_ids)
                    .await
            {
                warn!(
                    snapshot_id = snapshot.snapshot_id,
                    %error,
                    "symbolication pass failed for MCP"
                );
            }

            let table = load_snapshot_backtrace_table(self.state.db.clone(), &backtrace_ids).await;
            snapshot.backtraces = table.backtraces;
            snapshot.frames = table.frames;
            remember_snapshot(&self.state, &snapshot).await;

            if snapshot
                .frames
                .iter()
                .all(|record| !is_pending_frame(&record.frame))
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(DEFAULT_SYMBOLICATION_WAIT_TICK).await;
        }

        Ok(snapshot)
    }

    fn build_wait_graph(&self, snapshot: &SnapshotCutResponse) -> Result<WaitGraph, String> {
        let backtrace_index = backtrace_index(snapshot);
        let frame_catalog = frame_catalog(snapshot);

        let mut nodes: HashMap<String, WaitNode> = HashMap::new();
        let mut edges: Vec<WaitEdgeRuntime> = Vec::new();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        let mut indegree: HashMap<String, usize> = HashMap::new();
        let mut seen_edges: HashSet<(String, String)> = HashSet::new();

        for process in &snapshot.processes {
            let local_entities: HashMap<String, &Entity> = process
                .snapshot
                .entities
                .iter()
                .map(|entity| (entity.id.as_str().to_owned(), entity))
                .collect();

            for edge in &process.snapshot.edges {
                if edge.kind != EdgeKind::WaitingOn {
                    continue;
                }

                let Some(src) = local_entities.get(edge.src.as_str()) else {
                    return Err(format!(
                        "invariant violated: missing src entity {} for waiting_on edge in process {}",
                        edge.src.as_str(),
                        process.process_id.as_str()
                    ));
                };
                let Some(dst) = local_entities.get(edge.dst.as_str()) else {
                    return Err(format!(
                        "invariant violated: missing dst entity {} for waiting_on edge in process {}",
                        edge.dst.as_str(),
                        process.process_id.as_str()
                    ));
                };

                let src_key = compose_node_key(&process.process_id, &src.id);
                let dst_key = compose_node_key(&process.process_id, &dst.id);

                nodes
                    .entry(src_key.clone())
                    .or_insert_with(|| wait_node(process, src, &backtrace_index, &frame_catalog));
                nodes
                    .entry(dst_key.clone())
                    .or_insert_with(|| wait_node(process, dst, &backtrace_index, &frame_catalog));

                if seen_edges.insert((src_key.clone(), dst_key.clone())) {
                    edges.push(WaitEdgeRuntime {
                        process_id: process.process_id.as_str().to_owned(),
                        src_key: src_key.clone(),
                        dst_key: dst_key.clone(),
                        dst_entity_id: dst.id.as_str().to_owned(),
                        edge_frame_ids: selected_frames_for_backtrace_id(
                            edge.backtrace.as_u64(),
                            &backtrace_index,
                            &frame_catalog,
                            frame_start_index_for_entity(src),
                            SOURCE_FRAMES_PER_ITEM,
                        ),
                    });
                    adjacency
                        .entry(src_key.clone())
                        .or_default()
                        .push(dst_key.clone());
                    *indegree.entry(dst_key).or_insert(0) += 1;
                    indegree.entry(src_key).or_insert(0);
                }
            }
        }

        for outs in adjacency.values_mut() {
            outs.sort();
            outs.dedup();
        }

        Ok((nodes, edges, adjacency, indegree))
    }

    async fn load_source_for_nodes<'a>(
        &self,
        snapshot: &SnapshotCutResponse,
        nodes: impl Iterator<Item = &'a WaitNode>,
    ) -> Result<HashMap<u64, McpSourceContext>, String> {
        let mut frame_ids = BTreeSet::new();
        for node in nodes {
            for frame_id in &node.frame_ids {
                frame_ids.insert(frame_id.as_u64());
            }
        }
        if frame_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let (contexts, _unavailable) = self.resolve_source_contexts(frame_ids).await?;
        let by_frame = contexts
            .into_iter()
            .map(|ctx| (ctx.frame_id, ctx))
            .collect::<HashMap<_, _>>();

        let _ = snapshot;
        Ok(by_frame)
    }

    async fn load_source_for_graph<'a>(
        &self,
        snapshot: &SnapshotCutResponse,
        nodes: impl Iterator<Item = &'a WaitNode>,
        edges: &[WaitEdgeRuntime],
    ) -> Result<HashMap<u64, McpSourceContext>, String> {
        let mut frame_ids = BTreeSet::new();
        for node in nodes {
            for frame_id in &node.frame_ids {
                frame_ids.insert(frame_id.as_u64());
            }
        }
        for edge in edges {
            for frame_id in &edge.edge_frame_ids {
                frame_ids.insert(frame_id.as_u64());
            }
        }
        if frame_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let (contexts, _unavailable) = self.resolve_source_contexts(frame_ids).await?;
        let by_frame = contexts
            .into_iter()
            .map(|ctx| (ctx.frame_id, ctx))
            .collect::<HashMap<_, _>>();
        let _ = snapshot;
        Ok(by_frame)
    }

    async fn resolve_source_contexts(
        &self,
        frame_ids: BTreeSet<u64>,
    ) -> Result<(Vec<McpSourceContext>, Vec<u64>), String> {
        let db = self.state.db.clone();
        tokio::task::spawn_blocking(move || {
            let mut previews = Vec::new();
            let mut unavailable_frame_ids = Vec::new();

            for frame_id_raw in frame_ids {
                let Some((frame_id, module_identity, rel_pc)) =
                    lookup_frame_source_by_raw(frame_id_raw)
                else {
                    unavailable_frame_ids.push(frame_id_raw);
                    continue;
                };

                let Some(location) =
                    lookup_source_text_location_in_db(&db, module_identity, rel_pc)?
                else {
                    unavailable_frame_ids.push(frame_id_raw);
                    continue;
                };

                let statement_text = location.language.and_then(|lang| {
                    extract_target_statement(
                        &location.content,
                        lang,
                        location.target_line,
                        location.target_col,
                    )
                });

                let enclosing_fn_text = location.language.and_then(|lang| {
                    extract_enclosing_fn(
                        &location.content,
                        lang,
                        location.target_line,
                        location.target_col,
                    )
                });

                let (compact_scope_text, compact_scope_range) = location
                    .language
                    .and_then(|lang| {
                        cut_source_compact(
                            &location.content,
                            lang,
                            location.target_line,
                            location.target_col,
                        )
                    })
                    .map(|cut| {
                        (
                            Some(cut.cut_source),
                            Some(McpLineRange {
                                start: cut.scope_range.start,
                                end: cut.scope_range.end,
                            }),
                        )
                    })
                    .unwrap_or((None, None));

                previews.push(McpSourceContext {
                    format: String::from("text/plain"),
                    frame_id: frame_id.as_u64(),
                    source_file: location.source_file,
                    target_line: location.target_line,
                    target_col: location.target_col,
                    total_lines: location.total_lines,
                    statement_text,
                    enclosing_fn_text,
                    compact_scope_text,
                    compact_scope_range,
                });
            }

            Ok::<(Vec<McpSourceContext>, Vec<u64>), String>((previews, unavailable_frame_ids))
        })
        .await
        .map_err(|error| format!("source context worker join error: {error}"))?
    }

    fn resolve_roots(&self, nodes: &HashMap<String, WaitNode>, roots: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        for root in roots {
            for (key, node) in nodes {
                if node.entity_id == *root {
                    out.push(key.clone());
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_wait_paths(
        &self,
        adjacency: &HashMap<String, Vec<String>>,
        _start: &str,
        max_depth: usize,
        path: &mut Vec<String>,
        chains: &mut Vec<McpWaitChain>,
        chain_count: &mut usize,
        nodes: &HashMap<String, WaitNode>,
        sources: &HashMap<u64, McpSourceContext>,
        edge_wait_source: &HashMap<(String, String), Option<McpSourceContext>>,
        edge_wait_sources: &HashMap<(String, String), Vec<McpSourceContext>>,
    ) {
        if chains.len() >= DEFAULT_WAIT_CHAIN_MAX_RESULTS {
            return;
        }

        let Some(current) = path.last().cloned() else {
            return;
        };
        let nexts = adjacency.get(&current).cloned().unwrap_or_default();

        if nexts.is_empty() || path.len() >= max_depth {
            *chain_count += 1;
            chains.push(make_chain(
                *chain_count,
                path,
                false,
                nodes,
                sources,
                edge_wait_source,
                edge_wait_sources,
                path.len() >= max_depth,
            ));
            return;
        }

        for next in nexts {
            if let Some(cycle_start) = path.iter().position(|id| id == &next) {
                let mut cycle_path = path[cycle_start..].to_vec();
                cycle_path.push(next.clone());
                *chain_count += 1;
                chains.push(make_chain(
                    *chain_count,
                    &cycle_path,
                    true,
                    nodes,
                    sources,
                    edge_wait_source,
                    edge_wait_sources,
                    false,
                ));
                if chains.len() >= DEFAULT_WAIT_CHAIN_MAX_RESULTS {
                    return;
                }
                continue;
            }

            path.push(next);
            self.walk_wait_paths(
                adjacency,
                &current,
                max_depth,
                path,
                chains,
                chain_count,
                nodes,
                sources,
                edge_wait_source,
                edge_wait_sources,
            );
            path.pop();

            if chains.len() >= DEFAULT_WAIT_CHAIN_MAX_RESULTS {
                return;
            }
        }
    }

    fn find_entity<'a>(
        &self,
        snapshot: &'a SnapshotCutResponse,
        entity_id: &str,
    ) -> Result<(&'a ProcessSnapshotView, &'a Entity), String> {
        let mut matches: Vec<(&ProcessSnapshotView, &Entity)> = Vec::new();
        for process in &snapshot.processes {
            for entity in &process.snapshot.entities {
                if entity.id.as_str() == entity_id {
                    matches.push((process, entity));
                }
            }
        }

        match matches.len() {
            0 => Err(format!("unknown entity_id `{entity_id}`")),
            1 => Ok(matches.remove(0)),
            n => Err(format!(
                "entity_id `{entity_id}` is ambiguous across {n} processes; expected a unique id"
            )),
        }
    }
}

#[async_trait]
impl ServerHandler for MoireMcpHandler {
    async fn handle_list_tools_request(
        &self,
        _params: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn rust_mcp_sdk::McpServer>,
    ) -> Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: MoireTools::tools(),
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: Arc<dyn rust_mcp_sdk::McpServer>,
    ) -> Result<CallToolResult, CallToolError> {
        let tool_name = params.name.clone();
        let args = params.arguments.unwrap_or_default();
        let response = match self.dispatch_tool(tool_name.as_str(), &args).await {
            Ok(body) => body,
            Err(error) => format!("Error: {error}"),
        };
        Ok(CallToolResult::text_content(vec![response.into()]))
    }
}

pub async fn run_mcp_server(listener: TcpListener, state: AppState) -> Result<(), String> {
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("resolve mcp listener addr: {error}"))?;
    let handler = MoireMcpHandler::new(state);
    let app_state = Arc::new(McpAppState {
        session_store: Arc::new(InMemorySessionStore::new()),
        id_generator: Arc::new(UuidGenerator {}),
        stream_id_gen: Arc::new(FastIdGenerator::new(Some("s_"))),
        server_details: Arc::new(server_details()),
        handler: handler.to_mcp_server_handler(),
        ping_interval: DEFAULT_MCP_PING_INTERVAL,
        transport_options: Arc::new(TransportOptions::default()),
        enable_json_response: false,
        event_store: None,
        task_store: None,
        client_task_store: None,
    });

    let http_handler = Arc::new(McpHttpHandler::new(vec![]));

    let app = Router::new()
        .route(
            DEFAULT_MCP_ENDPOINT,
            get(handle_streamable_http_get)
                .post(handle_streamable_http_post)
                .delete(handle_streamable_http_delete),
        )
        .with_state(app_state)
        .layer(Extension(http_handler));

    info!(
        endpoint = %DEFAULT_MCP_ENDPOINT,
        addr = %local_addr,
        "moire-web MCP Streamable HTTP ready"
    );

    axum::serve(listener, app)
        .await
        .map_err(|error| format!("MCP server failed: {error}"))
}

fn server_details() -> InitializeResult {
    InitializeResult {
        server_info: Implementation {
            name: "moire-web".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            description: Some(
                "Moire runtime graph server with deadlock-focused MCP tools. Run moire_help first."
                    .into(),
            ),
            title: Some("moire-web MCP".into()),
            icons: vec![],
            website_url: Some("https://github.com/bearcove/moire".into()),
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        protocol_version: LATEST_PROTOCOL_VERSION.into(),
        instructions: Some(
            "Run moire_help first. It defines the recommended workflow, entity semantics, and hang patterns. \
Then run moire_cut_fresh and keep using its snapshot_id for all follow-up calls."
                .into(),
        ),
        meta: None,
    }
}

fn render_help_markdown(response: &McpHelpResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Read this first");
    let _ = writeln!(out, "{}", response.read_this_first);
    let _ = writeln!(out);

    let _ = writeln!(out, "Recommended workflow");
    for step in &response.first_steps {
        let _ = writeln!(out, "- {step}");
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "Tool guide");
    for guide in &response.tool_guide {
        let _ = writeln!(out, "- {}: {}", guide.tool, guide.purpose);
        let _ = writeln!(out, "  when: {}", guide.when_to_use);
        let _ = writeln!(out, "  args: {}", guide.typical_args);
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "Entity kinds");
    for kind in &response.entity_kinds {
        let _ = writeln!(
            out,
            "- {}: {} (hang signal: {})",
            kind.kind, kind.means, kind.hang_signal
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "Typical hang patterns");
    for pattern in &response.hang_patterns {
        let _ = writeln!(out, "- {}", pattern.name);
        let _ = writeln!(out, "  signature: {}", pattern.signature);
        let _ = writeln!(out, "  likely cause: {}", pattern.likely_cause);
        let _ = writeln!(out, "  next calls:");
        for next in &pattern.next_calls {
            let _ = writeln!(out, "    - {next}");
        }
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "Interpretation notes");
    for note in &response.interpretation_notes {
        let _ = writeln!(out, "- {note}");
    }
    out.trim_end().to_string()
}

fn render_cut_fresh_markdown(response: &McpCutFreshResponse) -> String {
    format!(
        "cut_id: {}\nsnapshot_id: {}\ncaptured_at_unix_ms: {}\nrequested_connections: {}\nprocess_count: {}\ntimed_out_count: {}",
        response.cut_id.as_str(),
        response.snapshot_id,
        response.captured_at_unix_ms,
        response.requested_connections,
        response.process_count,
        response.timed_out_count
    )
}

fn render_wait_edges_markdown(response: &McpWaitEdgesResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "snapshot_id: {}", response.snapshot_id);
    let _ = writeln!(out, "wait_edges: {}", response.row_count);

    for (idx, edge) in response.wait_edges.iter().enumerate() {
        let _ = writeln!(
            out,
            "\n{}. {} [{}] -> {} [{}]",
            idx + 1,
            edge.waiter_name,
            edge.waiter_kind,
            edge.blocked_on_name,
            edge.blocked_on_kind
        );
        let _ = writeln!(
            out,
            "   ids: {} -> {} (process {})",
            edge.waiter_id, edge.blocked_on_id, edge.process_id
        );
        let _ = writeln!(
            out,
            "   births_ms: waiter={} blocked={}",
            edge.waiter_birth_ms, edge.blocked_birth_ms
        );
        append_source_set(
            &mut out,
            "   waiter sources",
            edge.waiter_source.as_ref(),
            &edge.waiter_sources,
            "   ",
        );
        append_source_set(
            &mut out,
            "   blocked_on sources",
            edge.blocked_on_source.as_ref(),
            &edge.blocked_on_sources,
            "   ",
        );
        append_source_set(
            &mut out,
            "   wait-site sources",
            edge.wait_site_source.as_ref(),
            &edge.wait_site_sources,
            "   ",
        );
    }
    out.trim_end().to_string()
}

fn render_wait_chains_markdown(response: &McpWaitChainsResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "snapshot_id: {}", response.snapshot_id);
    let _ = writeln!(out, "chains: {}", response.chain_count);

    for chain in &response.chains {
        let cycle = if chain.is_cycle { "cycle" } else { "acyclic" };
        let wake = if chain.has_external_wake_source {
            "has external wake source"
        } else {
            "no external wake source"
        };
        let _ = writeln!(
            out,
            "\n{} ({}, {}): {}",
            chain.chain_id, cycle, wake, chain.summary
        );
        if !chain.nodes.is_empty() {
            let path = chain
                .nodes
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>()
                .join(" -> ");
            let _ = writeln!(out, "path: {path}");
        }

        let _ = writeln!(out, "nodes:");
        for node in &chain.nodes {
            let _ = writeln!(
                out,
                "- {} [{}] id={} process={}",
                node.name, node.kind, node.entity_id, node.process_id
            );
            append_source_set(
                &mut out,
                "  sources",
                node.source.as_ref(),
                &node.sources,
                "  ",
            );
        }

        let _ = writeln!(out, "edges:");
        for edge in &chain.edges {
            let _ = writeln!(out, "- {} -> {}", edge.src_entity_id, edge.dst_entity_id);
            append_source_set(
                &mut out,
                "  wait-site sources",
                edge.wait_site_source.as_ref(),
                &edge.wait_site_sources,
                "  ",
            );
        }
    }

    out.trim_end().to_string()
}

fn render_deadlock_candidates_markdown(response: &McpDeadlockCandidatesResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "snapshot_id: {}", response.snapshot_id);
    let _ = writeln!(out, "candidates: {}", response.candidate_count);

    for candidate in &response.candidates {
        let _ = writeln!(
            out,
            "\n{}: confidence={}",
            candidate.candidate_id, candidate.confidence
        );
        let _ = writeln!(out, "reasons: {}", candidate.reasons.join(", "));
        let _ = writeln!(out, "entity_ids: {}", candidate.entity_ids.join(", "));
        if let Some(duration) = candidate.blocked_duration_hint_ms {
            let _ = writeln!(out, "blocked_duration_hint_ms: {duration}");
        }
        for node in &candidate.cycle_nodes {
            let _ = writeln!(out, "- {} [{}] id={}", node.name, node.kind, node.entity_id);
            append_source_set(
                &mut out,
                "  sources",
                node.source.as_ref(),
                &node.sources,
                "  ",
            );
        }
    }

    out.trim_end().to_string()
}

fn render_entity_markdown(response: &McpEntityResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "snapshot_id: {}", response.snapshot_id);
    let _ = writeln!(
        out,
        "entity: {} [{}]",
        response.entity_name, response.entity_kind
    );
    let _ = writeln!(out, "entity_id: {}", response.entity_id);
    let _ = writeln!(
        out,
        "process: {} (pid {}, id {})",
        response.process_name, response.pid, response.process_id
    );
    let _ = writeln!(out, "scope_ids: {}", response.scope_ids.join(", "));
    let _ = writeln!(out, "entity_body: {}", response.entity_body_json);
    append_source_set(
        &mut out,
        "sources",
        response.source.as_ref(),
        &response.sources,
        "",
    );

    let _ = writeln!(out, "\nincoming_wait_edges:");
    for edge in &response.incoming_wait_edges {
        let _ = writeln!(out, "- {} -> {}", edge.src_entity_id, edge.dst_entity_id);
        append_source_set(
            &mut out,
            "  wait-site sources",
            edge.wait_site_source.as_ref(),
            &edge.wait_site_sources,
            "  ",
        );
    }

    let _ = writeln!(out, "\noutgoing_wait_edges:");
    for edge in &response.outgoing_wait_edges {
        let _ = writeln!(out, "- {} -> {}", edge.src_entity_id, edge.dst_entity_id);
        append_source_set(
            &mut out,
            "  wait-site sources",
            edge.wait_site_source.as_ref(),
            &edge.wait_site_sources,
            "  ",
        );
    }

    out.trim_end().to_string()
}

fn render_channel_state_markdown(response: &McpChannelStateResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "snapshot_id: {}", response.snapshot_id);
    let _ = writeln!(out, "channels: {}", response.channels.len());

    for channel in &response.channels {
        let _ = writeln!(
            out,
            "\n- {} [{}] id={} process={}",
            channel.name, channel.channel_kind, channel.entity_id, channel.process_id
        );
        if let Some(capacity) = channel.capacity {
            let _ = writeln!(out, "  capacity: {capacity}");
        }
        if let Some(occupancy) = channel.occupancy {
            let _ = writeln!(out, "  occupancy: {occupancy}");
        }
        let _ = writeln!(
            out,
            "  waiters: senders={} receivers={}",
            channel.sender_waiters, channel.receiver_waiters
        );
        if let Some(hints) = channel.lifecycle_hints.as_ref() {
            let _ = writeln!(out, "  lifecycle: {hints}");
        }
        append_source_set(
            &mut out,
            "  sources",
            channel.source.as_ref(),
            &channel.sources,
            "  ",
        );
    }
    out.trim_end().to_string()
}

fn render_task_state_markdown(response: &McpTaskStateResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "snapshot_id: {}", response.snapshot_id);
    let _ = writeln!(out, "tasks: {}", response.tasks.len());

    for task in &response.tasks {
        let _ = writeln!(
            out,
            "\n- {} id={} process={}",
            task.name, task.entity_id, task.process_id
        );
        let _ = writeln!(out, "  entry_backtrace_id: {}", task.entry_backtrace_id);
        if !task.entry_frame_ids.is_empty() {
            let frame_ids = task
                .entry_frame_ids
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(out, "  entry_frame_ids: {frame_ids}");
        } else if let Some(frame_id) = task.entry_frame_id {
            let _ = writeln!(out, "  entry_frame_id: {frame_id}");
        }
        if let Some(awaiting) = task.awaiting_on_entity_id.as_ref() {
            let _ = writeln!(out, "  awaiting_on_entity_id: {awaiting}");
        }
        let _ = writeln!(out, "  scope_ids: {}", task.scope_ids.join(", "));
        append_source_set(
            &mut out,
            "  sources",
            task.source.as_ref(),
            &task.sources,
            "  ",
        );
    }

    out.trim_end().to_string()
}

fn render_source_context_markdown(response: &McpSourceContextResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "snapshot_id: {}", response.snapshot_id);
    let _ = writeln!(out, "format: {}", response.format);
    let _ = writeln!(out, "previews: {}", response.previews.len());

    for preview in &response.previews {
        let _ = writeln!(out, "\nframe_id: {}", preview.frame_id);
        append_rendered_block(&mut out, &render_source_snippet(preview), "");
    }

    if !response.unavailable_frame_ids.is_empty() {
        let ids = response
            .unavailable_frame_ids
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "\nunavailable_frame_ids: [{ids}]");
    }
    out.trim_end().to_string()
}

fn render_backtrace_markdown(response: &McpBacktraceResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "snapshot_id: {}", response.snapshot_id);
    let _ = writeln!(out, "backtrace_id: {}", response.backtrace_id);
    let _ = writeln!(out, "frame_count: {}", response.frame_count);

    for frame in &response.frames {
        let _ = writeln!(out, "\n- frame {} [{}]", frame.frame_id, frame.status);
        if let Some(function_name) = frame.function_name.as_ref() {
            let _ = writeln!(out, "  function: {function_name}");
        }
        if let Some(source_file) = frame.source_file.as_ref() {
            if let Some(line) = frame.line {
                let _ = writeln!(out, "  source: {source_file}:{line}");
            } else {
                let _ = writeln!(out, "  source: {source_file}");
            }
        } else {
            let _ = writeln!(out, "  module: {}", frame.module_path);
        }
        if let Some(reason) = frame.reason.as_ref() {
            let _ = writeln!(out, "  unresolved_reason: {reason}");
        }
        if let Some(source) = frame.source.as_ref() {
            append_source_set(&mut out, "  context", Some(source), &[], "  ");
        }
    }
    out.trim_end().to_string()
}

fn render_diff_snapshots_markdown(response: &McpDiffSnapshotsResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "from_snapshot_id: {}\nto_snapshot_id: {}",
        response.from_snapshot_id, response.to_snapshot_id
    );
    let _ = writeln!(out, "entity_added: {}", response.entity_added.join(", "));
    let _ = writeln!(
        out,
        "entity_removed: {}",
        response.entity_removed.join(", ")
    );
    let _ = writeln!(
        out,
        "waiting_on_added: {}",
        response.waiting_on_added.join(", ")
    );
    let _ = writeln!(
        out,
        "waiting_on_removed: {}",
        response.waiting_on_removed.join(", ")
    );

    let _ = writeln!(out, "\nchannel_changes:");
    for change in &response.channel_changes {
        let _ = writeln!(
            out,
            "- {}: {} -> {}",
            change.entity_id, change.before, change.after
        );
    }

    let _ = writeln!(out, "\ntask_changes:");
    for change in &response.task_changes {
        let before = change.awaiting_before.as_deref().unwrap_or("<none>");
        let after = change.awaiting_after.as_deref().unwrap_or("<none>");
        let _ = writeln!(out, "- {}: {} -> {}", change.entity_id, before, after);
    }

    out.trim_end().to_string()
}

fn append_source_set(
    out: &mut String,
    label: &str,
    source: Option<&McpSourceContext>,
    sources: &[McpSourceContext],
    indent: &str,
) {
    let mut all = Vec::<McpSourceContext>::new();
    if let Some(source) = source {
        all.push(source.clone());
    }
    for source in sources {
        if all.iter().any(|seen| seen.frame_id == source.frame_id) {
            continue;
        }
        all.push(source.clone());
    }

    let _ = writeln!(out, "{indent}{label}:");
    if all.is_empty() {
        let _ = writeln!(out, "{indent}  <source unavailable>");
        return;
    }
    for (idx, source) in all.iter().enumerate() {
        let _ = writeln!(out, "{indent}  [{}] frame_id={}", idx + 1, source.frame_id);
        append_rendered_block(
            out,
            &render_source_snippet(source),
            &format!("{indent}    "),
        );
    }
}

fn append_rendered_block(out: &mut String, block: &str, indent: &str) {
    for line in block.lines() {
        let _ = writeln!(out, "{indent}{line}");
    }
}

fn render_source_snippet(source: &McpSourceContext) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "in {}", source.source_file);

    let (start_line, body) = source_body_for_render(source);
    let mut rendered = 0usize;
    let mut truncated = false;
    for (index, raw_line) in body.lines().enumerate() {
        if rendered >= MAX_RENDERED_SOURCE_LINES {
            truncated = true;
            break;
        }
        let line = raw_line.trim_end();
        if line.is_empty() {
            continue;
        }
        let line_no = start_line.saturating_add(index as u32);
        let marker = if line_no == source.target_line {
            '>'
        } else {
            ' '
        };
        let _ = writeln!(out, "{marker}{line_no:>5} | {line}");
        rendered = rendered.saturating_add(1);
    }

    if rendered == 0 {
        let _ = writeln!(
            out,
            ">{:>5} | <source text unavailable>",
            source.target_line
        );
    }
    if truncated {
        let _ = writeln!(out, "  ...  | ...");
    }

    out.trim_end().to_string()
}

fn source_body_for_render(source: &McpSourceContext) -> (u32, String) {
    if let (Some(compact_scope_text), Some(range)) = (
        source.compact_scope_text.as_ref(),
        source.compact_scope_range.as_ref(),
    ) && compact_target_line_visible(compact_scope_text, range, source.target_line)
    {
        return (range.start, compact_scope_text.clone());
    }
    if let Some(statement_text) = source.statement_text.as_ref()
        && !statement_text.trim().is_empty()
    {
        return (source.target_line, statement_text.clone());
    }
    if let Some(enclosing_fn_text) = source.enclosing_fn_text.as_ref()
        && !enclosing_fn_text.trim().is_empty()
    {
        return (source.target_line, enclosing_fn_text.clone());
    }
    (source.target_line, String::new())
}

fn compact_target_line_visible(text: &str, range: &McpLineRange, target_line: u32) -> bool {
    if target_line < range.start || target_line > range.end {
        return false;
    }
    let index = target_line.saturating_sub(range.start) as usize;
    let Some(line) = text.lines().nth(index) else {
        return false;
    };
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed != "/* ... */"
}

fn source_for_node(
    node: &WaitNode,
    sources: &HashMap<u64, McpSourceContext>,
) -> Option<McpSourceContext> {
    sources_for_node(node, sources).into_iter().next()
}

fn sources_for_node(
    node: &WaitNode,
    sources: &HashMap<u64, McpSourceContext>,
) -> Vec<McpSourceContext> {
    sources_for_frame_ids(&node.frame_ids, sources)
}

fn sources_for_frame_ids(
    frame_ids: &[FrameId],
    sources: &HashMap<u64, McpSourceContext>,
) -> Vec<McpSourceContext> {
    frame_ids
        .iter()
        .filter_map(|id| sources.get(&id.as_u64()).cloned())
        .collect()
}

fn wait_node(
    process: &ProcessSnapshotView,
    entity: &Entity,
    backtrace_index: &HashMap<u64, &SnapshotBacktrace>,
    frame_catalog: &HashMap<u64, &SnapshotBacktraceFrame>,
) -> WaitNode {
    let frame_ids = selected_frames_for_entity(
        entity,
        backtrace_index,
        frame_catalog,
        SOURCE_FRAMES_PER_ITEM,
    );
    WaitNode {
        process_id: process.process_id.as_str().to_owned(),
        ptime_now_ms: process.ptime_now_ms,
        entity_id: entity.id.as_str().to_owned(),
        name: entity.name.clone(),
        kind: entity_kind_name(&entity.body).to_owned(),
        birth_ms: entity.birth.as_millis(),
        frame_ids,
    }
}

fn compose_node_key(process_id: &ProcessId, entity_id: &EntityId) -> String {
    format!("{}::{}", process_id.as_str(), entity_id.as_str())
}

fn backtrace_index(snapshot: &SnapshotCutResponse) -> HashMap<u64, &SnapshotBacktrace> {
    snapshot
        .backtraces
        .iter()
        .map(|bt| (bt.backtrace_id.as_u64(), bt))
        .collect()
}

fn frame_catalog(snapshot: &SnapshotCutResponse) -> HashMap<u64, &SnapshotBacktraceFrame> {
    snapshot
        .frames
        .iter()
        .map(|record| (record.frame_id.as_u64(), &record.frame))
        .collect()
}

fn frame_start_index_for_entity(entity: &Entity) -> usize {
    match &entity.body {
        EntityBody::Future(fut) => fut.skip_entry_frames.unwrap_or(0) as usize,
        _ => 0,
    }
}

fn selected_frames_for_entity(
    entity: &Entity,
    backtrace_index: &HashMap<u64, &SnapshotBacktrace>,
    frame_catalog: &HashMap<u64, &SnapshotBacktraceFrame>,
    frame_count: usize,
) -> Vec<FrameId> {
    let Some(backtrace) = backtrace_index.get(&entity.backtrace.as_u64()) else {
        return Vec::new();
    };
    select_frames_for_backtrace(
        backtrace,
        frame_catalog,
        frame_start_index_for_entity(entity),
        frame_count,
    )
}

fn selected_frames_for_backtrace_id(
    backtrace_id: u64,
    backtrace_index: &HashMap<u64, &SnapshotBacktrace>,
    frame_catalog: &HashMap<u64, &SnapshotBacktraceFrame>,
    app_skip_count: usize,
    frame_count: usize,
) -> Vec<FrameId> {
    let Some(backtrace) = backtrace_index.get(&backtrace_id) else {
        return Vec::new();
    };
    select_frames_for_backtrace(backtrace, frame_catalog, app_skip_count, frame_count)
}

fn select_frames_for_backtrace(
    backtrace: &SnapshotBacktrace,
    frame_catalog: &HashMap<u64, &SnapshotBacktraceFrame>,
    app_skip_count: usize,
    frame_count: usize,
) -> Vec<FrameId> {
    if backtrace.frame_ids.is_empty() || frame_count == 0 {
        return Vec::new();
    }

    let mut application_resolved = Vec::<FrameId>::new();
    let mut resolved_all = Vec::<FrameId>::new();
    for frame_id in &backtrace.frame_ids {
        let Some(frame) = frame_catalog.get(&frame_id.as_u64()) else {
            continue;
        };
        let SnapshotBacktraceFrame::Resolved(resolved) = frame else {
            continue;
        };
        resolved_all.push(*frame_id);
        let is_system =
            crate_from_function_name(resolved.function_name.as_str()).is_some_and(is_system_crate);
        if !is_system {
            application_resolved.push(*frame_id);
        }
    }

    let mut out = Vec::<FrameId>::new();
    for frame_id in application_resolved.into_iter().skip(app_skip_count) {
        if !out.contains(&frame_id) {
            out.push(frame_id);
        }
        if out.len() >= frame_count {
            return out;
        }
    }
    for frame_id in resolved_all {
        if !out.contains(&frame_id) {
            out.push(frame_id);
        }
        if out.len() >= frame_count {
            return out;
        }
    }
    for frame_id in &backtrace.frame_ids {
        if !out.contains(frame_id) {
            out.push(*frame_id);
        }
        if out.len() >= frame_count {
            return out;
        }
    }
    out
}

fn crate_from_function_name(function_name: &str) -> Option<&str> {
    let trimmed = function_name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let ch = bytes[index] as char;
        if ch == '<' || ch == ' ' || ch == '&' || ch == '*' {
            index += 1;
            continue;
        }
        break;
    }

    let rest = &trimmed[index..];
    let mut chars = rest.char_indices();
    let (_, first) = chars.next()?;
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return None;
    }
    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            end = idx + ch.len_utf8();
            continue;
        }
        break;
    }
    Some(&rest[..end])
}

fn is_system_crate(krate: &str) -> bool {
    SYSTEM_CRATES.contains(&krate)
}

#[allow(clippy::too_many_arguments)]
fn make_chain(
    chain_num: usize,
    path: &[String],
    is_cycle: bool,
    nodes: &HashMap<String, WaitNode>,
    sources: &HashMap<u64, McpSourceContext>,
    edge_wait_source: &HashMap<(String, String), Option<McpSourceContext>>,
    edge_wait_sources: &HashMap<(String, String), Vec<McpSourceContext>>,
    truncated: bool,
) -> McpWaitChain {
    let mut chain_nodes = Vec::new();
    let mut node_ids = Vec::new();
    for key in path {
        if let Some(node) = nodes.get(key) {
            node_ids.push(node.entity_id.clone());
            chain_nodes.push(McpNodeSummary {
                process_id: node.process_id.clone(),
                entity_id: node.entity_id.clone(),
                name: node.name.clone(),
                kind: node.kind.clone(),
                source: source_for_node(node, sources),
                sources: sources_for_node(node, sources),
            });
        }
    }

    let mut edges = Vec::new();
    for pair in path.windows(2) {
        let Some(src) = nodes.get(&pair[0]) else {
            continue;
        };
        let Some(dst) = nodes.get(&pair[1]) else {
            continue;
        };
        edges.push(McpChainEdge {
            src_entity_id: src.entity_id.clone(),
            dst_entity_id: dst.entity_id.clone(),
            wait_site_source: edge_wait_source
                .get(&(pair[0].clone(), pair[1].clone()))
                .cloned()
                .flatten(),
            wait_site_sources: edge_wait_sources
                .get(&(pair[0].clone(), pair[1].clone()))
                .cloned()
                .unwrap_or_default(),
        });
    }

    let has_external_wake_source = chain_nodes
        .iter()
        .any(|node| node_has_external_wake_source(node.kind.as_str()));

    let summary = if is_cycle {
        format!("cycle of {} nodes", chain_nodes.len())
    } else if truncated {
        format!(
            "chain truncated at {} nodes (max depth reached)",
            chain_nodes.len()
        )
    } else {
        format!("chain of {} nodes", chain_nodes.len())
    };

    McpWaitChain {
        chain_id: format!("chain-{chain_num}"),
        is_cycle,
        has_external_wake_source,
        summary,
        node_ids,
        edges,
        nodes: chain_nodes,
    }
}

fn node_has_external_wake_source(kind: &str) -> bool {
    matches!(
        kind,
        "mpsc_rx"
            | "broadcast_rx"
            | "watch_rx"
            | "oneshot_rx"
            | "notify"
            | "semaphore"
            | "net_accept"
            | "net_read"
            | "request"
            | "response"
    )
}

fn count_waiters(
    edges: &[WaitEdgeRuntime],
    nodes: &HashMap<String, WaitNode>,
    channel_entity: &Entity,
) -> (u32, u32) {
    let mut sender_waiters = 0u32;
    let mut receiver_waiters = 0u32;

    for edge in edges {
        if edge.dst_entity_id != channel_entity.id.as_str() {
            continue;
        }
        let Some(waiter) = nodes.get(&edge.src_key) else {
            continue;
        };
        if waiter.name.contains(".send") {
            sender_waiters = sender_waiters.saturating_add(1);
        } else {
            receiver_waiters = receiver_waiters.saturating_add(1);
        }
    }

    (sender_waiters, receiver_waiters)
}

fn entity_kind_name(body: &EntityBody) -> &'static str {
    match body {
        EntityBody::Future(_) => "future",
        EntityBody::Lock(_) => "lock",
        EntityBody::MpscTx(_) => "mpsc_tx",
        EntityBody::MpscRx(_) => "mpsc_rx",
        EntityBody::BroadcastTx(_) => "broadcast_tx",
        EntityBody::BroadcastRx(_) => "broadcast_rx",
        EntityBody::WatchTx(_) => "watch_tx",
        EntityBody::WatchRx(_) => "watch_rx",
        EntityBody::OneshotTx(_) => "oneshot_tx",
        EntityBody::OneshotRx(_) => "oneshot_rx",
        EntityBody::Semaphore(_) => "semaphore",
        EntityBody::Notify(_) => "notify",
        EntityBody::OnceCell(_) => "once_cell",
        EntityBody::Command(_) => "command",
        EntityBody::FileOp(_) => "file_op",
        EntityBody::NetConnect(_) => "net_connect",
        EntityBody::NetAccept(_) => "net_accept",
        EntityBody::NetRead(_) => "net_read",
        EntityBody::NetWrite(_) => "net_write",
        EntityBody::Request(_) => "request",
        EntityBody::Response(_) => "response",
        EntityBody::Custom(_) => "custom",
        EntityBody::Aether(_) => "aether",
    }
}

fn is_channel_entity(body: &EntityBody) -> bool {
    matches!(
        body,
        EntityBody::MpscTx(_)
            | EntityBody::MpscRx(_)
            | EntityBody::BroadcastTx(_)
            | EntityBody::BroadcastRx(_)
            | EntityBody::WatchTx(_)
            | EntityBody::WatchRx(_)
            | EntityBody::OneshotTx(_)
            | EntityBody::OneshotRx(_)
    )
}

fn is_task_entity(body: &EntityBody) -> bool {
    matches!(body, EntityBody::Future(_))
}

fn channel_metrics(body: &EntityBody) -> (Option<u32>, Option<u32>, Option<String>, &'static str) {
    match body {
        EntityBody::MpscTx(tx) => (
            tx.capacity,
            Some(tx.queue_len),
            Some(format!(
                "queue_len={} capacity={:?}",
                tx.queue_len, tx.capacity
            )),
            "mpsc",
        ),
        EntityBody::MpscRx(_) => (None, None, None, "mpsc"),
        EntityBody::BroadcastTx(tx) => (
            Some(tx.capacity),
            None,
            Some(format!("capacity={}", tx.capacity)),
            "broadcast",
        ),
        EntityBody::BroadcastRx(rx) => (
            None,
            Some(rx.lag),
            Some(format!("lag={}", rx.lag)),
            "broadcast",
        ),
        EntityBody::WatchTx(tx) => (
            None,
            None,
            Some(format!(
                "last_update_at_ms={:?}",
                tx.last_update_at.map(|t| t.as_millis())
            )),
            "watch",
        ),
        EntityBody::WatchRx(_) => (None, None, None, "watch"),
        EntityBody::OneshotTx(tx) => (
            Some(1),
            Some(if tx.sent { 1 } else { 0 }),
            Some(format!("sent={}", tx.sent)),
            "oneshot",
        ),
        EntityBody::OneshotRx(_) => (Some(1), None, None, "oneshot"),
        _ => (None, None, None, "unknown"),
    }
}

fn snapshot_entity_keys(snapshot: &SnapshotCutResponse) -> HashSet<String> {
    let mut out = HashSet::new();
    for process in &snapshot.processes {
        for entity in &process.snapshot.entities {
            out.insert(compose_node_key(&process.process_id, &entity.id));
        }
    }
    out
}

fn snapshot_waiting_edges(snapshot: &SnapshotCutResponse) -> HashSet<String> {
    let mut out = HashSet::new();
    for process in &snapshot.processes {
        for edge in &process.snapshot.edges {
            if edge.kind == EdgeKind::WaitingOn {
                out.insert(format!(
                    "{}::{}->{}",
                    process.process_id.as_str(),
                    edge.src.as_str(),
                    edge.dst.as_str()
                ));
            }
        }
    }
    out
}

fn snapshot_channel_fingerprint(snapshot: &SnapshotCutResponse) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for process in &snapshot.processes {
        for entity in &process.snapshot.entities {
            if !is_channel_entity(&entity.body) {
                continue;
            }
            let key = compose_node_key(&process.process_id, &entity.id);
            let value = facet_json::to_string(&entity.body)
                .unwrap_or_else(|_| String::from("<encode_error>"));
            out.insert(key, value);
        }
    }
    out
}

fn snapshot_task_wait_target(snapshot: &SnapshotCutResponse) -> HashMap<String, Option<String>> {
    let mut out = HashMap::new();
    for process in &snapshot.processes {
        let mut waiting_by_src: HashMap<String, String> = HashMap::new();
        for edge in &process.snapshot.edges {
            if edge.kind == EdgeKind::WaitingOn {
                waiting_by_src.insert(edge.src.as_str().to_owned(), edge.dst.as_str().to_owned());
            }
        }
        for entity in &process.snapshot.entities {
            if !is_task_entity(&entity.body) {
                continue;
            }
            let key = compose_node_key(&process.process_id, &entity.id);
            out.insert(key, waiting_by_src.get(entity.id.as_str()).cloned());
        }
    }
    out
}

fn strongly_connected_components(
    keys: Vec<String>,
    adjacency: &HashMap<String, Vec<String>>,
) -> Vec<Vec<String>> {
    struct TarjanState {
        index: usize,
        stack: Vec<String>,
        on_stack: HashSet<String>,
        index_map: HashMap<String, usize>,
        lowlink_map: HashMap<String, usize>,
        components: Vec<Vec<String>>,
    }

    fn strongconnect(node: String, adjacency: &HashMap<String, Vec<String>>, st: &mut TarjanState) {
        st.index_map.insert(node.clone(), st.index);
        st.lowlink_map.insert(node.clone(), st.index);
        st.index += 1;
        st.stack.push(node.clone());
        st.on_stack.insert(node.clone());

        if let Some(neighbors) = adjacency.get(&node) {
            for next in neighbors {
                if !st.index_map.contains_key(next) {
                    strongconnect(next.clone(), adjacency, st);
                    let next_low = st.lowlink_map.get(next).copied().unwrap_or(usize::MAX);
                    if let Some(node_low) = st.lowlink_map.get_mut(&node) {
                        *node_low = (*node_low).min(next_low);
                    }
                } else if st.on_stack.contains(next) {
                    let next_idx = st.index_map.get(next).copied().unwrap_or(usize::MAX);
                    if let Some(node_low) = st.lowlink_map.get_mut(&node) {
                        *node_low = (*node_low).min(next_idx);
                    }
                }
            }
        }

        let node_idx = st.index_map.get(&node).copied().unwrap_or(usize::MAX);
        let node_low = st.lowlink_map.get(&node).copied().unwrap_or(usize::MAX);
        if node_low == node_idx {
            let mut component = Vec::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack.remove(&w);
                component.push(w.clone());
                if w == node {
                    break;
                }
            }
            st.components.push(component);
        }
    }

    let mut state = TarjanState {
        index: 0,
        stack: Vec::new(),
        on_stack: HashSet::new(),
        index_map: HashMap::new(),
        lowlink_map: HashMap::new(),
        components: Vec::new(),
    };

    for node in keys {
        if !state.index_map.contains_key(&node) {
            strongconnect(node, adjacency, &mut state);
        }
    }

    state.components
}

fn required_non_empty_string(
    args: &JsonMap<String, JsonValue>,
    field: &str,
) -> Result<String, String> {
    let value = args
        .get(field)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| format!("missing required `{field}` string argument"))?
        .trim()
        .to_string();
    if value.is_empty() {
        return Err(format!("`{field}` must not be empty"));
    }
    Ok(value)
}

fn optional_non_empty_string(
    args: &JsonMap<String, JsonValue>,
    field: &str,
) -> Result<Option<String>, String> {
    let Some(value) = args.get(field) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(format!("`{field}` must be a string"));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("`{field}` must not be empty when provided"));
    }
    Ok(Some(value.to_owned()))
}

fn optional_string_list(
    args: &JsonMap<String, JsonValue>,
    field: &str,
) -> Result<Option<Vec<String>>, String> {
    let Some(raw) = args.get(field) else {
        return Ok(None);
    };
    let values = raw
        .as_array()
        .ok_or_else(|| format!("`{field}` must be an array of strings"))?;
    let mut out = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let Some(text) = value.as_str() else {
            return Err(format!("`{field}[{index}]` must be a string"));
        };
        let text = text.trim();
        if text.is_empty() {
            return Err(format!("`{field}[{index}]` must not be empty"));
        }
        out.push(text.to_owned());
    }
    Ok(Some(out))
}

fn optional_u32(args: &JsonMap<String, JsonValue>, field: &str) -> Result<Option<u32>, String> {
    let Some(raw) = args.get(field) else {
        return Ok(None);
    };
    let raw = raw
        .as_u64()
        .ok_or_else(|| format!("`{field}` must be an unsigned integer"))?;
    u32::try_from(raw)
        .map(Some)
        .map_err(|_| format!("`{field}` value {raw} exceeds u32::MAX"))
}

fn optional_bool(args: &JsonMap<String, JsonValue>, field: &str) -> Result<Option<bool>, String> {
    let Some(raw) = args.get(field) else {
        return Ok(None);
    };
    raw.as_bool()
        .map(Some)
        .ok_or_else(|| format!("`{field}` must be a boolean"))
}

fn optional_i64(args: &JsonMap<String, JsonValue>, field: &str) -> Result<Option<i64>, String> {
    let Some(raw) = args.get(field) else {
        return Ok(None);
    };
    raw.as_i64()
        .map(Some)
        .ok_or_else(|| format!("`{field}` must be an integer"))
}

fn required_i64(args: &JsonMap<String, JsonValue>, field: &str) -> Result<i64, String> {
    args.get(field)
        .and_then(JsonValue::as_i64)
        .ok_or_else(|| format!("missing required `{field}` integer argument"))
}

fn required_u64(args: &JsonMap<String, JsonValue>, field: &str) -> Result<u64, String> {
    let value = args
        .get(field)
        .and_then(JsonValue::as_u64)
        .ok_or_else(|| format!("missing required `{field}` unsigned integer argument"))?;
    Ok(value)
}

fn required_u64_list(args: &JsonMap<String, JsonValue>, field: &str) -> Result<Vec<u64>, String> {
    let values = args
        .get(field)
        .and_then(JsonValue::as_array)
        .ok_or_else(|| format!("missing required `{field}` array argument"))?;
    let mut out = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let numeric = value
            .as_u64()
            .ok_or_else(|| format!("`{field}[{index}]` must be an unsigned integer"))?;
        out.push(numeric);
    }
    Ok(out)
}

async fn handle_streamable_http_get(
    headers: HeaderMap,
    uri: Uri,
    State(state): State<Arc<McpAppState>>,
    Extension(http_handler): Extension<Arc<McpHttpHandler>>,
) -> Result<Response, TransportServerError> {
    let request = McpHttpHandler::create_request(Method::GET, uri, headers, None);
    let generic_response = http_handler.handle_streamable_http(request, state).await?;
    Ok(convert_response(generic_response))
}

async fn handle_streamable_http_post(
    headers: HeaderMap,
    uri: Uri,
    State(state): State<Arc<McpAppState>>,
    Extension(http_handler): Extension<Arc<McpHttpHandler>>,
    payload: String,
) -> Result<Response, TransportServerError> {
    let request =
        McpHttpHandler::create_request(Method::POST, uri, headers, Some(payload.as_str()));
    let generic_response = http_handler.handle_streamable_http(request, state).await?;
    Ok(convert_response(generic_response))
}

async fn handle_streamable_http_delete(
    headers: HeaderMap,
    uri: Uri,
    State(state): State<Arc<McpAppState>>,
    Extension(http_handler): Extension<Arc<McpHttpHandler>>,
) -> Result<Response, TransportServerError> {
    let request = McpHttpHandler::create_request(Method::DELETE, uri, headers, None);
    let generic_response = http_handler.handle_streamable_http(request, state).await?;
    Ok(convert_response(generic_response))
}

fn convert_response(response: axum::http::Response<GenericBody>) -> Response {
    let (parts, body) = response.into_parts();
    Response::from_parts(parts, axum::body::Body::new(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strongly_connected_components_finds_cycle_cluster() {
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        adjacency.insert(String::from("a"), vec![String::from("b")]);
        adjacency.insert(String::from("b"), vec![String::from("c")]);
        adjacency.insert(String::from("c"), vec![String::from("a")]);
        adjacency.insert(String::from("d"), vec![String::from("e")]);
        adjacency.insert(String::from("e"), vec![]);
        let keys = vec![
            String::from("a"),
            String::from("b"),
            String::from("c"),
            String::from("d"),
            String::from("e"),
        ];

        let mut components = strongly_connected_components(keys, &adjacency);
        components.iter_mut().for_each(|c| c.sort());
        components.sort_by_key(|c| c.first().cloned().unwrap_or_default());

        assert_eq!(components.len(), 3);
        assert_eq!(
            components[0],
            vec![String::from("a"), String::from("b"), String::from("c")]
        );
        assert_eq!(components[1], vec![String::from("d")]);
        assert_eq!(components[2], vec![String::from("e")]);
    }

    #[test]
    fn external_wake_source_kind_classification_is_strict() {
        assert!(node_has_external_wake_source("mpsc_rx"));
        assert!(node_has_external_wake_source("net_read"));
        assert!(!node_has_external_wake_source("future"));
        assert!(!node_has_external_wake_source("mpsc_tx"));
    }

    #[test]
    fn crate_parser_handles_trait_impl_style_names() {
        assert_eq!(
            crate_from_function_name("<alloc::vec::Vec<u8> as core::fmt::Debug>::fmt"),
            Some("alloc")
        );
        assert_eq!(
            crate_from_function_name("tokio::runtime::context::enter"),
            Some("tokio")
        );
    }

    #[test]
    fn source_snippet_renders_line_prefixed_text() {
        let rendered = render_source_snippet(&McpSourceContext {
            format: String::from("text/plain"),
            frame_id: 42,
            source_file: String::from("/tmp/example.rs"),
            target_line: 15,
            target_col: Some(9),
            total_lines: 200,
            statement_text: None,
            enclosing_fn_text: None,
            compact_scope_text: Some(String::from(
                "fn demo() {\n\n    let x = 1;\n    do_work(x).await?;\n\n}",
            )),
            compact_scope_range: Some(McpLineRange { start: 12, end: 17 }),
        });
        assert!(rendered.contains("in /tmp/example.rs"));
        assert!(rendered.contains("   12 | fn demo() {"));
        assert!(rendered.contains(">   15 |     do_work(x).await?;"));
        assert!(!rendered.contains("\n\n\n"));
    }
}
