import { useEffect, useRef, useState } from "preact/hooks";
import type { ProcessDump } from "./types";
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

import "./styles.css";

const TABS = [
  "problems",
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
  const [tab, setTab] = useState<Tab>("problems");
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
            setDumps(data);
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
        onDumps: (data) => {
          if (!cancelled) {
            wsFailures = 0;
            setDumps(data);
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
      setDumps(data);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const hasSync = dumps.some((d) => d.sync != null);
  const hasRoam = dumps.some((d) => d.roam != null);
  const hasShm = dumps.some((d) => d.shm != null);
  const hasLocks = dumps.some((d) => d.locks != null);

  const visibleTabs = TABS.filter((t) => {
    if (t === "problems") return true;
    if (t === "sync" && !hasSync) return false;
    if (t === "requests" && !hasRoam) return false;
    if (t === "connections" && !hasRoam) return false;
    if (t === "shm" && !hasShm) return false;
    if (t === "locks" && !hasLocks) return false;
    return true;
  });

  return (
    <div class="app">
      <Header dumps={dumps} filter={filter} onFilter={setFilter} onRefresh={refresh} error={error} />
      <TabBar tabs={visibleTabs} active={tab} onSelect={setTab} dumps={dumps} />
      <div class="content">
        {tab === "problems" && <ProblemsView dumps={dumps} filter={filter} />}
        {tab === "tasks" && <TasksView dumps={dumps} filter={filter} />}
        {tab === "threads" && <ThreadsView dumps={dumps} filter={filter} />}
        {tab === "locks" && <LocksView dumps={dumps} filter={filter} />}
        {tab === "sync" && <SyncView dumps={dumps} filter={filter} />}
        {tab === "requests" && <RequestsView dumps={dumps} filter={filter} />}
        {tab === "connections" && (
          <ConnectionsView dumps={dumps} filter={filter} />
        )}
        {tab === "processes" && (
          <ProcessesView dumps={dumps} filter={filter} />
        )}
        {tab === "shm" && <ShmView dumps={dumps} filter={filter} />}
      </div>
    </div>
  );
}
