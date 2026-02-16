export interface JumpNowResponse {
  snapshot_id: number;
  requested: number;
  responded: number;
  timed_out: number;
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
