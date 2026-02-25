import { useCallback, useMemo, useRef, useState } from "react";
import { Badge, type BadgeTone } from "../ui/primitives/Badge";
import type { FilterMenuItem } from "../ui/primitives/FilterMenu";
import type { Column } from "../ui/primitives/Table";
import { NodeChip } from "../ui/primitives/NodeChip";
import { RelativeTimestamp } from "../ui/primitives/RelativeTimestamp";
import { kindIcon } from "../nodeKindSpec";
import { SCOPE_DARK_RGB, SCOPE_LIGHT_RGB } from "../components/graph/scopeColors";
import type { RecordingState, SnapshotState } from "../App";
import type { EntityDef, RenderSource } from "../snapshot";

function fakeSource(krate: string, path: string, line: number): RenderSource {
  return { path, line, krate };
}

type DemoTone = "neutral" | "ok" | "warn" | "crit";

type DemoConnectionRow = {
  id: string;
  healthLabel: string;
  healthTone: DemoTone;
  connectionKind: string;
  connectionLabel: string;
  pending: number;
  lastRecvBasis: "P" | "N";
  lastRecvBasisLabel: string;
  lastRecvBasisTime: string;
  lastRecvEventTime: string;
  lastRecvTone: DemoTone;
  lastSentBasis: "P" | "N";
  lastSentBasisLabel: string;
  lastSentBasisTime: string;
  lastSentEventTime: string | null;
  lastSentTone: DemoTone;
};

const CONTEXT_MENU_DEMO_NODES = [
  {
    id: "n1",
    label: "store.incoming.recv",
    kind: "future",
    krate: "tokio",
    processId: "vx-store",
    processLabel: "vx-store(1234)",
    kindLabel: "Future",
    location: "runtime.rs:42",
  },
  {
    id: "n2",
    label: "DemoRpc.sleepy_forever",
    kind: "request",
    krate: "roam-session",
    processId: "vx-runner",
    processLabel: "vx-runner(5678)",
    kindLabel: "Request",
    location: "lib.rs:17",
  },
  {
    id: "n3",
    label: "store.state_lock",
    kind: "lock",
    krate: "tokio",
    processId: "vx-store",
    processLabel: "vx-store(1234)",
    kindLabel: "Lock",
    location: null,
  },
] as const;

export { CONTEXT_MENU_DEMO_NODES };

export function useStorybookState() {
  const [textValue, setTextValue] = useState("Hello");
  const [searchValue, setSearchValue] = useState("");
  const [checked, setChecked] = useState(true);
  const [selectValue, setSelectValue] = useState("all");
  const [sliderValue, setSliderValue] = useState(1);
  const [lastMenuPick, setLastMenuPick] = useState<string | null>(null);
  const lastMenuPickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pickMenuAction = useCallback((prefix: string, id: string) => {
    if (lastMenuPickTimer.current) clearTimeout(lastMenuPickTimer.current);
    setLastMenuPick(`${prefix} → ${id}`);
    lastMenuPickTimer.current = setTimeout(() => setLastMenuPick(null), 3000);
  }, []);

  const [searchOnlyKind, setSearchOnlyKind] = useState<string | null>(null);
  const [selectedSearchId, setSelectedSearchId] = useState<string | null>(null);
  const [segmentedMode, setSegmentedMode] = useState("graph");
  const [segmentedSeverity, setSegmentedSeverity] = useState("all");
  const [tableSortKey, setTableSortKey] = useState("health");
  const [tableSortDir, setTableSortDir] = useState<"asc" | "desc">("desc");
  const [selectedTableRow, setSelectedTableRow] = useState<string | null>(null);
  const [hiddenKinds, setHiddenKinds] = useState<Set<string>>(new Set());
  const [hiddenProcesses, setHiddenProcesses] = useState<Set<string>>(new Set());
  const [hiddenCrates, setHiddenCrates] = useState<Set<string>>(new Set());
  const [leftPaneTab, setLeftPaneTab] = useState<"graph" | "scopes" | "entities" | "events">("graph");
  const [showLoners, setShowLoners] = useState(false);
  const [isLive, setIsLive] = useState(false);
  const [focusedEntityId, setFocusedEntityId] = useState<string | null>(null);
  const [graphFilterText, setGraphFilterText] = useState(
    "colorBy:crate groupBy:process source:on",
  );
  const fileInputRef = useRef<HTMLInputElement>(null);
  const tones = useMemo<BadgeTone[]>(() => ["neutral", "ok", "warn", "crit"], []);
  const searchDataset = useMemo(
    () => [
      {
        id: "future:store.incoming.recv",
        label: "store.incoming.recv",
        kind: "future",
        process: "vx-store",
      },
      {
        id: "request:demorpc.sleepy",
        label: "DemoRpc.sleepy_forever",
        kind: "request",
        process: "example-roam-rpc-stuck-request",
      },
      {
        id: "request:demorpc.ping",
        label: "DemoRpc.ping",
        kind: "request",
        process: "example-roam-rpc-stuck-request",
      },
      {
        id: "channel:mpsc.tx",
        label: "channel.v1.mpsc.send",
        kind: "channel",
        process: "vx-runner",
      },
      { id: "channel:mpsc.rx", label: "channel.v1.mpsc.recv", kind: "channel", process: "vx-vfsd" },
      {
        id: "oneshot:recv",
        label: "channel.v1.oneshot.recv",
        kind: "oneshot",
        process: "vx-store",
      },
      {
        id: "resource:conn",
        label: "connection initiator->acceptor",
        kind: "resource",
        process: "vxd",
      },
      { id: "net:read", label: "net.readable.wait", kind: "net", process: "vxd" },
    ],
    [],
  );

  const filterKindItems = useMemo<FilterMenuItem[]>(
    () => [
      {
        id: "connection",
        label: "Connection",
        icon: kindIcon("connection", 14),
        meta: "connection",
      },
      { id: "mutex", label: "Mutex", icon: kindIcon("mutex", 14), meta: "lock" },
      { id: "request", label: "Request", icon: kindIcon("request", 14), meta: "request" },
      { id: "response", label: "Response", icon: kindIcon("response", 14), meta: "response" },
      { id: "channel_rx", label: "Channel Rx", icon: kindIcon("channel_rx", 14), meta: "rx" },
      { id: "channel_tx", label: "Channel Tx", icon: kindIcon("channel_tx", 14), meta: "tx" },
    ],
    [],
  );

  const filterProcessItems = useMemo<FilterMenuItem[]>(
    () => [
      { id: "vx-store", label: "vx-store" },
      { id: "vx-runner", label: "vx-runner" },
      { id: "vx-vfsd", label: "vx-vfsd" },
      { id: "vxd", label: "vxd" },
      { id: "moire-collector", label: "moire-collector" },
    ],
    [],
  );

  const filterCrateItems = useMemo<FilterMenuItem[]>(
    () => [
      { id: "roam-session", label: "roam-session" },
      { id: "moire-examples", label: "moire-examples" },
      { id: "tokio", label: "tokio" },
    ],
    [],
  );

  const graphFilterNodeIds = useMemo(
    () => [
      "vx-store/store.incoming.recv",
      "example-roam-rpc/DemoRpc.sleepy_forever",
      "example-roam-rpc/DemoRpc.ping",
      "vx-store/channel.v1.mpsc.send",
      "vxd/connection.initiator",
    ],
    [],
  );

  const graphFilterLocations = useMemo(
    () => ["main.rs:20", "server.rs:45", "handler.rs:12", "session.rs:88"],
    [],
  );
  const graphFilterFocusItems = useMemo(
    () =>
      graphFilterNodeIds.map((id) => {
        const tail = id.split("/").pop() ?? id;
        return {
          id,
          label: `${tail} (storybook)`,
          searchText: `${id} ${tail} storybook`,
        };
      }),
    [graphFilterNodeIds],
  );

  const toggleKind = useCallback((id: string) => {
    setHiddenKinds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const soloKind = useCallback(
    (id: string) => {
      setHiddenKinds((prev) => {
        const othersAllHidden = filterKindItems.every(
          (item) => item.id === id || prev.has(item.id),
        );
        if (othersAllHidden && !prev.has(id)) return new Set();
        return new Set(filterKindItems.filter((item) => item.id !== id).map((item) => item.id));
      });
    },
    [filterKindItems],
  );

  const toggleProcess = useCallback((id: string) => {
    setHiddenProcesses((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const soloProcess = useCallback(
    (id: string) => {
      setHiddenProcesses((prev) => {
        const othersAllHidden = filterProcessItems.every(
          (item) => item.id === id || prev.has(item.id),
        );
        if (othersAllHidden && !prev.has(id)) return new Set();
        return new Set(filterProcessItems.filter((item) => item.id !== id).map((item) => item.id));
      });
    },
    [filterProcessItems],
  );

  const toggleCrate = useCallback((id: string) => {
    setHiddenCrates((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const soloCrate = useCallback(
    (id: string) => {
      setHiddenCrates((prev) => {
        const othersAllHidden = filterCrateItems.every(
          (item) => item.id === id || prev.has(item.id),
        );
        if (othersAllHidden && !prev.has(id)) return new Set();
        return new Set(filterCrateItems.filter((item) => item.id !== id).map((item) => item.id));
      });
    },
    [filterCrateItems],
  );

  const tableRows = useMemo<DemoConnectionRow[]>(
    () => [
      {
        id: "conn-01",
        healthLabel: "Ok",
        healthTone: "ok",
        connectionKind: "connection",
        connectionLabel: "example-roam-rpc-stuck-request: initiator→acceptor",
        pending: 0,
        lastRecvBasis: "P",
        lastRecvBasisLabel: "process started",
        lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
        lastRecvEventTime: "2026-02-17T10:05:12.000Z",
        lastRecvTone: "ok",
        lastSentBasis: "N",
        lastSentBasisLabel: "node created",
        lastSentBasisTime: "2026-02-17T10:05:12.000Z",
        lastSentEventTime: "2026-02-17T10:05:18.000Z",
        lastSentTone: "ok",
      },
      {
        id: "conn-02",
        healthLabel: "Warning",
        healthTone: "warn",
        connectionKind: "channel_tx",
        connectionLabel: "vx-store ⇄ channel.v1.mpsc.send",
        pending: 3,
        lastRecvBasis: "P",
        lastRecvBasisLabel: "process started",
        lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
        lastRecvEventTime: "2026-02-17T10:05:22.000Z",
        lastRecvTone: "warn",
        lastSentBasis: "N",
        lastSentBasisLabel: "connection opened",
        lastSentBasisTime: "2026-02-17T10:03:20.000Z",
        lastSentEventTime: "2026-02-17T10:04:10.000Z",
        lastSentTone: "warn",
      },
      {
        id: "conn-03",
        healthLabel: "Critical",
        healthTone: "crit",
        connectionKind: "request",
        connectionLabel: "example-roam-rpc-stuck-request ⇄ DemoRpc.sleepy_forever",
        pending: 12,
        lastRecvBasis: "N",
        lastRecvBasisLabel: "node opened",
        lastRecvBasisTime: "2026-02-17T09:20:00.000Z",
        lastRecvEventTime: "2026-02-17T09:27:55.000Z",
        lastRecvTone: "crit",
        lastSentBasis: "N",
        lastSentBasisLabel: "node opened",
        lastSentBasisTime: "2026-02-17T09:20:00.000Z",
        lastSentEventTime: "2026-02-17T09:24:22.000Z",
        lastSentTone: "crit",
      },
      {
        id: "conn-04",
        healthLabel: "Warning",
        healthTone: "warn",
        connectionKind: "connection",
        connectionLabel: "vxd ⇄ connection: initiator<->acceptor",
        pending: 8,
        lastRecvBasis: "P",
        lastRecvBasisLabel: "process started",
        lastRecvBasisTime: "2026-02-17T10:05:00.000Z",
        lastRecvEventTime: "2026-02-17T10:05:30.000Z",
        lastRecvTone: "warn",
        lastSentBasis: "N",
        lastSentBasisLabel: "resource created",
        lastSentBasisTime: "2026-02-17T10:03:30.000Z",
        lastSentEventTime: null,
        lastSentTone: "warn",
      },
      {
        id: "conn-05",
        healthLabel: "Ok",
        healthTone: "ok",
        connectionKind: "request",
        connectionLabel: "vx-vfsd ⇄ net.readable.wait",
        pending: 1,
        lastRecvBasis: "N",
        lastRecvBasisLabel: "socket opened",
        lastRecvBasisTime: "2026-02-17T10:04:10.000Z",
        lastRecvEventTime: "2026-02-17T10:04:14.000Z",
        lastRecvTone: "ok",
        lastSentBasis: "N",
        lastSentBasisLabel: "socket opened",
        lastSentBasisTime: "2026-02-17T10:04:10.000Z",
        lastSentEventTime: "2026-02-17T10:04:12.000Z",
        lastSentTone: "ok",
      },
    ],
    [],
  );

  const tableColumns = useMemo<readonly Column<DemoConnectionRow>[]>(
    () => [
      {
        key: "health",
        label: "Health",
        sortable: true,
        width: "80px",
        render: (row) => <Badge tone={row.healthTone}>{row.healthLabel}</Badge>,
      },
      {
        key: "connection",
        label: "Connection",
        sortable: true,
        width: "1fr",
        render: (row) => (
          <NodeChip
            kind={row.connectionKind}
            label={row.connectionLabel}
            onClick={() => console.log(`select connection ${row.id}`)}
            onContextMenu={(event) => {
              event.preventDefault();
              console.log(`connection context menu ${row.id}`);
            }}
          />
        ),
      },
      {
        key: "pending",
        label: "Pending Req",
        sortable: true,
        width: "80px",
        render: (row) => row.pending,
      },
      {
        key: "lastRecv",
        label: "Last Recv",
        sortable: true,
        width: "100px",
        render: (row) => (
          <RelativeTimestamp
            basis={row.lastRecvBasis}
            basisLabel={row.lastRecvBasisLabel}
            basisTime={row.lastRecvBasisTime}
            eventTime={row.lastRecvEventTime}
            tone={row.lastRecvTone}
          />
        ),
      },
      {
        key: "lastSent",
        label: "Last Sent",
        sortable: true,
        width: "100px",
        render: (row) => {
          if (row.lastSentEventTime === null) return <span>&mdash;</span>;
          return (
            <RelativeTimestamp
              basis={row.lastSentBasis}
              basisLabel={row.lastSentBasisLabel}
              basisTime={row.lastSentBasisTime}
              eventTime={row.lastSentEventTime}
              tone={row.lastSentTone}
            />
          );
        },
      },
    ],
    [],
  );

  const tableSortedRows = useMemo(() => {
    const healthOrder: Record<string, number> = {
      healthy: 1,
      warning: 2,
      critical: 3,
      ok: 1,
      warn: 2,
      crit: 3,
    };
    const by =
      tableSortKey === "connection"
        ? (row: DemoConnectionRow) => row.connectionLabel
        : tableSortKey === "pending"
          ? (row: DemoConnectionRow) => row.pending
          : tableSortKey === "lastRecv"
            ? (row: DemoConnectionRow) => Date.parse(row.lastRecvEventTime)
            : tableSortKey === "lastSent"
              ? (row: DemoConnectionRow) =>
                  row.lastSentEventTime === null
                    ? Number.NEGATIVE_INFINITY
                    : Date.parse(row.lastSentEventTime)
              : (row: DemoConnectionRow) => healthOrder[row.healthTone];
    const direction = tableSortDir === "asc" ? 1 : -1;

    return [...tableRows].sort((a, b) => {
      const aValue = by(a);
      const bValue = by(b);
      if (typeof aValue === "number" && typeof bValue === "number")
        return (aValue - bValue) * direction;
      return String(aValue).localeCompare(String(bValue), undefined, { numeric: true }) * direction;
    });
  }, [tableRows, tableSortDir, tableSortKey]);

  const onTableSort = useCallback(
    (key: string) => {
      if (!tableColumns.some((column) => column.key === key && column.sortable)) return;
      if (tableSortKey === key) {
        setTableSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
        return;
      }
      setTableSortKey(key);
      setTableSortDir("desc");
    },
    [tableColumns, tableSortKey],
  );

  const searchResults = useMemo(() => {
    const needle = searchValue.trim().toLowerCase();
    return searchDataset
      .filter((item) => !searchOnlyKind || item.kind === searchOnlyKind)
      .filter((item) => {
        if (needle.length === 0) return true;
        return (
          item.label.toLowerCase().includes(needle) ||
          item.id.toLowerCase().includes(needle) ||
          item.process.toLowerCase().includes(needle) ||
          item.kind.toLowerCase().includes(needle)
        );
      })
      .slice(0, 6);
  }, [searchDataset, searchOnlyKind, searchValue]);
  const showSearchResults = searchValue.trim().length > 0 || searchOnlyKind !== null;
  const selectOptions = useMemo(
    () => [
      { value: "all", label: "All" },
      { value: "warn", label: "Warning+" },
      { value: "crit", label: "Critical" },
    ],
    [],
  );
  const nodeTypeMenu = useMemo(
    () => [
      { id: "show-kind", label: "Show only this kind" },
      { id: "hide-kind", label: "Hide this kind" },
      { id: "reset", label: "Reset filters", danger: true },
    ],
    [],
  );
  const processMenu = useMemo(
    () => [
      { id: "open-resources", label: "Open in Resources" },
      { id: "show-process", label: "Show only this process" },
      { id: "hide-process", label: "Hide this process" },
    ],
    [],
  );

  const scopeLightPalette = useMemo(
    () => SCOPE_LIGHT_RGB.map(([r, g, b]) => `rgb(${r} ${g} ${b})`),
    [],
  );
  const scopeDarkPalette = useMemo(
    () => SCOPE_DARK_RGB.map(([r, g, b]) => `rgb(${r} ${g} ${b})`),
    [],
  );

  const demoSnap = useMemo<SnapshotState>(
    () => ({
      phase: "ready",
      entities: [],
      edges: [],
      scopes: [],
      events: [],
      backtracesById: new Map(),
    }),
    [],
  );

  const demoRecording = useMemo<RecordingState>(
    () => ({
      phase: "idle",
    }),
    [],
  );

  const sampleGraphEntities = useMemo<EntityDef[]>(() => {
    const now = Date.now();
    const base = {
      processName: "moire-example",
      processPid: 12345,
      source: fakeSource("moire-example", "main.rs", 1),
      krate: "moire-example",
      birthPtime: 100,
      ageMs: 0,
      birthApproxUnixMs: now,
      inCycle: false,
      meta: {},
    };
    return [
      {
        ...base,
        id: "mutex_a",
        processId: "p1",
        processName: "vx-store",
        source: fakeSource("tokio", "sync/mutex.rs", 42),
        krate: "tokio",
        name: "store.state_lock",
        kind: "lock",
        body: { lock: { kind: "mutex" } },
        status: { label: "held", tone: "crit" as const },
      },
      {
        ...base,
        id: "future_a",
        processId: "p1",
        processName: "vx-store",
        source: fakeSource("roam-session", "session.rs", 88),
        krate: "roam-session",
        name: "store.incoming.recv",
        kind: "future",
        body: { future: {} },
        status: { label: "polling", tone: "neutral" as const },
        ageMs: 250,
      },
      {
        ...base,
        id: "sem_a",
        processId: "p1",
        processName: "vx-store",
        source: fakeSource("tokio", "sync/semaphore.rs", 12),
        krate: "tokio",
        name: "request_limiter",
        kind: "semaphore",
        body: { semaphore: { max_permits: 16, handed_out_permits: 14 } },
        status: { label: "2/16 permits", tone: "warn" as const },
        stat: "2/16",
        statTone: "warn" as const,
      },
      {
        ...base,
        id: "future_b",
        processId: "p2",
        processName: "vx-runner",
        source: fakeSource("roam-session", "rpc.rs", 17),
        krate: "roam-session",
        name: "DemoRpc.sleepy_forever",
        kind: "request",
        body: { request: { service_name: "DemoRpc", method_name: "sleepy_forever", args_json: "()" } },
        status: { label: "in_flight", tone: "warn" as const },
        ageMs: 4800,
      },
      {
        ...base,
        id: "future_c",
        processId: "p2",
        processName: "vx-runner",
        source: fakeSource("moire-examples", "main.rs", 20),
        krate: "moire-examples",
        name: "runner.tick",
        kind: "future",
        body: { future: {} },
        status: { label: "polling", tone: "neutral" as const },
      },
    ] as EntityDef[];
  }, []);

  const sampleGraphEdges = useMemo(
    () => [
      {
        id: "e1",
        source: "p1/future_a",
        target: "p1/mutex_a",
        kind: "waiting_on" as const,
      },
      {
        id: "e2",
        source: "mutex_a",
        target: "sem_a",
        kind: "holds" as const,
      },
      {
        id: "e3",
        source: "future_b",
        target: "sem_a",
        kind: "waiting_on" as const,
      },
      {
        id: "e4",
        source: "future_c",
        target: "future_b",
        kind: "polls" as const,
      },
    ],
    [],
  );

  return {
    textValue,
    setTextValue,
    searchValue,
    setSearchValue,
    checked,
    setChecked,
    selectValue,
    setSelectValue,
    sliderValue,
    setSliderValue,
    lastMenuPick,
    pickMenuAction,
    searchOnlyKind,
    setSearchOnlyKind,
    selectedSearchId,
    setSelectedSearchId,
    segmentedMode,
    setSegmentedMode,
    segmentedSeverity,
    setSegmentedSeverity,
    tableSortKey,
    tableSortDir,
    selectedTableRow,
    setSelectedTableRow,
    hiddenKinds,
    hiddenProcesses,
    hiddenCrates,
    leftPaneTab,
    setLeftPaneTab,
    showLoners,
    setShowLoners,
    isLive,
    setIsLive,
    focusedEntityId,
    setFocusedEntityId,
    fileInputRef,
    tones,
    searchDataset,
    filterKindItems,
    filterProcessItems,
    filterCrateItems,
    toggleKind,
    soloKind,
    toggleProcess,
    soloProcess,
    toggleCrate,
    soloCrate,
    tableRows,
    tableColumns,
    tableSortedRows,
    onTableSort,
    selectOptions,
    nodeTypeMenu,
    processMenu,
    scopeLightPalette,
    scopeDarkPalette,
    demoSnap,
    demoRecording,
    showSearchResults,
    searchResults,
    graphFilterText,
    setGraphFilterText,
    graphFilterNodeIds,
    graphFilterLocations,
    graphFilterFocusItems,
    sampleGraphEntities,
    sampleGraphEdges,
  };
}

export type StorybookState = ReturnType<typeof useStorybookState>;

