import type { ProcessDump } from "./types";

export async function fetchDumps(): Promise<ProcessDump[]> {
  const resp = await fetch("/api/dumps");
  if (!resp.ok) throw new Error(`fetch failed: ${resp.status}`);
  return resp.json();
}

export interface WebSocketCallbacks {
  onDumps: (dumps: ProcessDump[]) => void;
  onError: (err: string) => void;
  onClose: () => void;
}

export function connectWebSocket(callbacks: WebSocketCallbacks): () => void {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const url = `${proto}//${location.host}/api/ws`;
  const ws = new WebSocket(url);

  ws.onmessage = (event) => {
    try {
      const dumps: ProcessDump[] = JSON.parse(event.data);
      callbacks.onDumps(dumps);
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
