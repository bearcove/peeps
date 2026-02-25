import type { ApiClient } from "./client";
import type {
  ConnectionsResponse,
  CutStatusResponse,
  RecordCurrentResponse,
  RecordingSessionInfo,
  RecordStartRequest,
  SnapshotSymbolicationUpdate,
  SqlResponse,
  SnapshotCutResponse,
  SourcePreviewBatchResponse,
  SourcePreviewResponse,
  TriggerCutResponse,
} from "./types.generated";
import { apiLog } from "../debug";

async function readErrorMessage(res: Response): Promise<string> {
  const body = await res.text();
  if (!body) return `${res.status} ${res.statusText}`;
  try {
    const parsed = JSON.parse(body) as unknown;
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "error" in parsed &&
      typeof (parsed as { error?: unknown }).error === "string"
    ) {
      return (parsed as { error: string }).error;
    }
  } catch {
    // Fall back to response body text.
  }
  return `${res.status} ${res.statusText}: ${body}`;
}

async function getJson<T>(url: string): Promise<T> {
  apiLog("GET %s request", url);
  const res = await fetch(url);
  apiLog("GET %s response %d", url, res.status);
  if (!res.ok) {
    const message = await readErrorMessage(res);
    apiLog("GET %s error %s", url, message);
    throw new Error(message);
  }
  const payload = (await res.json()) as T;
  apiLog("GET %s payload %O", url, payload);
  return payload;
}

async function getJsonOrNullOn404<T>(url: string): Promise<T | null> {
  apiLog("GET %s request", url);
  const res = await fetch(url);
  apiLog("GET %s response %d", url, res.status);
  if (res.status === 404) {
    apiLog("GET %s 404 -> null", url);
    return null;
  }
  if (!res.ok) {
    const message = await readErrorMessage(res);
    apiLog("GET %s error %s", url, message);
    throw new Error(message);
  }
  const payload = (await res.json()) as T;
  apiLog("GET %s payload %O", url, payload);
  return payload;
}

async function postJson<T>(url: string, body: unknown): Promise<T> {
  apiLog("POST %s request body=%O", url, body);
  const res = await fetch(url, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  apiLog("POST %s response %d", url, res.status);
  if (!res.ok) {
    const message = await readErrorMessage(res);
    apiLog("POST %s error %s", url, message);
    throw new Error(message);
  }
  const payload = (await res.json()) as T;
  apiLog("POST %s payload %O", url, payload);
  return payload;
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
    fetchExistingSnapshot: () => getJsonOrNullOn404<SnapshotCutResponse>("/api/snapshot/current"),
    fetchSnapshot: () => postJson<SnapshotCutResponse>("/api/snapshot", {}),
    streamSnapshotSymbolication: (snapshotId, onUpdate, onError) => {
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const url = `${protocol}//${window.location.host}/api/snapshot/${encodeURIComponent(String(snapshotId))}/symbolication/ws`;
      apiLog("WS %s open", url);
      console.info(`[moire:symbolication] opening stream snapshot_id=${snapshotId} url=${url}`);
      const socket = new WebSocket(url);
      socket.addEventListener("open", () => {
        console.info(`[moire:symbolication] stream open snapshot_id=${snapshotId}`);
      });
      socket.addEventListener("message", (event) => {
        try {
          const parsed = JSON.parse(event.data) as SnapshotSymbolicationUpdate;
          console.info(
            `[moire:symbolication] update snapshot_id=${parsed.snapshot_id} completed=${parsed.completed_frames}/${parsed.total_frames} updated=${parsed.updated_frames.length} done=${parsed.done}`,
          );
          onUpdate(parsed);
        } catch (e) {
          const error = new Error(
            `[symbolication] failed to parse websocket payload: ${e instanceof Error ? e.message : String(e)}`,
          );
          apiLog("WS %s parse error %s", url, error.message);
          console.warn(
            `[moire:symbolication] parse error snapshot_id=${snapshotId}: ${error.message}`,
          );
          if (onError) onError(error);
        }
      });
      socket.addEventListener("error", () => {
        const error = new Error("[symbolication] websocket error");
        apiLog("WS %s error", url);
        console.warn(`[moire:symbolication] websocket error snapshot_id=${snapshotId}`);
        if (onError) onError(error);
      });
      socket.addEventListener("close", () => {
        apiLog("WS %s closed", url);
        console.info(`[moire:symbolication] stream closed snapshot_id=${snapshotId}`);
      });
      return () => socket.close();
    },
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
    fetchRecordingFrame: (frameIndex: number) =>
      getJson<SnapshotCutResponse>(`/api/record/current/frame/${frameIndex}`),
    exportRecording: async () => {
      apiLog("GET /api/record/current/export request");
      const res = await fetch("/api/record/current/export");
      apiLog("GET /api/record/current/export response %d", res.status);
      if (!res.ok) {
        const message = await readErrorMessage(res);
        apiLog("GET /api/record/current/export error %s", message);
        throw new Error(message);
      }
      return res.blob();
    },
    fetchSourcePreview: (frameId: number) =>
      getJson<SourcePreviewResponse>(`/api/source/preview?frame_id=${frameId}`),
    fetchSourcePreviews: (frameIds: number[]) =>
      postJson<SourcePreviewBatchResponse>("/api/source/previews", { frame_ids: frameIds }),
    importRecording: async (file: File) => {
      apiLog("POST /api/record/import request size=%d", file.size);
      const res = await fetch("/api/record/import", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: file,
      });
      apiLog("POST /api/record/import response %d", res.status);
      if (!res.ok) {
        const message = await readErrorMessage(res);
        apiLog("POST /api/record/import error %s", message);
        throw new Error(message);
      }
      return expectRecordingSession(
        (await res.json()) as RecordCurrentResponse,
        "/api/record/import",
      );
    },
  };
}
