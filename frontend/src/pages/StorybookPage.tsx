import { useCallback, useMemo, useRef, useState } from "react";
import "./StorybookPage.css";
import {
  WarningCircle,
  CaretDown,
  CopySimple,
  ArrowSquareOut,
  FileRs,
  CircleNotch,
  LinkSimple,
  Package,
  PaperPlaneTilt,
  Terminal,
  Timer,
} from "@phosphor-icons/react";
import { Panel } from "../ui/layout/Panel";
import { PanelHeader } from "../ui/layout/PanelHeader";
import { Row } from "../ui/layout/Row";
import { Section } from "../ui/layout/Section";
import { Badge, type BadgeTone } from "../ui/primitives/Badge";
import { TextInput } from "../ui/primitives/TextInput";
import { SearchInput } from "../ui/primitives/SearchInput";
import { Checkbox } from "../ui/primitives/Checkbox";
import { Select } from "../ui/primitives/Select";
import { LabeledSlider } from "../ui/primitives/Slider";
import { Menu } from "../ui/primitives/Menu";
import { ContextMenu, ContextMenuItem, ContextMenuSeparator } from "../ui/primitives/ContextMenu";
import { FilterMenu, type FilterMenuItem } from "../ui/primitives/FilterMenu";
import { SegmentedGroup } from "../ui/primitives/SegmentedGroup";
import { KeyValueRow } from "../ui/primitives/KeyValueRow";
import { RelativeTimestamp } from "../ui/primitives/RelativeTimestamp";
import { DurationDisplay } from "../ui/primitives/DurationDisplay";
import { NodeChip } from "../ui/primitives/NodeChip";
import { Table, type Column } from "../ui/primitives/Table";
import { ActionButton } from "../ui/primitives/ActionButton";
import { kindIcon } from "../nodeKindSpec";
import { SCOPE_DARK_RGB, SCOPE_LIGHT_RGB } from "../components/graph/scopeColors";
import { AppHeader } from "../components/AppHeader";
import type { RecordingState, SnapshotState } from "../App";
import { Switch } from "../ui/primitives/Switch";
import type { EntityDef } from "../snapshot";
import { GraphNode } from "../components/graph/GraphNode";
import { GraphFilterInput } from "../components/graph/GraphFilterInput";
import { SampleGraph } from "../components/graph/SampleGraph";
import "../components/graph/GraphPanel.css";
import { InspectorPanel } from "../components/inspector/InspectorPanel";

type DemoTone = "neutral" | "ok" | "warn" | "crit";

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
  const [leftPaneTab, setLeftPaneTab] = useState<"graph" | "scopes" | "entities">("graph");
  const [showLoners, setShowLoners] = useState(false);
  const [isLive, setIsLive] = useState(false);
  const [focusedEntityId, setFocusedEntityId] = useState<string | null>(null);
  const [graphFilterText, setGraphFilterText] = useState(
    "colorBy:crate groupBy:process loners:off",
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
      { id: "peeps-collector", label: "peeps-collector" },
    ],
    [],
  );

  const filterCrateItems = useMemo<FilterMenuItem[]>(
    () => [
      { id: "roam-session", label: "roam-session" },
      { id: "peeps-examples", label: "peeps-examples" },
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
    }),
    [],
  );

  const demoRecording = useMemo<RecordingState>(
    () => ({
      phase: "idle",
    }),
    [],
  );

  const inspectorEntity = useMemo<EntityDef>(() => {
    const now = Date.now();
    const base = {
      processId: "101",
      processName: "peeps-examples",
      processPid: 84025,
      source: "tokio_runtime.rs:20",
      krate: "roam-session",
      birthPtime: 16,
      ageMs: 739,
      birthApproxUnixMs: now - 739,
      inCycle: false,
      meta: {},
    };
    const tx = {
      ...base,
      id: "101/chan_tx",
      rawEntityId: "chan_tx",
      name: "roam_driver:tx",
      kind: "channel_tx",
      body: {
        channel_tx: {
          lifecycle: "open",
          details: { mpsc: { buffer: { occupancy: 0, capacity: 256 } } },
        },
      },
      status: { label: "open", tone: "ok" as const },
      stat: "0/256",
      statTone: "ok" as const,
    } as unknown as EntityDef;
    const rx = {
      ...base,
      id: "101/chan_rx",
      rawEntityId: "chan_rx",
      name: "roam_driver:rx",
      kind: "channel_rx",
      body: {
        channel_rx: {
          lifecycle: "open",
          details: { mpsc: { buffer: { occupancy: 0, capacity: 256 } } },
        },
      },
      status: { label: "open", tone: "ok" as const },
      stat: "0/256",
      statTone: "ok" as const,
    } as unknown as EntityDef;

    return {
      ...base,
      id: "101/roam_driver",
      rawEntityId: "roam_driver",
      name: "roam_driver",
      kind: "channel_pair",
      body: tx.body,
      status: { label: "open", tone: "ok" as const },
      stat: "0/256",
      statTone: "ok" as const,
      channelPair: { tx, rx },
    } as unknown as EntityDef;
  }, []);

  const sampleGraphEntities = useMemo<EntityDef[]>(() => {
    const now = Date.now();
    const base = {
      processName: "peeps-example",
      processPid: 12345,
      source: "main.rs:1",
      krate: "peeps-example",
      birthPtime: 100,
      ageMs: 0,
      birthApproxUnixMs: now,
      inCycle: false,
      meta: {},
    };
    return [
      {
        ...base,
        id: "p1/mutex_a",
        rawEntityId: "mutex_a",
        processId: "p1",
        processName: "vx-store",
        krate: "tokio",
        name: "store.state_lock",
        kind: "lock",
        body: { lock: { kind: "mutex" } },
        status: { label: "held", tone: "crit" as const },
      },
      {
        ...base,
        id: "p1/future_a",
        rawEntityId: "future_a",
        processId: "p1",
        processName: "vx-store",
        krate: "roam-session",
        name: "store.incoming.recv",
        kind: "future",
        body: "future" as const,
        status: { label: "polling", tone: "neutral" as const },
        ageMs: 250,
      },
      {
        ...base,
        id: "p1/sem_a",
        rawEntityId: "sem_a",
        processId: "p1",
        processName: "vx-store",
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
        id: "p2/future_b",
        rawEntityId: "future_b",
        processId: "p2",
        processName: "vx-runner",
        krate: "roam-session",
        name: "DemoRpc.sleepy_forever",
        kind: "request",
        body: { request: { method: "DemoRpc.sleepy_forever", args_preview: "()" } },
        status: { label: "in_flight", tone: "warn" as const },
        ageMs: 4800,
      },
      {
        ...base,
        id: "p2/future_c",
        rawEntityId: "future_c",
        processId: "p2",
        processName: "vx-runner",
        krate: "peeps-examples",
        name: "runner.tick",
        kind: "future",
        body: "future" as const,
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
        kind: "needs" as const,
        meta: {},
        opKind: "lock",
        state: "pending",
      },
      {
        id: "e2",
        source: "p1/mutex_a",
        target: "p1/sem_a",
        kind: "holds" as const,
        meta: {},
      },
      {
        id: "e3",
        source: "p2/future_b",
        target: "p1/sem_a",
        kind: "needs" as const,
        meta: {},
        opKind: "acquire",
        state: "pending",
      },
      {
        id: "e4",
        source: "p2/future_c",
        target: "p2/future_b",
        kind: "touches" as const,
        meta: {},
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
    inspectorEntity,
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

export function StorybookPage({
  colorScheme,
  sharedState,
}: {
  colorScheme?: "dark" | "light";
  sharedState: StorybookState;
}) {
  const {
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
    filterKindItems,
    filterProcessItems,
    filterCrateItems,
    toggleKind,
    soloKind,
    toggleProcess,
    soloProcess,
    toggleCrate,
    soloCrate,
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
    inspectorEntity,
    showSearchResults,
    searchResults,
    graphFilterText,
    setGraphFilterText,
    graphFilterNodeIds,
    graphFilterLocations,
    graphFilterFocusItems,
    sampleGraphEntities,
    sampleGraphEdges,
  } = sharedState;

  const contextMenuContainerRef = useRef<HTMLDivElement>(null);
  const [contextMenuState, setContextMenuState] = useState<{
    x: number;
    y: number;
    nodeId: string;
  } | null>(null);
  const closeContextMenu = useCallback(() => setContextMenuState(null), []);

  const colorVariables = useMemo(() => {
    if (typeof window === "undefined") return [] as Array<{ name: string; value: string }>;
    const rootStyles = window.getComputedStyle(document.documentElement);
    const colorVarNamePattern = /(color|surface|border|text|focus|accent|control)/i;
    const colorValuePattern =
      /(#[0-9a-f]{3,8}\b|rgb\(|hsl\(|oklch\(|oklab\(|lab\(|lch\(|hwb\(|color\()/i;
    const vars: Array<{ name: string; value: string }> = [];

    for (let i = 0; i < rootStyles.length; i++) {
      const name = rootStyles.item(i);
      if (!name.startsWith("--")) continue;
      const value = rootStyles.getPropertyValue(name).trim();
      if (!value) continue;
      if (!colorVarNamePattern.test(name) && !colorValuePattern.test(value)) continue;
      vars.push({ name, value });
    }

    vars.sort((a, b) => a.name.localeCompare(b.name));
    return vars;
  }, []);
  const roleTokenVariables = useMemo(() => {
    const order = [
      "--bg-base",
      "--bg-surface",
      "--bg-elevated",
      "--border-subtle",
      "--border-default",
      "--text-primary",
      "--text-secondary",
      "--text-muted",
      "--accent",
      "--accent-hover",
      "--accent-active",
      "--status-success",
      "--status-warning",
      "--status-danger",
      "--focus-ring",
      "--shadow-soft",
      "--shadow-strong",
    ];
    const byName = new Map(colorVariables.map((item) => [item.name, item] as const));
    return order.flatMap((name) => {
      const value = byName.get(name);
      return value ? [value] : [];
    });
  }, [colorVariables]);

  return (
    <Panel
      variant="lab"
      style={{
        ...(colorScheme
          ? { colorScheme, background: "var(--bg-base)", color: "var(--text-primary)" }
          : undefined),
      }}
    >
      <PanelHeader title="Storybook" hint="Primitives and tone language" />
      <div className="lab-body">
        <Section title="Control Strip" subtitle="All core controls on one line" wide>
          <div className="ui-control-line" aria-label="One-line controls strip">
            <div className="ui-control-line__text">
              <TextInput
                value={textValue}
                onChange={setTextValue}
                placeholder="Type…"
                aria-label="Control strip text input"
              />
            </div>
            <div className="ui-control-line__search">
              <SearchInput
                value={searchValue}
                onChange={setSearchValue}
                placeholder="Search nodes…"
                aria-label="Control strip search input"
                items={searchResults.map((item) => ({
                  id: item.id,
                  label: item.label,
                  meta: item.process,
                }))}
                showSuggestions={showSearchResults}
                selectedId={selectedSearchId}
                onSelect={(id) => setSelectedSearchId(id)}
                onAltSelect={(id) => {
                  const item = searchResults.find((entry) => entry.id === id);
                  if (!item) return;
                  setSearchOnlyKind((prev) => (prev === item.kind ? null : item.kind));
                }}
              />
            </div>
            <FilterMenu
              label="Kinds"
              items={filterKindItems}
              hiddenIds={hiddenKinds}
              onToggle={toggleKind}
              onSolo={soloKind}
            />
            <FilterMenu
              label="Processes"
              items={filterProcessItems}
              hiddenIds={hiddenProcesses}
              onToggle={toggleProcess}
              onSolo={soloProcess}
            />
            <Menu
              label={
                <span className="ui-menu-label">
                  Node types <CaretDown size={12} weight="bold" />
                </span>
              }
              items={nodeTypeMenu}
            />
            <Menu
              label={
                <span className="ui-menu-label">
                  Process <CaretDown size={12} weight="bold" />
                </span>
              }
              items={processMenu}
            />
            <ActionButton>Default</ActionButton>
            <ActionButton variant="primary">Primary</ActionButton>
            <SegmentedGroup
              value={segmentedMode}
              onChange={setSegmentedMode}
              options={[
                { value: "graph", label: "Graph" },
                { value: "timeline", label: "Timeline" },
                { value: "resources", label: "Resources" },
              ]}
              aria-label="Control strip mode switcher"
            />
          </div>
        </Section>

        <Section
          title="App Slice (Fake Data)"
          subtitle="Header, controls row, and floating inspector composed from real components"
          wide
        >
          <div className="ui-app-slice">
            <AppHeader
              leftPaneTab={leftPaneTab}
              onLeftPaneTabChange={setLeftPaneTab}
              snap={demoSnap}
              snapshotProcessCount={0}
              recording={demoRecording}
              connCount={0}
              isBusy={false}
              isLive={isLive}
              onSetIsLive={setIsLive}
              onShowProcessModal={() => undefined}
              onTakeSnapshot={() => undefined}
              onStartRecording={() => undefined}
              onStopRecording={() => undefined}
              onExport={() => undefined}
              onImportClick={() => fileInputRef.current?.click()}
              fileInputRef={fileInputRef}
              onImportFile={() => undefined}
            />
            <div className="ui-app-slice-controls">
              <Switch
                checked={showLoners}
                onChange={setShowLoners}
                label="Show loners"
                className="ui-app-slice-switch"
              />
              <FilterMenu
                label="Process"
                items={filterProcessItems}
                hiddenIds={hiddenProcesses}
                onToggle={toggleProcess}
                onSolo={soloProcess}
              />
              <FilterMenu
                label="Crate"
                items={filterCrateItems}
                hiddenIds={hiddenCrates}
                onToggle={toggleCrate}
                onSolo={soloCrate}
              />
              <FilterMenu
                label="Kind"
                items={filterKindItems}
                hiddenIds={hiddenKinds}
                onToggle={toggleKind}
                onSolo={soloKind}
              />
            </div>
            <div className="ui-floating-inspector">
              <InspectorPanel
                onClose={() => undefined}
                selection={{ kind: "entity", id: inspectorEntity.id }}
                entityDefs={[inspectorEntity]}
                edgeDefs={[]}
                focusedEntityId={focusedEntityId}
                onToggleFocusEntity={(id) => setFocusedEntityId(id)}
                onOpenScopeKind={() => undefined}
              />
            </div>
            <div className="ui-graph-node-strip">
              <GraphNode
                data={{
                  kind: "future",
                  label: "roam.call.await_response",
                  inCycle: false,
                  selected: false,
                  status: { label: "polling", tone: "neutral" },
                  ageMs: 688,
                  stat: "N+1ms",
                  statTone: "warn",
                }}
              />
              <GraphNode
                data={{
                  kind: "channel_pair",
                  label: "roam_driver",
                  inCycle: false,
                  selected: false,
                  status: { label: "open", tone: "ok" },
                  ageMs: 739,
                  stat: "0/256",
                  statTone: "ok",
                  portTopId: "sample-channel-pair:rx",
                  portBottomId: "sample-channel-pair:tx",
                }}
              />
              <GraphNode
                data={{
                  kind: "rpc_pair",
                  label: "roam.call.await_response",
                  inCycle: false,
                  selected: false,
                  status: { label: "pending", tone: "warn" },
                  ageMs: 721,
                  stat: "RESP pending",
                  statTone: "warn",
                  portTopId: "sample-rpc-pair:resp",
                  portBottomId: "sample-rpc-pair:req",
                }}
              />
            </div>
          </div>
        </Section>

        <Section
          title="Color System"
          subtitle="Role tokens + scope palette (graph only)"
          wide
          collapsible
          defaultCollapsed
        >
          <div className="ui-section-stack">
            <div className="ui-color-vars">
              <div className="ui-typo-kicker">
                Semantic Role Tokens ({roleTokenVariables.length})
              </div>
              <div className="ui-color-var-grid">
                {roleTokenVariables.map(({ name, value }) => (
                  <div key={name} className="ui-color-var-row">
                    <span className="ui-color-var-row__swatch" style={{ background: value }} />
                    <span className="ui-color-var-row__name">{name}</span>
                    <span className="ui-color-var-row__value">{value}</span>
                  </div>
                ))}
              </div>
            </div>

            <div className="ui-color-groups">
              <div className="ui-color-group">
                <div className="ui-typo-kicker">Scope Palette (Light)</div>
                <div className="ui-color-grid">
                  {scopeLightPalette.map((color, index) => (
                    <div key={`light-${index}`} className="ui-color-chip">
                      <span className="ui-color-chip__swatch" style={{ backgroundColor: color }} />
                      <span className="ui-color-chip__label">{color}</span>
                    </div>
                  ))}
                </div>
              </div>
              <div className="ui-color-group">
                <div className="ui-typo-kicker">Scope Palette (Dark)</div>
                <div className="ui-color-grid">
                  {scopeDarkPalette.map((color, index) => (
                    <div key={`dark-${index}`} className="ui-color-chip">
                      <span className="ui-color-chip__swatch" style={{ backgroundColor: color }} />
                      <span className="ui-color-chip__label">{color}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </Section>

        <Section title="Buttons" subtitle="Variants, sizes, and icon combinations">
          <div className="ui-section-stack">
            <Row>
              <ActionButton>Default</ActionButton>
              <ActionButton variant="primary">Primary</ActionButton>
              <ActionButton variant="ghost">Ghost</ActionButton>
              <ActionButton isDisabled>Disabled</ActionButton>
            </Row>
            <Row>
              <ActionButton>
                <WarningCircle size={14} weight="bold" />
                With icon
              </ActionButton>
              <ActionButton>
                <CopySimple size={12} weight="bold" />
                Copy
              </ActionButton>
              <ActionButton>
                <ArrowSquareOut size={12} weight="bold" />
                Open
              </ActionButton>
            </Row>
            <Row>
              <ActionButton size="sm">Small</ActionButton>
              <ActionButton size="sm" aria-label="Copy">
                <CopySimple size={12} weight="bold" />
              </ActionButton>
            </Row>
          </div>
        </Section>

        <Section title="Badges" subtitle="Single token primitive with variants">
          <div className="ui-section-stack">
            <Row>
              {tones.map((tone) => (
                <Badge key={`standard-${tone}`} tone={tone}>
                  {tone.toUpperCase()}
                </Badge>
              ))}
            </Row>
            <Row>
              {tones.map((tone) => (
                <Badge key={`count-${tone}`} tone={tone} variant="count">
                  {tone === "neutral" ? "0" : tone === "ok" ? "3" : tone === "warn" ? "7" : "118"}
                </Badge>
              ))}
            </Row>
          </div>
        </Section>

        <Section title="Text Input" subtitle="Plain text field">
          <TextInput
            value={textValue}
            onChange={setTextValue}
            placeholder="Type…"
            aria-label="Text input"
          />
        </Section>

        <Section title="Search" subtitle="Autocomplete with results, filters, and selection" wide>
          <SearchInput
            value={searchValue}
            onChange={setSearchValue}
            placeholder="Search nodes…"
            aria-label="Search input"
            items={searchResults.map((item) => ({
              id: item.id,
              label: <NodeChip kind={item.kind} label={item.label} />,
              meta: item.process,
            }))}
            showSuggestions={showSearchResults}
            selectedId={selectedSearchId}
            resultHint={
              <>
                <span>{searchResults.length} result(s)</span>
                <span className="ui-search-results-hint">
                  click to select &middot; alt+click to filter only this kind
                </span>
              </>
            }
            filterBadge={
              searchOnlyKind ? <Badge tone="neutral">{`kind:${searchOnlyKind}`}</Badge> : undefined
            }
            onClearFilter={() => setSearchOnlyKind(null)}
            onSelect={(id) => setSelectedSearchId(id)}
            onAltSelect={(id) => {
              const item = searchResults.find((entry) => entry.id === id);
              if (!item) return;
              setSearchOnlyKind((prev) => (prev === item.kind ? null : item.kind));
            }}
          />
        </Section>

        <Section title="Graph Filter" subtitle="Token-based filter bar with autocomplete" wide>
          <GraphFilterInput
            focusedEntityId={null}
            onExitFocus={() => {}}
            graphFilterText={graphFilterText}
            onGraphFilterTextChange={setGraphFilterText}
            crateItems={filterCrateItems}
            processItems={filterProcessItems}
            kindItems={filterKindItems}
            nodeIds={graphFilterNodeIds}
            locations={graphFilterLocations}
            focusItems={graphFilterFocusItems}
          />
        </Section>

        <Section
          title="Sample Graph"
          subtitle="ELK layout + full renderer — two processes, five nodes"
          wide
        >
          <div className="ui-sample-graph">
            <SampleGraph
              entityDefs={sampleGraphEntities}
              edgeDefs={sampleGraphEdges}
              scopeColorMode="process"
              subgraphScopeMode="process"
            />
          </div>
        </Section>

        <Section title="Controls" subtitle="Checkbox, select">
          <Row className="ui-row--controls">
            <Checkbox checked={checked} onChange={setChecked} label="Show resources" />
            <Select
              value={selectValue}
              onChange={(next) => setSelectValue(next)}
              aria-label="Select"
              options={selectOptions}
            />
          </Row>
        </Section>

        <Section title="Slider" subtitle="Labeled slider with discrete steps">
          <LabeledSlider
            value={sliderValue}
            min={0}
            max={2}
            step={1}
            onChange={(v) => setSliderValue(v)}
            aria-label="Detail level"
            label="Detail"
            valueLabel={sliderValue === 0 ? "info" : sliderValue === 1 ? "debug" : "trace"}
          />
        </Section>

        <Section title="Menu" subtitle="Action menus for context operations">
          <p className="ui-section-description">
            <strong>Click</strong> to open. Click the trigger again or click outside to close. Click
            an item to trigger it. <strong>Press and hold</strong> the trigger to open, then drag to
            an item and release — the action fires without a separate click. You can also drag from
            one menu trigger to another while holding to switch menus mid-drag.
          </p>
          <Row>
            <Menu
              label={
                <span className="ui-menu-label">
                  <span>Node types</span>
                  <CaretDown size={12} weight="bold" />
                </span>
              }
              items={nodeTypeMenu}
              onAction={(id) => pickMenuAction("node types", id)}
            />
            <Menu
              label={
                <span className="ui-menu-label">
                  Process <CaretDown size={12} weight="bold" />
                </span>
              }
              items={processMenu}
              onAction={(id) => pickMenuAction("process", id)}
            />
          </Row>
          {lastMenuPick && (
            <div className="ui-lab-event">
              You picked: <strong>{lastMenuPick}</strong>
            </div>
          )}
        </Section>

        <Section title="Context Menu" subtitle="Right-click triggered overlay for node actions">
          <p className="ui-section-description">
            Right-click any node chip below to open the context menu at the cursor position. Dismiss
            with Escape, click outside, or pick an action.
          </p>
          <div ref={contextMenuContainerRef} className="ui-context-menu-demo">
            <Row>
              {CONTEXT_MENU_DEMO_NODES.map((node) => (
                <NodeChip
                  key={node.id}
                  kind={node.kind}
                  label={node.label}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    const rect = contextMenuContainerRef.current?.getBoundingClientRect();
                    if (!rect) return;
                    setContextMenuState({
                      x: event.clientX - rect.left,
                      y: event.clientY - rect.top,
                      nodeId: node.id,
                    });
                  }}
                />
              ))}
            </Row>
            {contextMenuState &&
              (() => {
                const node = CONTEXT_MENU_DEMO_NODES.find((n) => n.id === contextMenuState.nodeId);
                if (!node) return null;
                return (
                  <ContextMenu
                    x={contextMenuState.x}
                    y={contextMenuState.y}
                    onClose={closeContextMenu}
                  >
                    <ContextMenuItem
                      onClick={() => {
                        pickMenuAction("context", "focus-connected");
                        closeContextMenu();
                      }}
                    >
                      Show only connected
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem
                      prefix="−"
                      onClick={() => {
                        pickMenuAction("context", "hide-node");
                        closeContextMenu();
                      }}
                    >
                      Hide this node
                    </ContextMenuItem>
                    {node.location && (
                      <ContextMenuItem
                        prefix="−"
                        onClick={() => {
                          pickMenuAction("context", `hide-location:${node.location}`);
                          closeContextMenu();
                        }}
                      >
                        <NodeChip
                          icon={<FileRs size={12} weight="bold" />}
                          label={node.location.split("/").pop() ?? node.location}
                        />
                      </ContextMenuItem>
                    )}
                    <ContextMenuSeparator />
                    <ContextMenuItem
                      prefix="−"
                      onClick={() => {
                        pickMenuAction("context", `hide-crate:${node.krate}`);
                        closeContextMenu();
                      }}
                    >
                      <NodeChip icon={<Package size={12} weight="bold" />} label={node.krate} />
                    </ContextMenuItem>
                    <ContextMenuItem
                      prefix="+"
                      onClick={() => {
                        pickMenuAction("context", `solo-crate:${node.krate}`);
                        closeContextMenu();
                      }}
                    >
                      <NodeChip icon={<Package size={12} weight="bold" />} label={node.krate} />
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem
                      prefix="−"
                      onClick={() => {
                        pickMenuAction("context", `hide-process:${node.processId}`);
                        closeContextMenu();
                      }}
                    >
                      <NodeChip
                        icon={<Terminal size={12} weight="bold" />}
                        label={node.processLabel}
                      />
                    </ContextMenuItem>
                    <ContextMenuItem
                      prefix="+"
                      onClick={() => {
                        pickMenuAction("context", `solo-process:${node.processId}`);
                        closeContextMenu();
                      }}
                    >
                      <NodeChip
                        icon={<Terminal size={12} weight="bold" />}
                        label={node.processLabel}
                      />
                    </ContextMenuItem>
                    <ContextMenuSeparator />
                    <ContextMenuItem
                      prefix="−"
                      onClick={() => {
                        pickMenuAction("context", `hide-kind:${node.kind}`);
                        closeContextMenu();
                      }}
                    >
                      <NodeChip icon={kindIcon(node.kind, 12)} label={node.kindLabel} />
                    </ContextMenuItem>
                    <ContextMenuItem
                      prefix="+"
                      onClick={() => {
                        pickMenuAction("context", `solo-kind:${node.kind}`);
                        closeContextMenu();
                      }}
                    >
                      <NodeChip icon={kindIcon(node.kind, 12)} label={node.kindLabel} />
                    </ContextMenuItem>
                  </ContextMenu>
                );
              })()}
          </div>
          {lastMenuPick && (
            <div className="ui-lab-event">
              You picked: <strong>{lastMenuPick}</strong>
            </div>
          )}
        </Section>

        <Section title="Filter Menu" subtitle="Multi-select with checkboxes, alt-click to solo">
          <Row>
            <FilterMenu
              label="Node types"
              items={filterKindItems}
              hiddenIds={hiddenKinds}
              onToggle={toggleKind}
              onSolo={soloKind}
            />
            <FilterMenu
              label="Processes"
              items={filterProcessItems}
              hiddenIds={hiddenProcesses}
              onToggle={toggleProcess}
              onSolo={soloProcess}
            />
          </Row>
        </Section>

        <Section title="Segmented Group" subtitle="Mutually-exclusive mode and severity controls">
          <div className="ui-section-stack">
            <SegmentedGroup
              value={segmentedMode}
              onChange={setSegmentedMode}
              options={[
                { value: "graph", label: "Graph" },
                { value: "timeline", label: "Timeline" },
                { value: "resources", label: "Resources" },
              ]}
              aria-label="Mode switcher"
            />
            <SegmentedGroup
              value={segmentedSeverity}
              onChange={setSegmentedSeverity}
              options={[
                { value: "all", label: "All" },
                { value: "warn", label: "Warning+" },
                { value: "crit", label: "Critical" },
              ]}
              aria-label="Severity filter"
            />
          </div>
        </Section>

        <Section title="Key-Value Rows" subtitle="Inspector-like metadata rows">
          <div className="ui-section-stack">
            <KeyValueRow
              label="Method"
              labelWidth={80}
              icon={<PaperPlaneTilt size={12} weight="bold" />}
            >
              DemoRpc.sleepy_forever
            </KeyValueRow>
            <KeyValueRow label="Source" labelWidth={80}>
              <NodeChip
                icon={<FileRs size={12} weight="bold" />}
                label="main.rs:20"
                href="zed://file/%2Fapp%2Fsrc%2Fmain.rs%3A20"
                title="Open /app/src/main.rs:20 in editor"
              />
            </KeyValueRow>
            <KeyValueRow
              label="Status"
              labelWidth={80}
              icon={<CircleNotch size={12} weight="bold" />}
            >
              <Badge tone="warn">IN_FLIGHT</Badge>
            </KeyValueRow>
            <KeyValueRow label="Elapsed" labelWidth={80}>
              <DurationDisplay ms={1245000} tone="crit" />
            </KeyValueRow>
            <KeyValueRow
              label="Connection"
              labelWidth={80}
              icon={<LinkSimple size={12} weight="bold" />}
            >
              <NodeChip
                kind="connection"
                label="initiator→acceptor"
                onClick={() => console.log("inspect initiator→acceptor")}
                onContextMenu={(event) => {
                  event.preventDefault();
                  console.log("open context for initiator→acceptor");
                }}
              />
            </KeyValueRow>
            <KeyValueRow label="Opened" labelWidth={80} icon={<Timer size={12} weight="bold" />}>
              <RelativeTimestamp
                basis="P"
                basisLabel="process started"
                basisTime="2026-02-17T10:06:00.000Z"
                eventTime="2026-02-17T10:06:06.000Z"
              />
            </KeyValueRow>
            <KeyValueRow label="Closed" labelWidth={80} icon={<Timer size={12} weight="bold" />}>
              <RelativeTimestamp
                basis="N"
                basisLabel="connection opened"
                basisTime="2026-02-17T10:06:00.000Z"
                eventTime="2026-02-17T10:07:05.000Z"
              />
            </KeyValueRow>
            <KeyValueRow label="Pending" labelWidth={80}>
              3
            </KeyValueRow>
          </div>
        </Section>

        <Section title="Relative Timestamps" subtitle="P/N deltas with tooltip context">
          <Row className="ui-relative-timestamp-group">
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp
                basis="P"
                basisLabel="6 seconds after process start"
                basisTime="2026-02-17T10:00:00.000Z"
                eventTime="2026-02-17T10:00:06.000Z"
              />
              <span className="ui-relative-timestamp-caption">process start</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp
                basis="P"
                basisLabel="2 minutes 30 seconds after process start"
                basisTime="2026-02-17T10:00:00.000Z"
                eventTime="2026-02-17T10:02:30.000Z"
              />
              <span className="ui-relative-timestamp-caption">process start</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp
                basis="N"
                basisLabel="node just created"
                basisTime="2026-02-17T10:00:30.000Z"
                eventTime="2026-02-17T10:00:30.000Z"
                tone="ok"
              />
              <span className="ui-relative-timestamp-caption">node created</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp
                basis="N"
                basisLabel="1m5s after node open"
                basisTime="2026-02-17T10:00:30.000Z"
                eventTime="2026-02-17T10:01:35.000Z"
                tone="warn"
              />
              <span className="ui-relative-timestamp-caption">node-relative</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp
                basis="N"
                basisLabel="stuck for 20 minutes"
                basisTime="2026-02-17T10:00:30.000Z"
                eventTime="2026-02-17T10:21:15.000Z"
                tone="crit"
              />
              <span className="ui-relative-timestamp-caption">stuck 20m</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp
                basis="N"
                basisLabel="sub-second timing check"
                basisTime="2026-02-17T10:00:30.000Z"
                eventTime="2026-02-17T10:00:30.145Z"
              />
              <span className="ui-relative-timestamp-caption">sub-second</span>
            </div>
          </Row>
        </Section>

        <Section title="Duration Display" subtitle="Automatic semantic coloring by magnitude">
          <Row className="ui-duration-row">
            <DurationDisplay ms={200} />
            <DurationDisplay ms={6200} />
            <DurationDisplay ms={45000} />
            <DurationDisplay ms={150000} />
            <DurationDisplay ms={1245000} />
            <DurationDisplay ms={4320000} />
          </Row>
        </Section>

        <Section title="Node Chips" subtitle="Inline clickable node/resource references">
          <Row>
            <NodeChip
              kind="connection"
              label="initiator→acceptor"
              onClick={() => console.log("open connection chip")}
              onContextMenu={(event) => {
                event.preventDefault();
                console.log("show connection context menu");
              }}
            />
            <NodeChip
              kind="request"
              label="DemoRpc.sleepy_forever"
              onClick={() => console.log("open request chip")}
              onContextMenu={(event) => {
                event.preventDefault();
                console.log("show request context menu");
              }}
            />
            <NodeChip
              kind="channel_tx"
              label="mpsc.send"
              onClick={() => console.log("open channel chip")}
              onContextMenu={(event) => {
                event.preventDefault();
                console.log("show channel context menu");
              }}
            />
            <NodeChip
              label="example-roam-rpc-stuck-request"
              onClick={() => console.log("open generic chip")}
              onContextMenu={(event) => {
                event.preventDefault();
                console.log("show generic chip context menu");
              }}
            />
          </Row>
          <div className="ui-lab-hint">Left-click to navigate, right-click for actions</div>
        </Section>

        <Section title="Table" subtitle="Sortable, sticky header, selectable rows" wide>
          <Table
            columns={tableColumns}
            rows={tableSortedRows}
            rowKey={(row) => row.id}
            sortKey={tableSortKey}
            sortDir={tableSortDir}
            selectedRowKey={selectedTableRow}
            onSort={onTableSort}
            onRowClick={(row) => setSelectedTableRow(row.id)}
            aria-label="Demo connections table"
          />
        </Section>
      </div>
    </Panel>
  );
}
