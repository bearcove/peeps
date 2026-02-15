import type { DashboardPayload, DeadlockCandidate, ProcessDump } from "./types";

export interface DashboardData {
  dumps: ProcessDump[];
  deadlockCandidates: DeadlockCandidate[];
}

function parseDashboardData(raw: unknown): DashboardData {
  // Handle both new DashboardPayload shape and legacy Vec<ProcessDump>
  if (Array.isArray(raw)) {
    return { dumps: raw, deadlockCandidates: [] };
  }
  const payload = raw as DashboardPayload;
  return {
    dumps: payload.dumps ?? [],
    deadlockCandidates: payload.deadlock_candidates ?? [],
  };
}

export async function fetchDumps(): Promise<DashboardData> {
  const resp = await fetch("/api/dumps");
  if (!resp.ok) throw new Error(`fetch failed: ${resp.status}`);
  const raw = await resp.json();
  return parseDashboardData(raw);
}

export interface WebSocketCallbacks {
  onData: (data: DashboardData) => void;
  onError: (err: string) => void;
  onClose: () => void;
}

export function connectWebSocket(callbacks: WebSocketCallbacks): () => void {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const url = `${proto}//${location.host}/api/ws`;
  const ws = new WebSocket(url);

  ws.onmessage = (event) => {
    try {
      const raw = JSON.parse(event.data);
      callbacks.onData(parseDashboardData(raw));
    } catch (e) {
      callbacks.onError(`WebSocket parse error: ${e}`);
    }
  };

  ws.onerror = () => {
    callbacks.onError("WebSocket error");
  };

  ws.onclose = () => {
    callbacks.onClose();
  };

  return () => {
    ws.close();
  };
}
