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

interface ApiErrorResponse {
  error?: string;
}

async function readErrorMessage(res: Response): Promise<string> {
  const body = await res.text();
  if (!body) return `${res.status} ${res.statusText}`;
  try {
    const parsed = JSON.parse(body) as ApiErrorResponse;
    if (parsed.error) return parsed.error;
  } catch {
    // Fall back to response body text.
  }
  return `${res.status} ${res.statusText}: ${body}`;
}

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(await readErrorMessage(res));
  }
  return res.json() as Promise<T>;
}

async function postJson<T>(url: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(await readErrorMessage(res));
  }
  return res.json() as Promise<T>;
}

export function fetchConnections(): Promise<ConnectionsResponse> {
  return getJson<ConnectionsResponse>("/api/connections");
}

export function triggerCut(): Promise<TriggerCutResponse> {
  return postJson<TriggerCutResponse>("/api/cuts", {});
}

export function fetchCutStatus(cutId: string): Promise<CutStatusResponse> {
  return getJson<CutStatusResponse>(`/api/cuts/${encodeURIComponent(cutId)}`);
}

export function runSql(sql: string): Promise<SqlResponse> {
  return postJson<SqlResponse>("/api/sql", { sql });
}
