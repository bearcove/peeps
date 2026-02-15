import type {
  JumpNowResponse,
  SqlRequest,
  SqlResponse,
  StuckRequest,
  SnapshotGraph,
  SnapshotNode,
  SnapshotEdge,
} from "./types";

async function post<T>(url: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const text = await res.text();
    let msg = `${res.status} ${res.statusText}`;
    try {
      const err = JSON.parse(text);
      if (err.error) msg = err.error;
    } catch {
      /* use status text */
    }
    throw new Error(msg);
  }
  return res.json() as Promise<T>;
}

export async function jumpNow(): Promise<JumpNowResponse> {
  return post<JumpNowResponse>("/api/jump-now", {});
}

export async function querySql(
  snapshotId: number,
  sql: string,
  params: (string | number | null)[] = [],
): Promise<SqlResponse> {
  const req: SqlRequest = { snapshot_id: snapshotId, sql, params };
  return post<SqlResponse>("/api/sql", req);
}

const STUCK_REQUEST_SQL = `SELECT
  r.id,
  json_extract(r.attrs_json, '$.method') AS method,
  r.process,
  CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) AS elapsed_ns,
  json_extract(r.attrs_json, '$.connection') AS connection,
  json_extract(r.attrs_json, '$.correlation_key') AS correlation_key
FROM nodes r
LEFT JOIN nodes resp
  ON resp.kind = 'response'
 AND json_extract(resp.attrs_json, '$.correlation_key') = json_extract(r.attrs_json, '$.correlation_key')
WHERE r.kind = 'request'
  AND CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) >= ?1
  AND (resp.id IS NULL OR json_extract(resp.attrs_json, '$.status') = 'in_flight')
ORDER BY CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) DESC
LIMIT 500;`;

export async function fetchStuckRequests(
  snapshotId: number,
  minElapsedNs: number,
): Promise<StuckRequest[]> {
  const resp = await querySql(snapshotId, STUCK_REQUEST_SQL, [minElapsedNs]);
  return resp.rows.map((row) => ({
    id: row[0] as string,
    method: row[1] as string | null,
    process: row[2] as string,
    elapsed_ns: row[3] as number,
    connection: row[4] as string | null,
    correlation_key: row[5] as string | null,
  }));
}

const NODES_SQL = `SELECT id, kind, process, proc_key, attrs_json FROM nodes ORDER BY id`;
const EDGES_SQL = `SELECT src_id, dst_id, kind, attrs_json FROM edges ORDER BY src_id, dst_id`;

export async function fetchGraph(snapshotId: number): Promise<SnapshotGraph> {
  const [nodesResp, edgesResp] = await Promise.all([
    querySql(snapshotId, NODES_SQL),
    querySql(snapshotId, EDGES_SQL),
  ]);

  const nodes: SnapshotNode[] = nodesResp.rows.map((row) => ({
    id: row[0] as string,
    kind: row[1] as string,
    process: row[2] as string,
    proc_key: row[3] as string,
    attrs: JSON.parse((row[4] as string) || "{}"),
  }));

  const edges: SnapshotEdge[] = edgesResp.rows.map((row) => ({
    src_id: row[0] as string,
    dst_id: row[1] as string,
    kind: row[2] as string,
    attrs: JSON.parse((row[3] as string) || "{}"),
  }));

  return { nodes, edges };
}
