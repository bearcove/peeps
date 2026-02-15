import { useEffect, useRef, useState } from "preact/hooks";
import type { ProcessDump, DeadlockCandidate } from "./types";
import { connectWebSocket, fetchDumps } from "./api";
import { Header } from "./components/Header";
import { TabBar } from "./components/TabBar";
import { TasksView } from "./components/TasksView";
import { ThreadsView } from "./components/ThreadsView";
import { LocksView } from "./components/LocksView";
import { SyncView } from "./components/SyncView";
import { ProcessesView } from "./components/ProcessesView";
import { ConnectionsView } from "./components/ConnectionsView";
import { RequestsView } from "./components/RequestsView";
import { ShmView } from "./components/ShmView";
import { ProblemsView } from "./components/ProblemsView";
import { DeadlocksView } from "./components/DeadlocksView";
import { navigateTo, tabFromPath, tabPath } from "./routes";

import "./styles.css";

const TABS = [
  "problems",
  "deadlocks",
  "tasks",
  "threads",
  "locks",
  "sync",
  "requests",
  "connections",
  "processes",
  "shm",
] as const;
export type Tab = (typeof TABS)[number];

const RECONNECT_DELAY_MS = 2000;
const MAX_WS_FAILURES = 3;

export function App() {
  const [dumps, setDumps] = useState<ProcessDump[]>([]);
  const [deadlockCandidates, setDeadlockCandidates] = useState<DeadlockCandidate[]>([]);
  const [path, setPath] = useState<string>(window.location.pathname || "/problems");
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    let cancelled = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let wsFailures = 0;
    let pollInterval: ReturnType<typeof setInterval> | null = null;

    function startPolling() {
      if (pollInterval) return;
      const poll = async () => {
        try {
          const data = await fetchDumps();
          if (!cancelled) {
            setDumps(data.dumps);
            setDeadlockCandidates(data.deadlockCandidates);
            setError(null);
          }
        } catch (e) {
          if (!cancelled) setError(String(e));
        }
      };
      poll();
      pollInterval = setInterval(poll, 2000);
    }

    function stopPolling() {
      if (pollInterval) {
        clearInterval(pollInterval);
        pollInterval = null;
      }
    }

    function connect() {
      if (cancelled) return;

      const close = connectWebSocket({
        onData: (data) => {
          if (!cancelled) {
            wsFailures = 0;
            setDumps(data.dumps);
            setDeadlockCandidates(data.deadlockCandidates);
            setError(null);
            stopPolling();
          }
        },
        onError: (err) => {
          if (!cancelled) setError(err);
        },
        onClose: () => {
          if (cancelled) return;
          wsFailures++;
          if (wsFailures >= MAX_WS_FAILURES) {
            startPolling();
          }
          reconnectTimer = setTimeout(connect, RECONNECT_DELAY_MS);
        },
      });

      cleanupRef.current = close;
    }

    connect();

    return () => {
      cancelled = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      stopPolling();
      if (cleanupRef.current) {
        cleanupRef.current();
        cleanupRef.current = null;
      }
    };
  }, []);

  const refresh = async () => {
    try {
      const data = await fetchDumps();
      setDumps(data.dumps);
      setDeadlockCandidates(data.deadlockCandidates);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const hasSync = dumps.some((d) => d.sync != null);
  const hasRoam = dumps.some((d) => d.roam != null);
  const hasShm = dumps.some((d) => d.shm != null);
  const hasLocks = dumps.some((d) => d.locks != null);
  const hasDeadlocks = deadlockCandidates.length > 0;

  const visibleTabs = TABS.filter((t) => {
    if (t === "problems") return true;
    if (t === "deadlocks" && !hasDeadlocks) return false;
    if (t === "sync" && !hasSync) return false;
    if (t === "requests" && !hasRoam) return false;
    if (t === "connections" && !hasRoam) return false;
    if (t === "shm" && !hasShm) return false;
    if (t === "locks" && !hasLocks) return false;
    return true;
  });

  const tab = tabFromPath(path);

  useEffect(() => {
    const onPop = () => setPath(window.location.pathname || "/problems");
    window.addEventListener("popstate", onPop);
    onPop();
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  useEffect(() => {
    if (window.location.pathname === "/") {
      navigateTo("/problems");
    }
  }, []);

  return (
    <div class="app">
      <Header dumps={dumps} filter={filter} onFilter={setFilter} onRefresh={refresh} error={error} />
      <TabBar
        tabs={visibleTabs}
        active={tab}
        onSelect={(t) => navigateTo(tabPath(t))}
        dumps={dumps}
        deadlockCandidates={deadlockCandidates}
      />
      <div class="content">
        {tab === "problems" && <ProblemsView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "deadlocks" && (
          <DeadlocksView candidates={deadlockCandidates} filter={filter} selectedPath={path} />
        )}
        {tab === "tasks" && <TasksView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "threads" && <ThreadsView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "locks" && <LocksView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "sync" && <SyncView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "requests" && <RequestsView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "connections" && (
          <ConnectionsView dumps={dumps} filter={filter} selectedPath={path} />
        )}
        {tab === "processes" && (
          <ProcessesView dumps={dumps} filter={filter} selectedPath={path} />
        )}
        {tab === "shm" && <ShmView dumps={dumps} filter={filter} selectedPath={path} />}
      </div>
    </div>
  );
}
