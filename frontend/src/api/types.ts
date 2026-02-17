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
  | { net_write: { addr: string } };

export interface SnapshotEntity {
  id: string;
  birth: number;
  source: string;
  name: string;
  body: EntityBody;
  meta: Record<string, unknown> | null;
}

export type SnapshotEdgeKind = "needs" | "polls" | "closed_by" | "channel_link" | "rpc_link";

export interface SnapshotEdge {
  src_id: string;
  dst_id: string;
  kind: SnapshotEdgeKind;
}

export interface SnapshotResponse {
  entities: SnapshotEntity[];
  edges: SnapshotEdge[];
}
