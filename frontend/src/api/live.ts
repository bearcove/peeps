import type { ApiClient } from "./client";
import type {
  ConnectionsResponse,
  CutStatusResponse,
  RecordCurrentResponse,
  RecordingSessionInfo,
  RecordStartRequest,
  SqlResponse,
  SnapshotCutResponse,
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

function expectRecordingSession(
  response: RecordCurrentResponse,
  endpoint: string,
): RecordingSessionInfo {
  if (!response.session) {
    throw new Error(`${endpoint} returned no recording session`);
  }
  return response.session;
}

export function createLiveApiClient(): ApiClient {
  return {
    fetchConnections: () => getJson<ConnectionsResponse>("/api/connections"),
    fetchSql: (sql: string) => postJson<SqlResponse>("/api/sql", { sql }),
    triggerCut: () => postJson<TriggerCutResponse>("/api/cuts", {}),
    fetchCutStatus: (cutId: string) =>
      getJson<CutStatusResponse>(`/api/cuts/${encodeURIComponent(cutId)}`),
    fetchSnapshot: () => postJson<SnapshotCutResponse>("/api/snapshot", {}),
    startRecording: async (req?: RecordStartRequest) =>
      expectRecordingSession(
        await postJson<RecordCurrentResponse>("/api/record/start", req ?? {}),
        "/api/record/start",
      ),
    stopRecording: async () =>
      expectRecordingSession(
        await postJson<RecordCurrentResponse>("/api/record/stop", {}),
        "/api/record/stop",
      ),
    fetchRecordingCurrent: () => getJson<RecordCurrentResponse>("/api/record/current"),
    fetchRecordingFrame: (frameIndex: number) => getJson<SnapshotCutResponse>(`/api/record/current/frame/${frameIndex}`),
    exportRecording: async () => {
      const res = await fetch("/api/record/current/export");
      if (!res.ok) {
        throw new Error(await readErrorMessage(res));
      }
      return res.blob();
    },
    importRecording: async (file: File) => {
      const res = await fetch("/api/record/import", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: file,
      });
      if (!res.ok) {
        throw new Error(await readErrorMessage(res));
      }
      return expectRecordingSession(
        (await res.json()) as RecordCurrentResponse,
        "/api/record/import",
      );
    },
  };
}
