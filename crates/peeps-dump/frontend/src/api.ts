import type { DashboardPayload, DeadlockCandidate, ProcessDump } from "./types";

export interface SummaryData {
  process_count: number;
  task_count: number;
  thread_count: number;
  seq: number;
}

interface ApiEnvelope<T> {
  version: number;
  seq: number;
  server_time_ms: number;
  data: T;
}

async function fetchEndpoint<T>(path: string): Promise<{ seq: number; data: T }> {
  const resp = await fetch(path);
  if (!resp.ok) throw new Error(`fetch ${path} failed: ${resp.status}`);
  const envelope: ApiEnvelope<T> = await resp.json();
  return { seq: envelope.seq, data: envelope.data };
}

export const fetchSummary = () => fetchEndpoint<SummaryData>("/api/summary");
export const fetchTasks = () => fetchEndpoint<ProcessDump[]>("/api/tasks");
export const fetchThreads = () => fetchEndpoint<ProcessDump[]>("/api/threads");
export const fetchDeadlocks = () => fetchEndpoint<DeadlockCandidate[]>("/api/deadlocks");
export const fetchConnections = () => fetchEndpoint<ProcessDump[]>("/api/connections");
export const fetchShm = () => fetchEndpoint<ProcessDump[]>("/api/shm");
export const fetchFullDump = () => fetchEndpoint<DashboardPayload>("/api/dumps");

export interface WebSocketCallbacks {
  onHello: (seq: number) => void;
  onUpdated: (seq: number, changed: string[]) => void;
  onError: (err: string) => void;
  onClose: () => void;
}

export function connectWebSocket(callbacks: WebSocketCallbacks): () => void {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const wsOverride = import.meta.env.VITE_PEEPS_WS_URL as string | undefined;
  const url = wsOverride && wsOverride.length > 0
    ? wsOverride
    : `${proto}//${location.host}/api/ws`;
  const ws = new WebSocket(url);

  ws.onmessage = (event) => {
    if (typeof event.data !== "string") return;
    try {
      const msg = JSON.parse(event.data);
      if (msg.type === "hello") {
        callbacks.onHello(msg.latest_seq ?? 0);
      } else if (msg.type === "updated") {
        callbacks.onUpdated(msg.seq ?? 0, msg.changed ?? []);
      }
    } catch (e) {
      console.error("[peeps/ws] failed to parse message", e);
    }
  };

  ws.onerror = () => {
    console.error("[peeps/ws] socket error");
    callbacks.onError("WebSocket error");
  };

  ws.onclose = () => {
    callbacks.onClose();
  };

  return () => {
    ws.close();
  };
}
