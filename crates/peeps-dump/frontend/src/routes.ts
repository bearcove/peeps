import type { Tab } from "./App";

export type ResourceRef =
  | { kind: "process"; process: string; pid?: number }
  | { kind: "task"; process: string; taskId: number }
  | { kind: "thread"; process: string; thread: string }
  | { kind: "lock"; process: string; lock: string }
  | { kind: "mpsc"; process: string; name: string }
  | { kind: "oneshot"; process: string; name: string }
  | { kind: "watch"; process: string; name: string }
  | { kind: "once_cell"; process: string; name: string }
  | { kind: "future_wait"; process: string; taskId: number; resource: string }
  | { kind: "connection"; process: string; connection: string }
  | { kind: "request"; process: string; connection: string; requestId: number }
  | { kind: "shm_segment"; process: string; segment: string }
  | { kind: "shm_peer"; process: string; segment: string; peerId: number };

function enc(v: string): string {
  return encodeURIComponent(v);
}

export function tabPath(tab: Tab): string {
  return `/${tab}`;
}

export function tabFromPath(pathname: string): Tab {
  const first = pathname.replace(/^\/+/, "").split("/")[0];
  switch (first) {
    case "tasks":
    case "threads":
    case "locks":
    case "sync":
    case "requests":
    case "connections":
    case "processes":
    case "shm":
    case "problems":
    case "deadlocks":
      return first;
    default:
      return "problems";
  }
}

export function resourceHref(ref: ResourceRef): string {
  switch (ref.kind) {
    case "process":
      return ref.pid != null
        ? `/processes/${enc(ref.process)}/${ref.pid}`
        : `/processes/${enc(ref.process)}`;
    case "task":
      return `/tasks/${enc(ref.process)}/${ref.taskId}`;
    case "thread":
      return `/threads/${enc(ref.process)}/${enc(ref.thread)}`;
    case "lock":
      return `/locks/${enc(ref.process)}/${enc(ref.lock)}`;
    case "mpsc":
      return `/sync/mpsc/${enc(ref.process)}/${enc(ref.name)}`;
    case "oneshot":
      return `/sync/oneshot/${enc(ref.process)}/${enc(ref.name)}`;
    case "watch":
      return `/sync/watch/${enc(ref.process)}/${enc(ref.name)}`;
    case "once_cell":
      return `/sync/once-cell/${enc(ref.process)}/${enc(ref.name)}`;
    case "future_wait":
      return `/tasks/future/${enc(ref.process)}/${ref.taskId}/${enc(ref.resource)}`;
    case "connection":
      return `/connections/${enc(ref.process)}/${enc(ref.connection)}`;
    case "request":
      return `/requests/${enc(ref.process)}/${enc(ref.connection)}/${ref.requestId}`;
    case "shm_segment":
      return `/shm/segments/${enc(ref.process)}/${enc(ref.segment)}`;
    case "shm_peer":
      return `/shm/peers/${enc(ref.process)}/${enc(ref.segment)}/${ref.peerId}`;
  }
}

export function navigateTo(path: string): void {
  if (window.location.pathname === path) return;
  window.history.pushState({}, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

export function isActivePath(currentPath: string, targetPath: string): boolean {
  return currentPath === targetPath;
}
