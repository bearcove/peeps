import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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

type MockEntityDef = {
  id: string;
  name: string;
  kind: string; // for icon lookup — "request", "mutex", "channel_tx", etc.
  bodyKind: string; // EntityBody variant name
  body: Record<string, any>; // body-specific fields for inspector
  source: string;
  birthAgeMs: number; // age in ms for display
  meta: Record<string, MetaValue>;
  inCycle?: boolean;
};

type MockEdgeDef = {
  id: string;
  source: string;
  target: string;
  kind: "needs" | "polls" | "closed_by" | "channel_link" | "rpc_link";
};

const MOCK_ENTITIES: MockEntityDef[] = [
  {
    id: "req_sleepy", name: "DemoRpc.sleepy_forever", kind: "request", bodyKind: "Request",
    body: { method: "DemoRpc.sleepy_forever", args_preview: "(no args)" },
    source: "src/rpc/demo.rs:42", birthAgeMs: 1245000,
    meta: { level: "info", rpc_service: "DemoRpc", transport: "roam-tcp" },
  },
  {
    id: "resp_sleepy", name: "DemoRpc.sleepy_forever", kind: "response", bodyKind: "Response",
    body: { method: "DemoRpc.sleepy_forever", status: "error" },
    source: "src/rpc/demo.rs:45", birthAgeMs: 1244800, inCycle: true,
    meta: { level: "info", status_detail: "deadline exceeded" },
  },
  {
    id: "req_ping", name: "DemoRpc.ping", kind: "request", bodyKind: "Request",
    body: { method: "DemoRpc.ping", args_preview: "{ ttl: 30 }" },
    source: "src/rpc/demo.rs:18", birthAgeMs: 820000,
    meta: { level: "info", rpc_service: "DemoRpc", transport: "roam-tcp" },
  },
  {
    id: "resp_ping", name: "DemoRpc.ping", kind: "response", bodyKind: "Response",
    body: { method: "DemoRpc.ping", status: "ok" },
    source: "src/rpc/demo.rs:20", birthAgeMs: 819500,
    meta: { level: "info" },
  },
  {
    id: "lock_state", name: "Mutex<GlobalState>", kind: "mutex", bodyKind: "Lock",
    body: { kind: "mutex" },
    source: "src/state.rs:12", birthAgeMs: 3600000, inCycle: true,
    meta: { level: "debug" },
  },
  {
    id: "ch_tx", name: "mpsc.send", kind: "channel_tx", bodyKind: "ChannelTx",
    body: { lifecycle: "open", details: { kind: "mpsc", capacity: 128, queue_len: 0 } },
    source: "src/dispatch.rs:67", birthAgeMs: 3590000,
    meta: { level: "debug" },
  },
  {
    id: "ch_rx", name: "mpsc.recv", kind: "channel_rx", bodyKind: "ChannelRx",
    body: { lifecycle: "open", details: { kind: "mpsc", capacity: 128, queue_len: 0 } },
    source: "src/dispatch.rs:68", birthAgeMs: 3590000, inCycle: true,
    meta: { level: "debug" },
  },
  {
    id: "future_store", name: "store.incoming.recv", kind: "future", bodyKind: "Future",
    body: {},
    source: "src/store.rs:104", birthAgeMs: 2100000,
    meta: { level: "trace", poll_count: 847 },
  },
];

// Edge semantics:
// - "needs": causal dependency (A is blocked waiting for B). Only needs edges form deadlock cycles.
// - "polls": non-blocking observation (A polls B without blocking).
// - "rpc_link": structural request↔response pairing.
// - "channel_link": structural channel tx↔rx pairing.
// - "closed_by": closure/cancellation cause.
const MOCK_EDGES: MockEdgeDef[] = [
  // Deadlock cycle: resp_sleepy →needs→ lock_state →needs→ ch_rx →needs→ resp_sleepy
  // The response handler needs the lock; the lock holder is blocked waiting for channel data;
  // the channel receiver is blocked waiting for the response to complete.
  { id: "e1", source: "resp_sleepy", target: "lock_state", kind: "needs" },
  { id: "e2", source: "lock_state", target: "ch_rx", kind: "needs" },
  { id: "e3", source: "ch_rx", target: "resp_sleepy", kind: "needs" },
  // Structural pairings
  { id: "e4", source: "ch_tx", target: "ch_rx", kind: "channel_link" },
  { id: "e5", source: "req_sleepy", target: "resp_sleepy", kind: "rpc_link" },
  { id: "e6", source: "req_ping", target: "resp_ping", kind: "rpc_link" },
  // Non-blocking observations
  { id: "e7", source: "req_ping", target: "lock_state", kind: "polls" },
  { id: "e8", source: "future_store", target: "ch_rx", kind: "polls" },
];

/** Validate edge definitions against entity kinds. */
function validateEdges(entities: MockEntityDef[], edges: MockEdgeDef[]) {
  const entityMap = new Map(entities.map((e) => [e.id, e]));
  for (const edge of edges) {
    const src = entityMap.get(edge.source);
    const dst = entityMap.get(edge.target);
    if (!src) console.error(`[validateEdges] unknown source "${edge.source}" in edge "${edge.id}"`);
    if (!dst) console.error(`[validateEdges] unknown target "${edge.target}" in edge "${edge.id}"`);
    if (!src || !dst) continue;

    switch (edge.kind) {
      case "rpc_link":
        if (src.bodyKind !== "Request" || dst.bodyKind !== "Response")
          console.error(`[validateEdges] rpc_link "${edge.id}" must be Request→Response, got ${src.bodyKind}→${dst.bodyKind}`);
        break;
      case "channel_link":
        if (src.bodyKind !== "ChannelTx" || dst.bodyKind !== "ChannelRx")
          console.error(`[validateEdges] channel_link "${edge.id}" must be ChannelTx→ChannelRx, got ${src.bodyKind}→${dst.bodyKind}`);
        break;
    }
  }
}

// Run validation at module load time so bad edges are caught immediately.
validateEdges(MOCK_ENTITIES, MOCK_EDGES);

/** Measure actual rendered node dimensions by briefly inserting into the DOM. */
function measureNodeDefs(defs: MockEntityDef[]): Map<string, { width: number; height: number }> {
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
    label.textContent = def.name;
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

function edgeStyle(kind: MockEdgeDef["kind"]) {
  switch (kind) {
    case "needs":
      return { stroke: "light-dark(#d7263d, #ff6b81)", strokeWidth: 2.4 };
    case "polls":
      return { stroke: "light-dark(#8e7cc3, #b4a7d6)", strokeWidth: 1.2, strokeDasharray: "2 3" };
    case "closed_by":
      return { stroke: "light-dark(#e08614, #f0a840)", strokeWidth: 1.5 };
    case "channel_link":
      return { stroke: "light-dark(#888, #666)", strokeWidth: 1, strokeDasharray: "6 3" };
    case "rpc_link":
      return { stroke: "light-dark(#888, #666)", strokeWidth: 1, strokeDasharray: "6 3" };
  }
}

function edgeTooltip(kind: MockEdgeDef["kind"], sourceName: string, targetName: string): string {
  switch (kind) {
    case "needs": return `${sourceName} is blocked waiting for ${targetName}`;
    case "polls": return `${sourceName} polls ${targetName} (non-blocking)`;
    case "closed_by": return `${sourceName} was closed by ${targetName}`;
    case "channel_link": return `Channel endpoint: ${sourceName} → ${targetName}`;
    case "rpc_link": return `RPC pair: ${sourceName} → ${targetName}`;
  }
}

function edgeMarkerSize(kind: MockEdgeDef["kind"]): number {
  return kind === "needs" ? 12 : 8;
}


type ElkPoint = { x: number; y: number };
type LayoutResult = { nodes: Node[]; edges: Edge[] };

async function layoutMockGraph(
  entityDefs: MockEntityDef[],
  edgeDefs: MockEdgeDef[],
  nodeSizes: Map<string, { width: number; height: number }>,
): Promise<LayoutResult> {
  const result = await elk.layout({
    id: "root",
    layoutOptions: elkOptions,
    children: entityDefs.map((n) => {
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
  // ELK returns sections on edges but the type defs don't include them
  const elkEdgeMap = new Map(
    (result.edges ?? []).map((e: any) => [e.id, e.sections ?? []]),
  );

  const nodes: Node[] = entityDefs.map((def) => ({
    id: def.id,
    type: "mockNode",
    position: posMap.get(def.id) ?? { x: 0, y: 0 },
    data: { kind: def.kind, label: def.name, inCycle: def.inCycle ?? false, selected: false },
  }));

  const entityNameMap = new Map(entityDefs.map((e) => [e.id, e.name]));
  const edges: Edge[] = edgeDefs.map((def) => {
    const sz = edgeMarkerSize(def.kind);
    const sections = elkEdgeMap.get(def.id) ?? [];
    const points: ElkPoint[] = [];
    for (const section of sections) {
      points.push(section.startPoint);
      if (section.bendPoints) points.push(...section.bendPoints);
      points.push(section.endPoint);
    }
    const srcName = entityNameMap.get(def.source) ?? def.source;
    const dstName = entityNameMap.get(def.target) ?? def.target;
    return {
      id: def.id,
      source: def.source,
      target: def.target,
      type: "elkrouted",
      data: { points, tooltip: edgeTooltip(def.kind, srcName, dstName) },
      style: edgeStyle(def.kind),
      markerEnd: { type: MarkerType.ArrowClosed, width: sz, height: sz },
    };
  });

  return { nodes, edges };
}

// ── Custom node component for mockup ─────────────────────────

const hiddenHandle: React.CSSProperties = { opacity: 0, width: 0, height: 0, minWidth: 0, minHeight: 0, position: "absolute", top: "50%", left: "50%", pointerEvents: "none" };

function MockNodeComponent({ data }: { data: { kind: string; label: string; inCycle: boolean; selected: boolean } }) {
  return (
    <>
      <Handle type="target" position={Position.Top} style={hiddenHandle} />
      <Handle type="source" position={Position.Bottom} style={hiddenHandle} />
      <div className={`mockup-node${data.inCycle ? " mockup-node--cycle" : ""}${data.selected ? " mockup-node--selected" : ""}`}>
        <span className="mockup-node-icon">{kindIcon(data.kind, 14)}</span>
        <span className="mockup-node-label">{data.label}</span>
      </div>
    </>
  );
}

/** Edge component that draws through ELK's computed route points. */
function ElkRoutedEdge({ id, data, style, markerEnd }: EdgeProps) {
  const edgeData = data as { points?: ElkPoint[]; tooltip?: string } | undefined;
  const points = edgeData?.points ?? [];
  if (points.length < 2) return null;

  const [start, ...rest] = points;
  let d = `M ${start.x} ${start.y}`;
  if (rest.length === 1) {
    d += ` L ${rest[0].x} ${rest[0].y}`;
  } else {
    for (let i = 0; i < rest.length - 1; i++) {
      const curr = rest[i];
      const next = rest[i + 1];
      if (i < rest.length - 2) {
        const midX = (curr.x + next.x) / 2;
        const midY = (curr.y + next.y) / 2;
        d += ` Q ${curr.x} ${curr.y}, ${midX} ${midY}`;
      } else {
        d += ` Q ${curr.x} ${curr.y}, ${next.x} ${next.y}`;
      }
    }
  }

  return (
    <g>
      {/* Invisible wider path for easier hover targeting */}
      <path d={d} fill="none" stroke="transparent" strokeWidth={12} />
      <path
        id={id}
        d={d}
        style={style as React.CSSProperties}
        markerEnd={markerEnd as string}
        fill="none"
        className="react-flow__edge-path"
      />
    </g>
  );
}

const mockNodeTypes = { mockNode: MockNodeComponent };
const mockEdgeTypes = { elkrouted: ElkRoutedEdge };

// ── Mock graph panel ──────────────────────────────────────────

type EdgeTooltipState = { text: string; x: number; y: number } | null;

function MockGraphPanel({ selectedEntityId, onSelectEntity }: { selectedEntityId?: string; onSelectEntity: (id: string) => void }) {
  const [layout, setLayout] = useState<LayoutResult>({ nodes: [], edges: [] });
  const [edgeTooltip, setEdgeTooltip] = useState<EdgeTooltipState>(null);
  const flowContainerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const sizes = measureNodeDefs(MOCK_ENTITIES);
    layoutMockGraph(MOCK_ENTITIES, MOCK_EDGES, sizes).then(setLayout);
  }, []);

  const nodesWithSelection = useMemo(() =>
    layout.nodes.map((n) => ({
      ...n,
      data: { ...n.data, selected: n.id === selectedEntityId },
    })),
    [layout.nodes, selectedEntityId],
  );

  const getRelativePos = useCallback((event: React.MouseEvent) => {
    const rect = flowContainerRef.current?.getBoundingClientRect();
    if (!rect) return null;
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  }, []);

  const onEdgeMouseEnter = useCallback((event: React.MouseEvent, edge: Edge) => {
    const text = (edge.data as any)?.tooltip;
    if (!text) return;
    const pos = getRelativePos(event);
    if (!pos) return;
    setEdgeTooltip({ text, ...pos });
  }, [getRelativePos]);

  const onEdgeMouseMove = useCallback((event: React.MouseEvent) => {
    setEdgeTooltip((prev) => {
      if (!prev) return null;
      const rect = flowContainerRef.current?.getBoundingClientRect();
      if (!rect) return prev;
      return { ...prev, x: event.clientX - rect.left, y: event.clientY - rect.top };
    });
  }, []);

  const onEdgeMouseLeave = useCallback(() => setEdgeTooltip(null), []);

  return (
    <div className="mockup-graph-panel">
      <div className="mockup-graph-toolbar">
        <div className="mockup-graph-toolbar-left">
          <span className="mockup-graph-stat">{MOCK_ENTITIES.length} entities</span>
          <span className="mockup-graph-stat">{MOCK_EDGES.length} edges</span>
        </div>
      </div>
      <div className="mockup-graph-flow" ref={flowContainerRef}>
        <ReactFlowProvider>
          <ReactFlow
            nodes={nodesWithSelection}
            edges={layout.edges}
            nodeTypes={mockNodeTypes}
            edgeTypes={mockEdgeTypes}
            onNodeClick={(_event, node) => onSelectEntity(node.id)}
            onEdgeMouseEnter={onEdgeMouseEnter}
            onEdgeMouseMove={onEdgeMouseMove}
            onEdgeMouseLeave={onEdgeMouseLeave}
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
        {edgeTooltip && (
          <div
            className="mockup-edge-tooltip"
            style={{ left: edgeTooltip.x, top: edgeTooltip.y }}
          >
            {edgeTooltip.text}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Inspector panel and body sections ─────────────────────────

function EntityBodySection({ entity }: { entity: MockEntityDef }) {
  const { bodyKind, body } = entity;
  switch (bodyKind) {
    case "Request":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Args">
            <span className={`mockup-inspector-mono${body.args_preview === "(no args)" ? " mockup-inspector-muted" : ""}`}>
              {body.args_preview}
            </span>
          </KeyValueRow>
        </div>
      );
    case "Response":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Method" icon={<PaperPlaneTilt size={12} weight="bold" />}>
            <span className="mockup-inspector-mono">{body.method}</span>
          </KeyValueRow>
          <KeyValueRow label="Status" icon={<CircleNotch size={12} weight="bold" />}>
            <Badge tone={body.status === "ok" ? "ok" : body.status === "error" ? "crit" : "warn"}>
              {body.status}
            </Badge>
          </KeyValueRow>
        </div>
      );
    case "Lock":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Lock kind">
            <span className="mockup-inspector-mono">{body.kind}</span>
          </KeyValueRow>
        </div>
      );
    case "ChannelTx":
    case "ChannelRx":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Lifecycle" icon={<CircleNotch size={12} weight="bold" />}>
            <Badge tone={body.lifecycle === "open" ? "ok" : "neutral"}>
              {body.lifecycle}
            </Badge>
          </KeyValueRow>
          <KeyValueRow label="Channel kind">
            <span className="mockup-inspector-mono">{body.details.kind}</span>
          </KeyValueRow>
          {body.details.capacity != null && (
            <KeyValueRow label="Capacity">
              <span className="mockup-inspector-mono">{body.details.capacity}</span>
            </KeyValueRow>
          )}
          <KeyValueRow label="Queue length">
            <span className="mockup-inspector-mono">{body.details.queue_len}</span>
          </KeyValueRow>
        </div>
      );
    case "Future":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Body">
            <span className="mockup-inspector-mono mockup-inspector-muted">Future (no body fields)</span>
          </KeyValueRow>
        </div>
      );
    default:
      return null;
  }
}

function EntityInspectorContent({ entity }: { entity: MockEntityDef }) {
  return (
    <>
      <div className="mockup-inspector-node-header">
        <span className="mockup-inspector-node-icon">{kindIcon(entity.kind, 16)}</span>
        <div className="mockup-inspector-node-header-text">
          <div className="mockup-inspector-node-kind">{entity.bodyKind}</div>
          <div className="mockup-inspector-node-label">{entity.name}</div>
        </div>
        <ActionButton>
          <Crosshair size={14} weight="bold" />
          Focus
        </ActionButton>
      </div>

      {/* Fixed-height alert slot — always present to prevent layout shift */}
      <div className="mockup-inspector-alert-slot">
        {entity.inCycle && (
          <div className="mockup-inspector-alert mockup-inspector-alert--crit">
            Part of <code>needs</code> cycle — possible deadlock
          </div>
        )}
      </div>

      <div className="mockup-inspector-section">
        <KeyValueRow label="Source" icon={<FileRs size={12} weight="bold" />}>
          <a className="mockup-inspector-source-link" href="#" title="Open in editor">
            {entity.source}
          </a>
        </KeyValueRow>
        <KeyValueRow label="Age" icon={<Timer size={12} weight="bold" />}>
          <DurationDisplay ms={entity.birthAgeMs} tone={entity.birthAgeMs > 600000 ? "crit" : entity.birthAgeMs > 60000 ? "warn" : undefined} />
        </KeyValueRow>
      </div>

      <EntityBodySection entity={entity} />
      <MockMetaSection meta={entity.meta} />
    </>
  );
}

function MockInspectorPanel({
  collapsed,
  onToggleCollapse,
  entity,
}: {
  collapsed: boolean;
  onToggleCollapse: () => void;
  entity: MockEntityDef | undefined;
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
        {entity ? <EntityInspectorContent entity={entity} /> : (
          <div className="mockup-inspector-empty">Select an entity in the graph</div>
        )}
      </div>
    </div>
  );
}

type MetaValue = string | number | boolean | null | MetaValue[] | { [key: string]: MetaValue };

function MetaTreeNode({ name, value, depth = 0 }: { name: string; value: MetaValue; depth?: number }) {
  const [expanded, setExpanded] = useState(depth < 1);
  const isObject = value !== null && typeof value === "object" && !Array.isArray(value);
  const isArray = Array.isArray(value);
  const isExpandable = isObject || isArray;

  if (!isExpandable) {
    return (
      <div className="mockup-meta-leaf" style={{ paddingLeft: depth * 14 }}>
        <span className="mockup-meta-key">{name}</span>
        <span className={`mockup-meta-value mockup-meta-value--${typeof value}`}>
          {value === null ? "null" : typeof value === "string" ? `"${value}"` : String(value)}
        </span>
      </div>
    );
  }

  const entries = isArray
    ? (value as MetaValue[]).map((v, i) => [String(i), v] as const)
    : Object.entries(value as Record<string, MetaValue>);

  return (
    <div className="mockup-meta-branch">
      <button
        className="mockup-meta-toggle"
        style={{ paddingLeft: depth * 14 }}
        onClick={() => setExpanded((v) => !v)}
      >
        <CaretDown
          size={10}
          weight="bold"
          style={{ transform: expanded ? undefined : "rotate(-90deg)", transition: "transform 0.15s" }}
        />
        <span className="mockup-meta-key">{name}</span>
        <span className="mockup-meta-hint">
          {isArray ? `[${entries.length}]` : `{${entries.length}}`}
        </span>
      </button>
      {expanded && entries.map(([k, v]) => (
        <MetaTreeNode key={k} name={k} value={v} depth={depth + 1} />
      ))}
    </div>
  );
}

function MockMetaSection({ meta }: { meta: Record<string, MetaValue> }) {
  return (
    <div className="mockup-inspector-section">
      <div className="mockup-inspector-raw-head">
        <span>Metadata</span>
        <ActionButton size="sm">
          <CopySimple size={12} weight="bold" />
        </ActionButton>
      </div>
      <div className="mockup-meta-tree">
        {Object.entries(meta).map(([k, v]) => (
          <MetaTreeNode key={k} name={k} value={v} />
        ))}
      </div>
    </div>
  );
}

// ── Deadlock detector mockup ──────────────────────────────────

function DeadlockDetectorMockup() {
  const [inspectorWidth, setInspectorWidth] = useState(340);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
  const [selectedEntityId, setSelectedEntityId] = useState<string | undefined>("resp_sleepy");
  const selectedEntity = MOCK_ENTITIES.find((e) => e.id === selectedEntityId);

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
        left={
          <MockGraphPanel
            selectedEntityId={selectedEntityId}
            onSelectEntity={setSelectedEntityId}
          />
        }
        right={
          <MockInspectorPanel
            collapsed={inspectorCollapsed}
            onToggleCollapse={() => setInspectorCollapsed((v) => !v)}
            entity={selectedEntity}
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
