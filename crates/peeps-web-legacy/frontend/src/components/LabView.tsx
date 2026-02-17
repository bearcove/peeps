import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  Handle,
  Position,
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  type Node,
  type Edge,
  type EdgeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import ELK from "elkjs/lib/elk-api.js";
import elkWorkerUrl from "elkjs/lib/elk-worker.min.js?url";
import {
  WarningCircle,
  CaretDown,
  CopySimple,
  ArrowSquareOut,
  FileRs,
  CircleNotch,
  LinkSimple,
  PaperPlaneTilt,
  Timer,
  Aperture,
  Camera,
  CheckCircle,
  MagnifyingGlass,
  CaretLeft,
  CaretRight,
  Crosshair,
  Hash,
  Users,
} from "@phosphor-icons/react";
import { Panel } from "../ui/layout/Panel";
import { PanelHeader } from "../ui/layout/PanelHeader";
import { Row } from "../ui/layout/Row";
import { Section } from "../ui/layout/Section";
import { SplitLayout } from "../ui/layout/SplitLayout";
import { Badge, type BadgeTone } from "../ui/primitives/Badge";
import { TextInput } from "../ui/primitives/TextInput";
import { SearchInput } from "../ui/primitives/SearchInput";
import { Checkbox } from "../ui/primitives/Checkbox";
import { Select } from "../ui/primitives/Select";
import { LabeledSlider } from "../ui/primitives/Slider";
import { Menu } from "../ui/primitives/Menu";
import { FilterMenu, type FilterMenuItem } from "../ui/primitives/FilterMenu";
import { SegmentedGroup } from "../ui/primitives/SegmentedGroup";
import { KeyValueRow } from "../ui/primitives/KeyValueRow";
import { RelativeTimestamp } from "../ui/primitives/RelativeTimestamp";
import { DurationDisplay } from "../ui/primitives/DurationDisplay";
import { NodeChip } from "../ui/primitives/NodeChip";
import { ProcessIdenticon } from "../ui/primitives/ProcessIdenticon";
import { Table, type Column } from "../ui/primitives/Table";
import { ActionButton } from "../ui/primitives/ActionButton";
import { kindIcon } from "../nodeKindSpec";

// ── ELK layout engine (same as real app) ──────────────────────

const elk = new ELK({ workerUrl: elkWorkerUrl });

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "24",
  "elk.layered.spacing.nodeNodeBetweenLayers": "48",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
  "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
};

// ── Mock data for the deadlock detector mockup ────────────────

type MockNodeDef = {
  id: string;
  kind: string;
  label: string;
  inCycle?: boolean;
};

const MOCK_NODE_DEFS: MockNodeDef[] = [
  { id: "future:store.incoming.recv", kind: "future", label: "store.incoming.recv" },
  { id: "request:demorpc.sleepy", kind: "request", label: "DemoRpc.sleepy_forever", inCycle: true },
  { id: "request:demorpc.ping", kind: "request", label: "DemoRpc.ping" },
  { id: "mutex:global_state", kind: "mutex", label: "Mutex<GlobalState>", inCycle: true },
  { id: "channel:mpsc.tx", kind: "channel_tx", label: "mpsc.send", inCycle: true },
  { id: "channel:mpsc.rx", kind: "channel_rx", label: "mpsc.recv", inCycle: true },
  { id: "connection:initiator", kind: "connection", label: "initiator\u2192acceptor" },
];

/** Measure actual rendered node dimensions by briefly inserting into the DOM. */
function measureNodeDefs(defs: MockNodeDef[]): Map<string, { width: number; height: number }> {
  const container = document.createElement("div");
  container.style.cssText = "position:fixed;top:-9999px;left:-9999px;visibility:hidden;pointer-events:none;display:flex;flex-direction:column;align-items:flex-start;gap:4px;";
  document.body.appendChild(container);

  // Create all elements first, then measure — single reflow
  const elements: { id: string; el: HTMLDivElement }[] = [];
  for (const def of defs) {
    const el = document.createElement("div");
    el.className = `mockup-node${def.inCycle ? " mockup-node--cycle" : ""}`;
    // Replicate the node's inner structure
    const icon = document.createElement("span");
    icon.className = "mockup-node-icon";
    icon.style.cssText = "display:inline-flex;align-items:center;justify-content:center;width:14px;height:14px;flex-shrink:0;";
    const label = document.createElement("span");
    label.className = "mockup-node-label";
    label.textContent = def.label;
    el.appendChild(icon);
    el.appendChild(label);
    container.appendChild(el);
    elements.push({ id: def.id, el });
  }

  // Force layout, then measure all
  const sizes = new Map<string, { width: number; height: number }>();
  for (const { id, el } of elements) {
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    sizes.set(id, { width: w, height: h });
  }

  document.body.removeChild(container);
  return sizes;
}

type MockEdgeDef = {
  id: string;
  source: string;
  target: string;
  rel: "needs" | "spawned" | "touches" | "default";
};

const MOCK_EDGE_DEFS: MockEdgeDef[] = [
  { id: "e1", source: "request:demorpc.sleepy", target: "mutex:global_state", rel: "needs" },
  { id: "e2", source: "mutex:global_state", target: "channel:mpsc.tx", rel: "needs" },
  { id: "e3", source: "channel:mpsc.tx", target: "channel:mpsc.rx", rel: "default" },
  { id: "e4", source: "channel:mpsc.rx", target: "request:demorpc.sleepy", rel: "needs" },
  { id: "e5", source: "request:demorpc.ping", target: "mutex:global_state", rel: "default" },
  { id: "e6", source: "future:store.incoming.recv", target: "request:demorpc.sleepy", rel: "spawned" },
  { id: "e7", source: "request:demorpc.sleepy", target: "connection:initiator", rel: "touches" },
];

function edgeStyle(rel: MockEdgeDef["rel"]) {
  switch (rel) {
    case "needs":
      return { stroke: "light-dark(#d7263d, #ff6b81)", strokeWidth: 2.4 };
    case "spawned":
      return { stroke: "light-dark(#8e7cc3, #b4a7d6)", strokeWidth: 1.2, strokeDasharray: "2 3" };
    case "touches":
      return { stroke: "light-dark(#a1a1a6, #636366)", strokeWidth: 1, strokeDasharray: "4 3" };
    default:
      return { stroke: "light-dark(#c7c7cc, #48484a)", strokeWidth: 1.5 };
  }
}

function edgeMarkerSize(rel: MockEdgeDef["rel"]) {
  return rel === "needs" ? 12 : rel === "spawned" ? 8 : 10;
}

type ElkPoint = { x: number; y: number };
type LayoutResult = { nodes: Node[]; edges: Edge[] };

async function layoutMockGraph(
  nodeDefs: MockNodeDef[],
  edgeDefs: MockEdgeDef[],
  nodeSizes: Map<string, { width: number; height: number }>,
): Promise<LayoutResult> {
  const result = await elk.layout({
    id: "root",
    layoutOptions: elkOptions,
    children: nodeDefs.map((n) => {
      const sz = nodeSizes.get(n.id);
      if (!sz || sz.width === 0 || sz.height === 0) {
        console.warn(`[layoutMockGraph] missing/zero size for node "${n.id}":`, sz);
      }
      return { id: n.id, width: sz?.width || 150, height: sz?.height || 36 };
    }),
    edges: edgeDefs.map((e) => ({
      id: e.id,
      sources: [e.source],
      targets: [e.target],
    })),
  });

  const posMap = new Map(
    (result.children ?? []).map((c) => [c.id, { x: c.x ?? 0, y: c.y ?? 0 }]),
  );

  // Build a map of ELK's edge sections (route points)
  const elkEdgeMap = new Map(
    (result.edges ?? []).map((e) => [e.id, e.sections ?? []]),
  );

  const nodes: Node[] = nodeDefs.map((def) => ({
    id: def.id,
    type: "mockNode",
    position: posMap.get(def.id) ?? { x: 0, y: 0 },
    data: { kind: def.kind, label: def.label, inCycle: def.inCycle ?? false },
  }));

  const edges: Edge[] = edgeDefs.map((def) => {
    const sz = edgeMarkerSize(def.rel);
    const sections = elkEdgeMap.get(def.id) ?? [];
    const points: ElkPoint[] = [];
    for (const section of sections) {
      points.push(section.startPoint);
      if (section.bendPoints) points.push(...section.bendPoints);
      points.push(section.endPoint);
    }
    return {
      id: def.id,
      source: def.source,
      target: def.target,
      type: "elkrouted",
      data: { points },
      style: edgeStyle(def.rel),
      markerEnd: { type: MarkerType.ArrowClosed, width: sz, height: sz },
    };
  });

  return { nodes, edges };
}

// ── Custom node component for mockup ─────────────────────────

const hiddenHandle: React.CSSProperties = { opacity: 0, width: 0, height: 0, minWidth: 0, minHeight: 0, position: "absolute", top: "50%", left: "50%", pointerEvents: "none" };

function MockNodeComponent({ data }: { data: { kind: string; label: string; inCycle: boolean } }) {
  return (
    <>
      <Handle type="target" position={Position.Top} style={hiddenHandle} />
      <Handle type="source" position={Position.Bottom} style={hiddenHandle} />
      <div className={`mockup-node${data.inCycle ? " mockup-node--cycle" : ""}`}>
        <span className="mockup-node-icon">{kindIcon(data.kind, 14)}</span>
        <span className="mockup-node-label">{data.label}</span>
      </div>
    </>
  );
}

/** Edge component that draws through ELK's computed route points. */
function ElkRoutedEdge({ id, data, style, markerEnd }: EdgeProps) {
  const points: ElkPoint[] = (data as any)?.points ?? [];
  if (points.length < 2) return null;

  const [start, ...rest] = points;
  // Build a smooth path through all points
  let d = `M ${start.x} ${start.y}`;
  if (rest.length === 1) {
    // Straight line
    d += ` L ${rest[0].x} ${rest[0].y}`;
  } else {
    // Smooth polyline: use quadratic curves through bend points
    for (let i = 0; i < rest.length - 1; i++) {
      const curr = rest[i];
      const next = rest[i + 1];
      if (i < rest.length - 2) {
        // Mid-bend: curve through this point toward the midpoint to next
        const midX = (curr.x + next.x) / 2;
        const midY = (curr.y + next.y) / 2;
        d += ` Q ${curr.x} ${curr.y}, ${midX} ${midY}`;
      } else {
        // Last segment: curve to final point
        d += ` Q ${curr.x} ${curr.y}, ${next.x} ${next.y}`;
      }
    }
  }

  return (
    <path
      id={id}
      d={d}
      style={style as React.CSSProperties}
      markerEnd={markerEnd as string}
      fill="none"
      className="react-flow__edge-path"
    />
  );
}

const mockNodeTypes = { mockNode: MockNodeComponent };
const mockEdgeTypes = { elkrouted: ElkRoutedEdge };

// ── Mock graph panel ──────────────────────────────────────────

function MockGraphPanel() {
  const [layout, setLayout] = useState<LayoutResult>({ nodes: [], edges: [] });

  useEffect(() => {
    const sizes = measureNodeDefs(MOCK_NODE_DEFS);
    layoutMockGraph(MOCK_NODE_DEFS, MOCK_EDGE_DEFS, sizes).then(setLayout);
  }, []);

  return (
    <div className="mockup-graph-panel">
      <div className="mockup-graph-toolbar">
        <div className="mockup-graph-toolbar-left">
          <Badge tone="crit">1 cycle</Badge>
          <Badge tone="warn">2 suspects</Badge>
          <span className="mockup-graph-stat">7 nodes</span>
          <span className="mockup-graph-stat">7 edges</span>
        </div>
        <div className="mockup-graph-toolbar-right">
          <Checkbox checked={false} onChange={() => {}} label="Resources" />
          <span className="mockup-graph-level-label">Detail</span>
          <Badge tone="neutral">info</Badge>
        </div>
      </div>
      <div className="mockup-graph-flow">
        <ReactFlowProvider>
          <ReactFlow
            nodes={layout.nodes}
            edges={layout.edges}
            nodeTypes={mockNodeTypes}
            edgeTypes={mockEdgeTypes}
            fitView
            fitViewOptions={{ padding: 0.3, maxZoom: 1.2 }}
            proOptions={{ hideAttribution: true }}
            minZoom={0.3}
            maxZoom={3}
            panOnDrag
            nodesDraggable={false}
            nodesConnectable={false}
            elementsSelectable={false}
          >
            <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
            <Controls showInteractive={false} />
          </ReactFlow>
        </ReactFlowProvider>
      </div>
    </div>
  );
}

// ── Mock inspector panel ──────────────────────────────────────

function MockInspectorPanel({
  collapsed,
  onToggleCollapse,
}: {
  collapsed: boolean;
  onToggleCollapse: () => void;
}) {
  if (collapsed) {
    return (
      <button
        className="mockup-inspector mockup-inspector--collapsed"
        onClick={onToggleCollapse}
        title="Expand inspector"
      >
        <CaretLeft size={14} weight="bold" />
        <span className="mockup-inspector-collapsed-label">Inspector</span>
      </button>
    );
  }

  return (
    <div className="mockup-inspector">
      <div className="mockup-inspector-header">
        <MagnifyingGlass size={14} weight="bold" />
        <span>Inspector</span>
        <button className="mockup-inspector-collapse-btn" onClick={onToggleCollapse} title="Collapse inspector">
          <CaretRight size={14} weight="bold" />
        </button>
      </div>
      <div className="mockup-inspector-body">
        <div className="mockup-inspector-node-header">
          <span className="mockup-inspector-node-icon">
            {kindIcon("request", 16)}
          </span>
          <div>
            <div className="mockup-inspector-node-kind">request</div>
            <div className="mockup-inspector-node-label">DemoRpc.sleepy_forever</div>
          </div>
          <ActionButton size="sm">
            <Crosshair size={12} weight="bold" />
            focus
          </ActionButton>
        </div>

        <div className="mockup-inspector-alert mockup-inspector-alert--crit">
          Suspect deadlock signal: <code>needs_cycle</code>
        </div>

        <div className="mockup-inspector-section">
          <KeyValueRow label="ID" icon={<Hash size={12} weight="bold" />}>
            <span className="mockup-inspector-mono">request:demorpc.sleepy</span>
          </KeyValueRow>
          <KeyValueRow label="Process" icon={<Users size={12} weight="bold" />}>
            <NodeChip
              label="example-roam-rpc-stuck-request"
              icon={<ProcessIdenticon name="example-roam-rpc-stuck-request" size={12} />}
            />
          </KeyValueRow>
          <KeyValueRow label="Method" icon={<PaperPlaneTilt size={12} weight="bold" />}>
            <span className="mockup-inspector-mono">DemoRpc.sleepy_forever</span>
          </KeyValueRow>
          <KeyValueRow label="Source">
            <NodeChip
              icon={<FileRs size={12} weight="bold" />}
              label="main.rs:42"
              href="#"
              title="Open in editor"
            />
          </KeyValueRow>
        </div>

        <div className="mockup-inspector-section">
          <KeyValueRow label="Status" icon={<CircleNotch size={12} weight="bold" />}>
            <Badge tone="warn">IN_FLIGHT</Badge>
          </KeyValueRow>
          <KeyValueRow label="Elapsed" icon={<Timer size={12} weight="bold" />}>
            <DurationDisplay ms={1245000} tone="crit" />
          </KeyValueRow>
          <KeyValueRow label="Connection" icon={<LinkSimple size={12} weight="bold" />}>
            <NodeChip
              kind="connection"
              label="initiator->acceptor"
              onClick={() => {}}
            />
          </KeyValueRow>
        </div>

        <div className="mockup-inspector-section">
          <KeyValueRow label="Wait blockers">
            <span className="mockup-inspector-crit">2</span>
          </KeyValueRow>
          <KeyValueRow label="waits on">
            <NodeChip
              kind="mutex"
              label="Mutex<GlobalState>"
              onClick={() => {}}
            />
          </KeyValueRow>
          <KeyValueRow label="waits on">
            <NodeChip
              kind="channel_rx"
              label="mpsc.recv"
              onClick={() => {}}
            />
          </KeyValueRow>
          <KeyValueRow label="Dependents">
            <span className="mockup-inspector-warn">1</span>
          </KeyValueRow>
          <KeyValueRow label="blocking">
            <NodeChip
              kind="request"
              label="DemoRpc.ping"
              onClick={() => {}}
            />
          </KeyValueRow>
        </div>

        <div className="mockup-inspector-section">
          <div className="mockup-inspector-raw-head">
            <span>All attributes (12)</span>
            <ActionButton size="sm">
              <CopySimple size={12} weight="bold" />
            </ActionButton>
            <ActionButton size="sm">Show raw</ActionButton>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Deadlock detector mockup ──────────────────────────────────

function DeadlockDetectorMockup() {
  const [inspectorWidth, setInspectorWidth] = useState(340);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);

  return (
    <div className="mockup-app">
      <div className="mockup-header">
        <Aperture size={16} weight="bold" />
        <span className="mockup-header-title">peeps</span>
        <span className="mockup-header-badge mockup-header-badge--active">
          <CheckCircle size={12} weight="bold" />
          snapshot #4
        </span>
        <span className="mockup-header-badge">
          3/3 responded
        </span>
        <span className="mockup-header-spacer" />
        <ActionButton variant="primary">
          <Camera size={14} weight="bold" />
          Take snapshot
        </ActionButton>
      </div>
      <SplitLayout
        left={<MockGraphPanel />}
        right={
          <MockInspectorPanel
            collapsed={inspectorCollapsed}
            onToggleCollapse={() => setInspectorCollapsed((v) => !v)}
          />
        }
        rightWidth={inspectorWidth}
        onRightWidthChange={setInspectorWidth}
        rightMinWidth={260}
        rightMaxWidth={600}
        rightCollapsed={inspectorCollapsed}
      />
    </div>
  );
}

// ── Lab primitives showcase (existing) ────────────────────────

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

export function LabView() {
  const [textValue, setTextValue] = useState("Hello");
  const [searchValue, setSearchValue] = useState("");
  const [checked, setChecked] = useState(true);
  const [selectValue, setSelectValue] = useState("all");
  const [sliderValue, setSliderValue] = useState(1);
  const [searchOnlyKind, setSearchOnlyKind] = useState<string | null>(null);
  const [selectedSearchId, setSelectedSearchId] = useState<string | null>(null);
  const [segmentedMode, setSegmentedMode] = useState("graph");
  const [segmentedSeverity, setSegmentedSeverity] = useState("all");
  const [tableSortKey, setTableSortKey] = useState("health");
  const [tableSortDir, setTableSortDir] = useState<"asc" | "desc">("desc");
  const [selectedTableRow, setSelectedTableRow] = useState<string | null>(null);
  const [hiddenKinds, setHiddenKinds] = useState<Set<string>>(new Set());
  const [hiddenProcesses, setHiddenProcesses] = useState<Set<string>>(new Set());
  const tones = useMemo<BadgeTone[]>(() => ["neutral", "ok", "warn", "crit"], []);
  const searchDataset = useMemo(() => [
    { id: "future:store.incoming.recv", label: "store.incoming.recv", kind: "future", process: "vx-store" },
    { id: "request:demorpc.sleepy", label: "DemoRpc.sleepy_forever", kind: "request", process: "example-roam-rpc-stuck-request" },
    { id: "request:demorpc.ping", label: "DemoRpc.ping", kind: "request", process: "example-roam-rpc-stuck-request" },
    { id: "channel:mpsc.tx", label: "channel.v1.mpsc.send", kind: "channel", process: "vx-runner" },
    { id: "channel:mpsc.rx", label: "channel.v1.mpsc.recv", kind: "channel", process: "vx-vfsd" },
    { id: "oneshot:recv", label: "channel.v1.oneshot.recv", kind: "oneshot", process: "vx-store" },
    { id: "resource:conn", label: "connection initiator->acceptor", kind: "resource", process: "vxd" },
    { id: "net:read", label: "net.readable.wait", kind: "net", process: "vxd" },
  ], []);

  const processIdenticonNames = useMemo(
    () => [
      "example-roam-rpc-stuck-request",
      "vx-store",
      "vx-runner",
      "vx-vfsd",
      "vxd",
      "peeps-collector",
    ],
    [],
  );

  const filterKindItems = useMemo<FilterMenuItem[]>(() => [
    { id: "connection", label: "Connection", icon: kindIcon("connection", 14), meta: "connection" },
    { id: "mutex", label: "Mutex", icon: kindIcon("mutex", 14), meta: "lock" },
    { id: "request", label: "Request", icon: kindIcon("request", 14), meta: "request" },
    { id: "response", label: "Response", icon: kindIcon("response", 14), meta: "response" },
    { id: "channel_rx", label: "Channel Rx", icon: kindIcon("channel_rx", 14), meta: "rx" },
    { id: "channel_tx", label: "Channel Tx", icon: kindIcon("channel_tx", 14), meta: "tx" },
  ], []);

  const filterProcessItems = useMemo<FilterMenuItem[]>(() => [
    { id: "vx-store", label: "vx-store" },
    { id: "vx-runner", label: "vx-runner" },
    { id: "vx-vfsd", label: "vx-vfsd" },
    { id: "vxd", label: "vxd" },
    { id: "peeps-collector", label: "peeps-collector" },
  ], []);

  const toggleKind = useCallback((id: string) => {
    setHiddenKinds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const soloKind = useCallback((id: string) => {
    setHiddenKinds((prev) => {
      const othersAllHidden = filterKindItems.every((item) => item.id === id || prev.has(item.id));
      if (othersAllHidden && !prev.has(id)) return new Set();
      return new Set(filterKindItems.filter((item) => item.id !== id).map((item) => item.id));
    });
  }, [filterKindItems]);

  const toggleProcess = useCallback((id: string) => {
    setHiddenProcesses((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const soloProcess = useCallback((id: string) => {
    setHiddenProcesses((prev) => {
      const othersAllHidden = filterProcessItems.every((item) => item.id === id || prev.has(item.id));
      if (othersAllHidden && !prev.has(id)) return new Set();
      return new Set(filterProcessItems.filter((item) => item.id !== id).map((item) => item.id));
    });
  }, [filterProcessItems]);

  const tableRows = useMemo<DemoConnectionRow[]>(() => [
    {
      id: "conn-01",
      healthLabel: "OK",
      healthTone: "ok",
      connectionKind: "connection",
      connectionLabel: "example-roam-rpc-stuck-request: initiator\u2192acceptor",
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
      healthLabel: "WARN",
      healthTone: "warn",
      connectionKind: "channel_tx",
      connectionLabel: "vx-store \u00b7 channel.v1.mpsc.send",
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
      healthLabel: "CRIT",
      healthTone: "crit",
      connectionKind: "request",
      connectionLabel: "example-roam-rpc-stuck-request \u00b7 DemoRpc.sleepy_forever",
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
      healthLabel: "WARN",
      healthTone: "warn",
      connectionKind: "connection",
      connectionLabel: "vxd \u00b7 connection: initiator<->acceptor",
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
      healthLabel: "OK",
      healthTone: "ok",
      connectionKind: "request",
      connectionLabel: "vx-vfsd \u00b7 net.readable.wait",
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
  ], []);

  const tableColumns = useMemo<readonly Column<DemoConnectionRow>[]>(() => [
    { key: "health", label: "Health", sortable: true, width: "80px", render: (row) => <Badge tone={row.healthTone}>{row.healthLabel}</Badge> },
    { key: "connection", label: "Connection", sortable: true, width: "1fr", render: (row) => (
      <NodeChip
        kind={row.connectionKind}
        label={row.connectionLabel}
        onClick={() => console.log(`select connection ${row.id}`)}
        onContextMenu={(event) => {
          event.preventDefault();
          console.log(`connection context menu ${row.id}`);
        }}
      />
    ) },
    { key: "pending", label: "Pending Req", sortable: true, width: "80px", render: (row) => row.pending },
    { key: "lastRecv", label: "Last Recv", sortable: true, width: "100px", render: (row) => (
      <RelativeTimestamp
        basis={row.lastRecvBasis}
        basisLabel={row.lastRecvBasisLabel}
        basisTime={row.lastRecvBasisTime}
        eventTime={row.lastRecvEventTime}
        tone={row.lastRecvTone}
      />
    ) },
    { key: "lastSent", label: "Last Sent", sortable: true, width: "100px", render: (row) => {
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
    } },
  ], []);

  const tableSortedRows = useMemo(() => {
    const healthOrder: Record<string, number> = {
      healthy: 1,
      warning: 2,
      critical: 3,
      ok: 1,
      warn: 2,
      crit: 3,
    };
    const by = tableSortKey === "connection" ? (row: DemoConnectionRow) => row.connectionLabel
      : tableSortKey === "pending" ? (row: DemoConnectionRow) => row.pending
      : tableSortKey === "lastRecv" ? (row: DemoConnectionRow) => Date.parse(row.lastRecvEventTime)
      : tableSortKey === "lastSent" ? (row: DemoConnectionRow) => row.lastSentEventTime === null ? Number.NEGATIVE_INFINITY : Date.parse(row.lastSentEventTime)
      : (row: DemoConnectionRow) => healthOrder[row.healthTone];
    const direction = tableSortDir === "asc" ? 1 : -1;

    return [...tableRows].sort((a, b) => {
      const aValue = by(a);
      const bValue = by(b);
      if (typeof aValue === "number" && typeof bValue === "number") return (aValue - bValue) * direction;
      return String(aValue).localeCompare(String(bValue), undefined, { numeric: true }) * direction;
    });
  }, [tableRows, tableSortDir, tableSortKey]);

  function onTableSort(key: string) {
    if (!tableColumns.some((column) => column.key === key && column.sortable)) return;
    if (tableSortKey === key) {
      setTableSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
      return;
    }
    setTableSortKey(key);
    setTableSortDir("desc");
  }

  const searchResults = useMemo(() => {
    const needle = searchValue.trim().toLowerCase();
    return searchDataset
      .filter((item) => !searchOnlyKind || item.kind === searchOnlyKind)
      .filter((item) => {
        if (needle.length === 0) return true;
        return (
          item.label.toLowerCase().includes(needle)
          || item.id.toLowerCase().includes(needle)
          || item.process.toLowerCase().includes(needle)
          || item.kind.toLowerCase().includes(needle)
        );
      })
      .slice(0, 6);
  }, [searchDataset, searchOnlyKind, searchValue]);
  const showSearchResults = searchValue.trim().length > 0 || searchOnlyKind !== null;
  const selectOptions = useMemo(() => [
    { value: "all", label: "All" },
    { value: "warn", label: "Warning+" },
    { value: "crit", label: "Critical" },
  ], []);
  const nodeTypeMenu = useMemo(() => [
    { id: "show-kind", label: "Show only this kind" },
    { id: "hide-kind", label: "Hide this kind" },
    { id: "reset", label: "Reset filters", danger: true },
  ], []);
  const processMenu = useMemo(() => [
    { id: "open-resources", label: "Open in Resources" },
    { id: "show-process", label: "Show only this process" },
    { id: "hide-process", label: "Hide this process" },
  ], []);

  return (
    <Panel variant="lab">
      <PanelHeader title="Lab" hint="Primitives and tone language" />
      <div className="lab-body">
        <Section title="Deadlock Detector" subtitle="Full app layout mockup with resizable inspector" wide>
          <DeadlockDetectorMockup />
        </Section>

        <Section title="UI font — Manrope" subtitle="UI font in the sizes we actually use" wide>
          <div className="ui-typo-card">
            <div className="ui-typo-sample ui-typo-ui ui-typo-ui--xl">Take a snapshot</div>
            <div className="ui-typo-sample ui-typo-ui ui-typo-ui--md">Inspector, Graph, Timeline, Resources</div>
            <div className="ui-typo-sample ui-typo-ui ui-typo-ui--sm ui-typo-muted">
              Buttons, labels, helper text, and navigation should mostly live here.
            </div>
            <div className="ui-typo-weights">
              <span className="ui-typo-pill ui-typo-ui ui-typo-w-400">400</span>
              <span className="ui-typo-pill ui-typo-ui ui-typo-w-700">700</span>
            </div>
          </div>
        </Section>

        <Section title="Mono font — Jetbrains Mono" subtitle="Mono font in the sizes we actually use" wide>
          <div className="ui-typo-card">
            <div className="ui-typo-sample ui-typo-mono ui-typo-mono--xl">request:01KHNGCY&hellip;</div>
            <div className="ui-typo-sample ui-typo-mono ui-typo-mono--md">connection: initiator-&gt;acceptor</div>
            <div className="ui-typo-sample ui-typo-mono ui-typo-mono--sm ui-typo-muted">
              IDs, paths, tokens, and anything users copy/paste.
            </div>
            <div className="ui-typo-weights">
              <span className="ui-typo-pill ui-typo-mono ui-typo-w-400">400</span>
              <span className="ui-typo-pill ui-typo-mono ui-typo-w-700">700</span>
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
              <ActionButton
                size="sm"
                aria-label="Copy"
              >
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
            placeholder="Type\u2026"
            aria-label="Text input"
          />
        </Section>

        <Section title="Search" subtitle="Autocomplete with results, filters, and selection" wide>
          <SearchInput
            value={searchValue}
            onChange={setSearchValue}
            placeholder="Search nodes\u2026"
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
                <span className="ui-search-results-hint">click to select &middot; alt+click to filter only this kind</span>
              </>
            }
            filterBadge={searchOnlyKind ? <Badge tone="neutral">{`kind:${searchOnlyKind}`}</Badge> : undefined}
            onClearFilter={() => setSearchOnlyKind(null)}
            onSelect={(id) => setSelectedSearchId(id)}
            onAltSelect={(id) => {
              const item = searchResults.find((entry) => entry.id === id);
              if (!item) return;
              setSearchOnlyKind((prev) => (prev === item.kind ? null : item.kind));
            }}
          />
        </Section>

        <Section title="Controls" subtitle="Checkbox, select">
          <Row className="ui-row--controls">
            <Checkbox
              checked={checked}
              onChange={setChecked}
              label="Show resources"
            />
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
          <Row>
            <Menu
              label={
                <span className="ui-menu-label">
                  <span>Node types</span>
                  <CaretDown size={12} weight="bold" />
                </span>
              }
              items={nodeTypeMenu}
            />
            <Menu
              label={<span className="ui-menu-label">Process <CaretDown size={12} weight="bold" /></span>}
              items={processMenu}
            />
          </Row>
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
              size="sm"
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
            <KeyValueRow
              label="Source"
              labelWidth={80}
            >
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
                label="initiator\u2192acceptor"
                onClick={() => console.log("inspect initiator\u2192acceptor")}
                onContextMenu={(event) => {
                  event.preventDefault();
                  console.log("open context for initiator\u2192acceptor");
                }}
              />
            </KeyValueRow>
            <KeyValueRow
              label="Opened"
              labelWidth={80}
              icon={<Timer size={12} weight="bold" />}
            >
              <RelativeTimestamp
                basis="P"
                basisLabel="process started"
                basisTime="2026-02-17T10:06:00.000Z"
                eventTime="2026-02-17T10:06:06.000Z"
              />
            </KeyValueRow>
            <KeyValueRow
              label="Closed"
              labelWidth={80}
              icon={<Timer size={12} weight="bold" />}
            >
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
              <RelativeTimestamp basis="P" basisLabel="6 seconds after process start" basisTime="2026-02-17T10:00:00.000Z" eventTime="2026-02-17T10:00:06.000Z" />
              <span className="ui-relative-timestamp-caption">process start</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="P" basisLabel="2 minutes 30 seconds after process start" basisTime="2026-02-17T10:00:00.000Z" eventTime="2026-02-17T10:02:30.000Z" />
              <span className="ui-relative-timestamp-caption">process start</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="N" basisLabel="node just created" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:00:30.000Z" tone="ok" />
              <span className="ui-relative-timestamp-caption">node created</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="N" basisLabel="1m5s after node open" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:01:35.000Z" tone="warn" />
              <span className="ui-relative-timestamp-caption">node-relative</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="N" basisLabel="stuck for 20 minutes" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:21:15.000Z" tone="crit" />
              <span className="ui-relative-timestamp-caption">stuck 20m</span>
            </div>
            <div className="ui-relative-timestamp-item">
              <RelativeTimestamp basis="N" basisLabel="sub-second timing check" basisTime="2026-02-17T10:00:30.000Z" eventTime="2026-02-17T10:00:30.145Z" />
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
              label="initiator\u2192acceptor:acceptor\u2194\u2192initiator"
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

        <Section title="Process Identicons" subtitle="Name-derived 5x5 process avatars">
          <div className="ui-identicon-list">
            {processIdenticonNames.map((name) => (
              <span key={name} className="ui-identicon-cell">
                <ProcessIdenticon name={name} size={20} />
                <span>{name}</span>
              </span>
            ))}
          </div>
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
