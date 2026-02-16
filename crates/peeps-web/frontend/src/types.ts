export interface JumpNowResponse {
  snapshot_id: number;
  captured_at_ns: number;
  requested: number;
  responded: number;
  timed_out: number;
}

export interface SnapshotProgressResponse {
  active: boolean;
  snapshot_id: number | null;
  requested: number;
  responded: number;
  pending: number;
  responded_processes: string[];
  pending_processes: string[];
}

export interface SqlRequest {
  snapshot_id: number;
  sql: string;
  params: (string | number | null)[];
}

export interface SqlResponse {
  snapshot_id: number;
  columns: string[];
  rows: (string | number | null)[][];
  row_count: number;
  truncated: boolean;
}

export interface StuckRequest {
  id: string;
  method: string | null;
  process: string;
  elapsed_ns: number;
  connection: string | null;
}

export interface TimelineProcessOption {
  proc_key: string;
  process: string;
}

export interface TimelineEvent {
  id: string;
  ts_ns: number;
  process: string;
  proc_key: string;
  entity_id: string;
  parent_entity_id: string | null;
  name: string;
  correlation_key: string | null;
  attrs: Record<string, unknown>;
}

// Raw graph data from the snapshot SQLite tables
export interface SnapshotNode {
  id: string;
  kind: string;
  process: string;
  proc_key: string;
  attrs: Record<string, unknown>;
}

export interface SnapshotEdge {
  src_id: string;
  dst_id: string;
  kind: string;
  attrs: Record<string, unknown>;
}

export interface SnapshotGraph {
  nodes: SnapshotNode[];
  edges: SnapshotEdge[];
  ghostNodes: SnapshotNode[];
}

export type TimelineRelation = "self" | "child" | "parent";

export interface TimelineCursor {
  ts_ns: number;
  id: string;
}

export interface TimelineRow {
  id: string;
  ts_ns: number;
  name: string;
  entity_id: string;
  parent_entity_id: string | null;
  relation: TimelineRelation;
  attrs: Record<string, unknown>;
}

export interface TimelinePage {
  rows: TimelineRow[];
  nextCursor: TimelineCursor | null;
}
