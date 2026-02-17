import type { ApiClient } from "./client";
import type {
  ConnectionsResponse,
  CutStatusResponse,
  SnapshotResponse,
  TriggerCutResponse,
} from "./types";

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

export function createLiveApiClient(): ApiClient {
  return {
    fetchConnections: () => getJson<ConnectionsResponse>("/api/connections"),
    triggerCut: () => postJson<TriggerCutResponse>("/api/cuts", {}),
    fetchCutStatus: (cutId: string) =>
      getJson<CutStatusResponse>(`/api/cuts/${encodeURIComponent(cutId)}`),
    fetchSnapshot: () => postJson<SnapshotResponse>("/api/snapshot", {}),
  };
}
