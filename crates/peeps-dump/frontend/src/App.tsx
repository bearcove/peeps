import { useEffect, useRef, useState } from "preact/hooks";
import type { ProcessDump, DeadlockCandidate } from "./types";
import {
  connectWebSocket,
  fetchSummary,
  fetchTasks,
  fetchThreads,
  fetchDeadlocks,
  fetchConnections,
  fetchShm,
  fetchFullDump,
  type SummaryData,
} from "./api";
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
  "sync",
  "locks",
  "requests",
  "connections",
  "processes",
  "shm",
] as const;
export type Tab = (typeof TABS)[number];

const RECONNECT_DELAY_MS = 2000;

// Tabs that share data from /api/dumps
const FULL_DUMP_TABS: readonly Tab[] = ["problems", "locks", "sync", "requests", "processes"];

export function App() {
  const [tabData, setTabData] = useState<Partial<Record<Tab, ProcessDump[]>>>({});
  const [deadlockCandidates, setDeadlockCandidates] = useState<DeadlockCandidate[]>([]);
  const [summary, setSummary] = useState<SummaryData | null>(null);
  const [currentSeq, setCurrentSeq] = useState(0);
  const [latestServerSeq, setLatestServerSeq] = useState(0);
  const [stale, setStale] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [path, setPath] = useState<string>(window.location.pathname || "/problems");
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  const tab = tabFromPath(path);

  // Refs so WS callbacks and effects always see latest values
  const tabDataRef = useRef(tabData);
  tabDataRef.current = tabData;
  const deadlockCandidatesRef = useRef(deadlockCandidates);
  deadlockCandidatesRef.current = deadlockCandidates;

  async function doFetchTab(activeTab: Tab): Promise<number> {
    if (FULL_DUMP_TABS.includes(activeTab)) {
      const result = await fetchFullDump();
      const dumps = result.data.dumps ?? [];
      const deadlocks = result.data.deadlock_candidates ?? [];
      setTabData((prev) => {
        const next = { ...prev };
        for (const ft of FULL_DUMP_TABS) next[ft] = dumps;
        return next;
      });
      setDeadlockCandidates(deadlocks);
      return result.seq;
    }
    if (activeTab === "deadlocks") {
      const result = await fetchDeadlocks();
      setDeadlockCandidates(result.data);
      return result.seq;
    }
    if (activeTab === "tasks") {
      const result = await fetchTasks();
      setTabData((prev) => ({ ...prev, tasks: result.data }));
      return result.seq;
    }
    if (activeTab === "threads") {
      const result = await fetchThreads();
      setTabData((prev) => ({ ...prev, threads: result.data }));
      return result.seq;
    }
    if (activeTab === "connections") {
      const result = await fetchConnections();
      setTabData((prev) => ({ ...prev, connections: result.data }));
      return result.seq;
    }
    if (activeTab === "shm") {
      const result = await fetchShm();
      setTabData((prev) => ({ ...prev, shm: result.data }));
      return result.seq;
    }
    return 0;
  }

  async function doRefresh(targetTab?: Tab) {
    const activeTab = targetTab ?? tab;
    setRefreshing(true);
    try {
      const [summaryResult, seq] = await Promise.all([
        fetchSummary(),
        doFetchTab(activeTab),
      ]);
      setSummary(summaryResult.data);
      setCurrentSeq(seq);
      setStale(false);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }

  const doRefreshRef = useRef(doRefresh);
  doRefreshRef.current = doRefresh;

  // WebSocket connection
  useEffect(() => {
    let cancelled = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    // Initial HTTP fetch
    doRefreshRef.current();

    function connect() {
      if (cancelled) return;
      const close = connectWebSocket({
        onHello: (seq) => {
          if (cancelled) return;
          setLatestServerSeq(seq);
          doRefreshRef.current();
        },
        onUpdated: (seq) => {
          if (cancelled) return;
          setLatestServerSeq(seq);
          setStale(true);
        },
        onError: (err) => {
          if (cancelled) return;
          setError(err);
        },
        onClose: () => {
          if (cancelled) return;
          reconnectTimer = setTimeout(connect, RECONNECT_DELAY_MS);
        },
      });
      cleanupRef.current = close;
    }

    connect();

    return () => {
      cancelled = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      if (cleanupRef.current) {
        cleanupRef.current();
        cleanupRef.current = null;
      }
    };
  }, []);

  // Fetch data when navigating to a tab without cached data
  useEffect(() => {
    const currentTab = tabFromPath(path);
    const hasCachedData =
      currentTab === "deadlocks"
        ? deadlockCandidatesRef.current.length > 0
        : !!tabDataRef.current[currentTab];
    if (!hasCachedData) {
      doRefreshRef.current(currentTab);
    }
  }, [path]);

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

  const dumps = tabData[tab] ?? [];

  return (
    <div class="app">
      <Header
        summary={summary}
        filter={filter}
        onFilter={setFilter}
        onRefresh={() => doRefresh()}
        error={error}
        stale={stale}
        latestSeq={latestServerSeq}
        currentSeq={currentSeq}
        refreshing={refreshing}
      />
      <TabBar
        tabs={TABS}
        active={tab}
        onSelect={(t) => navigateTo(tabPath(t))}
        summary={summary}
        deadlockCandidates={deadlockCandidates}
        problemsDumps={tabData.problems ?? []}
      />
      <div class="content">
        {tab === "problems" && <ProblemsView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "deadlocks" && (
          <DeadlocksView candidates={deadlockCandidates} filter={filter} selectedPath={path} />
        )}
        {tab === "tasks" && <TasksView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "threads" && <ThreadsView dumps={dumps} filter={filter} selectedPath={path} />}
        {tab === "sync" && (
          <>
            <LocksView dumps={dumps} filter={filter} selectedPath={path} />
            <SyncView dumps={dumps} filter={filter} selectedPath={path} mode="locks" />
          </>
        )}
        {tab === "locks" && <SyncView dumps={dumps} filter={filter} selectedPath={path} mode="channels" />}
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
