export interface ConnectedProcessInfo {
  conn_id: number;
  process_name: string;
  pid: number;
}

export interface ConnectionsResponse {
  connected_processes: number;
  processes: ConnectedProcessInfo[];
}

export interface TriggerCutResponse {
  cut_id: string;
  requested_at_ns: number;
  requested_connections: number;
}

export interface CutStatusResponse {
  cut_id: string;
  requested_at_ns: number;
  pending_connections: number;
  acked_connections: number;
  pending_conn_ids: number[];
}

export interface SqlResponse {
  columns: string[];
  rows: unknown[];
  row_count: number;
}

// ── Snapshot types ────────────────────────────────────────────

type ChannelLifecycle =
  | "open"
  | { closed: "sender_dropped" | "receiver_dropped" | "receiver_closed" };

interface BufferState {
  occupancy: number;
  capacity: number | null;
}

type ChannelDetails =
  | { mpsc: { buffer: BufferState | null } }
  | { broadcast: { buffer: BufferState | null } }
  | { watch: { last_update_at: number | null } }
  | { oneshot: { state: "pending" | "sent" | "received" | "sender_dropped" | "receiver_dropped" } };

interface ChannelEndpoint {
  lifecycle: ChannelLifecycle;
  details: ChannelDetails;
}

export type EntityBody =
  | "future"
  | { lock: { kind: "mutex" | "rw_lock" | "other" } }
  | { channel_tx: ChannelEndpoint }
  | { channel_rx: ChannelEndpoint }
  | { semaphore: { max_permits: number; handed_out_permits: number } }
  | { notify: { waiter_count: number } }
  | { once_cell: { waiter_count: number; state: "empty" | "initializing" | "initialized" } }
  | { command: { program: string; args: string[]; env: string[] } }
  | { file_op: { op: string; path: string } }
  | { net_connect: { addr: string } }
  | { net_accept: { addr: string } }
  | { net_read: { addr: string } }
  | { net_write: { addr: string } }
  | { request: { method: string; args_preview: string } }
  | { response: { method: string; status: "pending" | "ok" | "error" | "cancelled" } };

export interface SnapshotEntity {
  id: string;
  /** Process-relative birth time in milliseconds (PTime). Not comparable across processes. */
  birth: number;
  source: string;
  krate?: string;
  name: string;
  body: EntityBody;
  meta: Record<string, unknown> | null;
}

export type SnapshotEdgeKind =
  | "touches"
  | "needs"
  | "holds"
  | "polls"
  | "closed_by"
  | "channel_link"
  | "rpc_link";

export interface SnapshotScope {
  id: string;
  birth: number;
  source: string;
  krate?: string;
  name: string;
  /** Scope kind: "process" | "thread" | "task" | "connection" */
  body: string;
}

/** Maps a scope to one of its member entities, within a single process. */
export interface ScopeEntityLink {
  scope_id: string;
  entity_id: string;
}

export interface SnapshotEvent {
  id: string;
  at: number;
}

export interface SnapshotEdge {
  /** Process-local entity ID. */
  src: string;
  /** Process-local entity ID. */
  dst: string;
  kind: SnapshotEdgeKind;
  meta?: Record<string, unknown> | null;
}

export interface ProcessSnapshot {
  entities: SnapshotEntity[];
  scopes: SnapshotScope[];
  edges: SnapshotEdge[];
  events: SnapshotEvent[];
}

/** Per-process snapshot data. All times are process-relative (PTime in ms) unless noted. */
export interface ProcessSnapshotView {
  process_id: number;
  process_name: string;
  pid: number;
  /** Process-relative time (ms since process start) at the capture moment. */
  ptime_now_ms: number;
  snapshot: ProcessSnapshot;
  /** Which entities belong to which scope, derived from entity_scope_links at snapshot time. */
  scope_entity_links: ScopeEntityLink[];
}

export interface TimedOutProcess {
  process_id: number;
  process_name: string;
  pid: number;
}

export interface SnapshotCutResponse {
  /** Unix epoch ms of when this snapshot was captured (wall clock). */
  captured_at_unix_ms: number;
  processes: ProcessSnapshotView[];
  timed_out_processes: TimedOutProcess[];
}

// ── Recording types ─────────────────────────────────────────

export interface RecordStartRequest {
  interval_ms?: number;
  max_frames?: number;
  max_memory_bytes?: number;
}

export interface FrameSummary {
  frame_index: number;
  captured_at_unix_ms: number;
  process_count: number;
  capture_duration_ms: number;
}

export interface RecordingSessionInfo {
  session_id: string;
  status: "recording" | "stopped";
  interval_ms: number;
  started_at_unix_ms: number;
  stopped_at_unix_ms: number | null;
  frame_count: number;
  max_frames: number;
  max_memory_bytes: number;
  overflowed: boolean;
  approx_memory_bytes: number;
  avg_capture_ms: number;
  max_capture_ms: number;
  total_capture_ms: number;
  frames: FrameSummary[];
}

export interface RecordCurrentResponse {
  session: RecordingSessionInfo | null;
}

export interface RecordingExportFrame {
  frame_index: number;
  snapshot: SnapshotCutResponse;
}

export interface RecordingExport {
  version: number;
  session: RecordingSessionInfo;
  frames: RecordingExportFrame[];
}
