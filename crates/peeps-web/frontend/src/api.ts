import type {
  ConnectionsResponse,
  JumpNowResponse,
  SnapshotProgressResponse,
  SqlRequest,
  SqlResponse,
  StuckRequest,
  SnapshotGraph,
  SnapshotNode,
  SnapshotEdge,
  TimelineCursor,
  TimelineEvent,
  TimelinePage,
  TimelineProcessOption,
  TimelineRelation,
  TimelineRow,
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

async function get<T>(url: string): Promise<T> {
  const res = await fetch(url);
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

export async function fetchSnapshotProgress(): Promise<SnapshotProgressResponse> {
  return get<SnapshotProgressResponse>("/api/snapshot-progress");
}

export async function fetchConnections(): Promise<ConnectionsResponse> {
  return get<ConnectionsResponse>("/api/connections");
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
  json_extract(r.attrs_json, '$."rpc.connection"') AS connection
FROM nodes r
WHERE r.kind = 'request'
  AND CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) >= ?1
ORDER BY CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) DESC
LIMIT 10;`;

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
  }));
}

const NODES_SQL = `SELECT id, kind, process, proc_key, attrs_json FROM nodes ORDER BY id`;
const EDGES_SQL = `SELECT src_id, dst_id, kind, attrs_json FROM edges ORDER BY src_id, dst_id`;

const TIMELINE_SQL = `SELECT
  id,
  ts_ns,
  name,
  entity_id,
  parent_entity_id,
  attrs_json
FROM events
WHERE proc_key = ?1
  AND (entity_id = ?2 OR parent_entity_id = ?2)
  AND ts_ns <= ?3
  AND (
    ?4 IS NULL
    OR ts_ns < ?4
    OR (ts_ns = ?4 AND id < ?5)
  )
ORDER BY ts_ns DESC, id DESC
LIMIT ?6`;

function timelineRelationForRow(
  selectedEntityId: string,
  entityId: string,
  parentEntityId: string | null,
): TimelineRelation {
  if (entityId === selectedEntityId && parentEntityId && parentEntityId !== selectedEntityId) {
    return "parent";
  }
  if (entityId === selectedEntityId) return "self";
  return "child";
}

export async function fetchTimelinePage(
  snapshotId: number,
  params: {
    procKey: string;
    entityId: string;
    capturedAtNs: number;
    limit: number;
    cursor: TimelineCursor | null;
  },
): Promise<TimelinePage> {
  const resp = await querySql(snapshotId, TIMELINE_SQL, [
    params.procKey,
    params.entityId,
    params.capturedAtNs,
    params.cursor?.ts_ns ?? null,
    params.cursor?.id ?? null,
    params.limit,
  ]);

  const rows: TimelineRow[] = resp.rows.map((row) => {
    const entityId = row[3] as string;
    const parentEntityId = row[4] as string | null;
    const attrsRaw = row[5];
    let attrs: Record<string, unknown> = {};
    if (typeof attrsRaw === "string" && attrsRaw.length > 0) {
      try {
        const parsed = JSON.parse(attrsRaw) as unknown;
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
          attrs = parsed as Record<string, unknown>;
        }
      } catch {
        attrs = {};
      }
    }
    return {
      id: row[0] as string,
      ts_ns: Number(row[1]),
      name: row[2] as string,
      entity_id: entityId,
      parent_entity_id: parentEntityId,
      relation: timelineRelationForRow(params.entityId, entityId, parentEntityId),
      attrs,
    };
  });

  const nextCursor =
    rows.length === params.limit
      ? {
          ts_ns: rows[rows.length - 1].ts_ns,
          id: rows[rows.length - 1].id,
        }
      : null;

  return { rows, nextCursor };
}

const TIMELINE_PROCESS_OPTIONS_SQL = `
SELECT DISTINCT proc_key, process
FROM snapshot_processes
ORDER BY process, proc_key;`;

const RECENT_TIMELINE_EVENTS_SQL = `WITH proc_scope AS (
  SELECT proc_key, process
  FROM snapshot_processes
),
event_base AS (
  SELECT
    e.id,
    e.ts_ns,
    p.process,
    e.proc_key,
    e.entity_id,
    e.parent_entity_id,
    e.name,
    e.attrs_json,
    json_extract(e.attrs_json, '$.correlation') AS correlation
  FROM events e
  INNER JOIN proc_scope p ON p.proc_key = e.proc_key
  WHERE e.ts_ns >= ?1
    AND (?2 IS NULL OR e.proc_key = ?2)
  ORDER BY e.ts_ns DESC
  LIMIT ?3
)
SELECT
  id,
  ts_ns,
  process,
  proc_key,
  entity_id,
  parent_entity_id,
  name,
  correlation,
  attrs_json
FROM event_base
ORDER BY ts_ns DESC, id DESC;`;

function asNumber(value: string | number | null, fallback = 0): number {
  if (typeof value === "number") return value;
  if (typeof value === "string") {
    const parsed = Number(value);
    if (!Number.isNaN(parsed)) return parsed;
  }
  return fallback;
}

export async function fetchTimelineProcessOptions(snapshotId: number): Promise<TimelineProcessOption[]> {
  const resp = await querySql(snapshotId, TIMELINE_PROCESS_OPTIONS_SQL);
  return resp.rows.map((row) => ({
    proc_key: String(row[0] ?? ""),
    process: String(row[1] ?? ""),
  }));
}

export async function fetchRecentTimelineEvents(
  snapshotId: number,
  fromTsNs: number,
  procKey: string | null,
  limit = 1000,
): Promise<TimelineEvent[]> {
  const resp = await querySql(snapshotId, RECENT_TIMELINE_EVENTS_SQL, [fromTsNs, procKey, limit]);
  return resp.rows.map((row) => {
    const attrsRaw = row[8];
    let attrs: Record<string, unknown> = {};
    if (typeof attrsRaw === "string" && attrsRaw.length > 0) {
      try {
        const parsed = JSON.parse(attrsRaw) as unknown;
        if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
          attrs = parsed as Record<string, unknown>;
        }
      } catch {
        attrs = {};
      }
    }
    return {
      id: String(row[0] ?? ""),
      ts_ns: asNumber(row[1]),
      process: String(row[2] ?? ""),
      proc_key: String(row[3] ?? ""),
      entity_id: String(row[4] ?? ""),
      parent_entity_id: row[5] != null ? String(row[5]) : null,
      name: String(row[6] ?? ""),
      correlation: row[7] != null ? String(row[7]) : null,
      attrs,
    };
  });
}

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

  // Synthesize ghost nodes for dangling edge endpoints.
  const nodeIds = new Set(nodes.map((n) => n.id));
  const ghostMap = new Map<string, SnapshotNode>();

  // Fallback ghost synthesis: if any persisted edge references a missing endpoint,
  // materialize a ghost node so the edge remains visible instead of being dropped.
  for (const e of edges) {
    if (!nodeIds.has(e.src_id) && !ghostMap.has(e.src_id)) {
      ghostMap.set(e.src_id, {
        id: e.src_id,
        kind: "ghost",
        process: "",
        proc_key: "",
        attrs: {
          reason: "missing_src",
          missing_side: "src",
          source: "dangling_edge",
        },
      });
    }
    if (!nodeIds.has(e.dst_id) && !ghostMap.has(e.dst_id)) {
      ghostMap.set(e.dst_id, {
        id: e.dst_id,
        kind: "ghost",
        process: "",
        proc_key: "",
        attrs: {
          reason: "missing_dst",
          missing_side: "dst",
          source: "dangling_edge",
        },
      });
    }
  }

  const ghostNodes = Array.from(ghostMap.values());

  return { nodes: [...nodes, ...ghostNodes], edges, ghostNodes };
}
