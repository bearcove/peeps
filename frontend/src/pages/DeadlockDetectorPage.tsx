import React, { useEffect, useMemo, useState } from "react";
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
  CaretDown,
  CopySimple,
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
import { SplitLayout } from "../ui/layout/SplitLayout";
import { Badge } from "../ui/primitives/Badge";
import { KeyValueRow } from "../ui/primitives/KeyValueRow";
import { DurationDisplay } from "../ui/primitives/DurationDisplay";
import { ActionButton } from "../ui/primitives/ActionButton";
import { kindIcon } from "../nodeKindSpec";

// ── ELK layout engine ────────────────────────────────────────

const elk = new ELK({ workerUrl: elkWorkerUrl });

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "24",
  "elk.layered.spacing.nodeNodeBetweenLayers": "48",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
  "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
};

// ── Mock data ─────────────────────────────────────────────────

type MockEntityDef = {
  id: string;
  name: string;
  kind: string;
  bodyKind: string;
  body: Record<string, any>;
  source: string;
  birthAgeMs: number;
  meta: Record<string, MetaValue>;
  inCycle?: boolean;
  status: { label: string; tone: "ok" | "warn" | "crit" | "neutral" };
  stat?: string;
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
    status: { label: "in_flight", tone: "warn" },
  },
  {
    id: "resp_sleepy", name: "DemoRpc.sleepy_forever", kind: "response", bodyKind: "Response",
    body: { method: "DemoRpc.sleepy_forever", status: "error" },
    source: "src/rpc/demo.rs:45", birthAgeMs: 1244800, inCycle: true,
    meta: { level: "info", status_detail: "deadline exceeded" },
    status: { label: "error", tone: "crit" },
  },
  {
    id: "req_ping", name: "DemoRpc.ping", kind: "request", bodyKind: "Request",
    body: { method: "DemoRpc.ping", args_preview: "{ ttl: 30 }" },
    source: "src/rpc/demo.rs:18", birthAgeMs: 820000,
    meta: { level: "info", rpc_service: "DemoRpc", transport: "roam-tcp" },
    status: { label: "in_flight", tone: "warn" },
  },
  {
    id: "resp_ping", name: "DemoRpc.ping", kind: "response", bodyKind: "Response",
    body: { method: "DemoRpc.ping", status: "ok" },
    source: "src/rpc/demo.rs:20", birthAgeMs: 819500,
    meta: { level: "info" },
    status: { label: "ok", tone: "ok" },
  },
  {
    id: "lock_state", name: "Mutex<GlobalState>", kind: "mutex", bodyKind: "Lock",
    body: { kind: "mutex" },
    source: "src/state.rs:12", birthAgeMs: 3600000, inCycle: true,
    meta: { level: "debug" },
    status: { label: "held", tone: "crit" }, stat: "1 waiter",
  },
  {
    id: "ch_tx", name: "mpsc.send", kind: "channel_tx", bodyKind: "ChannelTx",
    body: { lifecycle: "open", details: { kind: "mpsc", capacity: 128, queue_len: 0 } },
    source: "src/dispatch.rs:67", birthAgeMs: 3590000,
    meta: { level: "debug" },
    status: { label: "open", tone: "ok" }, stat: "0/128",
  },
  {
    id: "ch_rx", name: "mpsc.recv", kind: "channel_rx", bodyKind: "ChannelRx",
    body: { lifecycle: "open", details: { kind: "mpsc", capacity: 128, queue_len: 0 } },
    source: "src/dispatch.rs:68", birthAgeMs: 3590000, inCycle: true,
    meta: { level: "debug" },
    status: { label: "blocked", tone: "crit" }, stat: "0/128",
  },
  {
    id: "future_store", name: "store.incoming.recv", kind: "future", bodyKind: "Future",
    body: {},
    source: "src/store.rs:104", birthAgeMs: 2100000,
    meta: { level: "trace", poll_count: 847 },
    status: { label: "polling", tone: "neutral" }, stat: "847 polls",
  },
  {
    id: "sem_conns", name: "conn.rate_limit", kind: "semaphore", bodyKind: "Semaphore",
    body: { permits_total: 5, permits_available: 3 },
    source: "src/server/limits.rs:28", birthAgeMs: 3580000,
    meta: { level: "debug", scope: "rate_limiter" },
    status: { label: "3/5 permits", tone: "warn" }, stat: "3/5",
  },
  {
    id: "notify_shutdown", name: "shutdown.signal", kind: "notify", bodyKind: "Notify",
    body: { waiters: 2 },
    source: "src/lifecycle.rs:15", birthAgeMs: 3600000,
    meta: { level: "info" },
    status: { label: "waiting", tone: "neutral" }, stat: "2 waiters",
  },
  {
    id: "oncecell_config", name: "AppConfig", kind: "oncecell", bodyKind: "OnceCell",
    body: { state: "initializing" },
    source: "src/config.rs:8", birthAgeMs: 1800000,
    meta: { level: "info", config_path: "/etc/app/config.toml" },
    status: { label: "initializing", tone: "warn" }, stat: "1 waiter",
  },
  {
    id: "cmd_migrate", name: "db-migrate", kind: "command", bodyKind: "Command",
    body: { program: "db-migrate", args: ["--up", "--env=staging"] },
    source: "src/bootstrap.rs:55", birthAgeMs: 45000,
    meta: { level: "info", exit_code: null },
    status: { label: "running", tone: "neutral" },
  },
  {
    id: "file_config", name: "config.toml", kind: "file_op", bodyKind: "FileOp",
    body: { op: "read", path: "/etc/app/config.toml" },
    source: "src/config.rs:22", birthAgeMs: 1799500,
    meta: { level: "debug", bytes: 4096 },
    status: { label: "reading", tone: "ok" },
  },
  {
    id: "net_peer", name: "peer:10.0.0.5:8080", kind: "net_connect", bodyKind: "NetConnect",
    body: { addr: "10.0.0.5:8080", transport: "tcp" },
    source: "src/net/peer.rs:31", birthAgeMs: 920000,
    meta: { level: "info", tls: true },
    status: { label: "connected", tone: "ok" },
  },
];

const MOCK_EDGES: MockEdgeDef[] = [
  { id: "e1", source: "resp_sleepy", target: "lock_state", kind: "needs" },
  { id: "e2", source: "lock_state", target: "ch_rx", kind: "needs" },
  { id: "e3", source: "ch_rx", target: "resp_sleepy", kind: "needs" },
  { id: "e4", source: "ch_tx", target: "ch_rx", kind: "channel_link" },
  { id: "e5", source: "req_sleepy", target: "resp_sleepy", kind: "rpc_link" },
  { id: "e6", source: "req_ping", target: "resp_ping", kind: "rpc_link" },
  { id: "e7", source: "req_ping", target: "lock_state", kind: "polls" },
  { id: "e8", source: "future_store", target: "ch_rx", kind: "polls" },
  { id: "e9", source: "oncecell_config", target: "file_config", kind: "needs" },
  { id: "e10", source: "cmd_migrate", target: "oncecell_config", kind: "polls" },
  { id: "e11", source: "net_peer", target: "sem_conns", kind: "polls" },
  { id: "e12", source: "notify_shutdown", target: "ch_tx", kind: "closed_by" },
];

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

validateEdges(MOCK_ENTITIES, MOCK_EDGES);

// ── Layout ────────────────────────────────────────────────────

function measureNodeDefs(defs: MockEntityDef[]): Map<string, { width: number; height: number }> {
  const container = document.createElement("div");
  container.style.cssText = "position:fixed;top:-9999px;left:-9999px;visibility:hidden;pointer-events:none;display:flex;flex-direction:column;align-items:flex-start;gap:4px;";
  document.body.appendChild(container);

  const elements: { id: string; el: HTMLDivElement }[] = [];
  for (const def of defs) {
    const el = document.createElement("div");
    el.className = `mockup-node${def.inCycle ? " mockup-node--cycle" : ""}`;

    const icon = document.createElement("span");
    icon.className = "mockup-node-icon";
    icon.style.cssText = "display:inline-flex;align-items:center;justify-content:center;width:18px;height:18px;flex-shrink:0;";
    el.appendChild(icon);

    const content = document.createElement("div");
    content.className = "mockup-node-content";

    const mainRow = document.createElement("div");
    mainRow.className = "mockup-node-main";
    const label = document.createElement("span");
    label.className = "mockup-node-label";
    label.textContent = def.name;
    mainRow.appendChild(label);
    content.appendChild(mainRow);

    const details = document.createElement("div");
    details.className = "mockup-node-details";
    const badgeEl = document.createElement("span");
    badgeEl.className = "badge badge--neutral";
    badgeEl.textContent = def.status.label;
    details.appendChild(badgeEl);
    const dot1 = document.createElement("span");
    dot1.className = "mockup-node-dot";
    dot1.textContent = "·";
    details.appendChild(dot1);
    const ageEl = document.createElement("span");
    ageEl.className = "ui-duration-display";
    ageEl.textContent = "00m00s";
    details.appendChild(ageEl);
    if (def.stat) {
      const dot2 = document.createElement("span");
      dot2.className = "mockup-node-dot";
      dot2.textContent = "·";
      details.appendChild(dot2);
      const statEl = document.createElement("span");
      statEl.className = "mockup-node-stat";
      statEl.textContent = def.stat;
      details.appendChild(statEl);
    }
    content.appendChild(details);

    el.appendChild(content);
    container.appendChild(el);
    elements.push({ id: def.id, el });
  }

  const sizes = new Map<string, { width: number; height: number }>();
  for (const { id, el } of elements) {
    sizes.set(id, { width: el.offsetWidth, height: el.offsetHeight });
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

  const elkEdgeMap = new Map(
    (result.edges ?? []).map((e: any) => [e.id, e.sections ?? []]),
  );

  const nodes: Node[] = entityDefs.map((def) => ({
    id: def.id,
    type: "mockNode",
    position: posMap.get(def.id) ?? { x: 0, y: 0 },
    data: {
      kind: def.kind, label: def.name, inCycle: def.inCycle ?? false, selected: false,
      status: def.status, birthAgeMs: def.birthAgeMs, stat: def.stat,
    },
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

// ── Custom node component ─────────────────────────────────────

const hiddenHandle: React.CSSProperties = { opacity: 0, width: 0, height: 0, minWidth: 0, minHeight: 0, position: "absolute", top: "50%", left: "50%", pointerEvents: "none" };

type MockNodeData = {
  kind: string; label: string; inCycle: boolean; selected: boolean;
  status: { label: string; tone: "ok" | "warn" | "crit" | "neutral" };
  birthAgeMs: number;
  stat?: string;
};

function MockNodeComponent({ data }: { data: MockNodeData }) {
  return (
    <>
      <Handle type="target" position={Position.Top} style={hiddenHandle} />
      <Handle type="source" position={Position.Bottom} style={hiddenHandle} />
      <div className={`mockup-node${data.inCycle ? " mockup-node--cycle" : ""}${data.selected ? " mockup-node--selected" : ""}`}>
        <span className="mockup-node-icon">{kindIcon(data.kind, 18)}</span>
        <div className="mockup-node-content">
          <div className="mockup-node-main">
            <span className="mockup-node-label">{data.label}</span>
          </div>
          <div className="mockup-node-details">
            <Badge tone={data.status.tone}>{data.status.label}</Badge>
            <span className="mockup-node-dot">&middot;</span>
            <DurationDisplay ms={data.birthAgeMs} />
            {data.stat && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <span className="mockup-node-stat">{data.stat}</span>
              </>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

function ElkRoutedEdge({ id, data, style, markerEnd, selected }: EdgeProps) {
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
      <path d={d} fill="none" stroke="transparent" strokeWidth={14} style={{ cursor: "pointer", pointerEvents: "all" }} />
      {selected && (
        <>
          <path d={d} fill="none" stroke="var(--accent, #3b82f6)" strokeWidth={10} strokeLinecap="round" opacity={0.18} className="mockup-edge-glow" />
          <path d={d} fill="none" stroke="var(--accent, #3b82f6)" strokeWidth={5} strokeLinecap="round" opacity={0.45} />
        </>
      )}
      <path
        id={id}
        d={d}
        style={{
          ...(style as React.CSSProperties),
          ...(selected ? { stroke: "var(--accent, #3b82f6)", strokeWidth: 2.5 } : {}),
        }}
        markerEnd={markerEnd as string}
        fill="none"
        className="react-flow__edge-path"
      />
    </g>
  );
}

const mockNodeTypes = { mockNode: MockNodeComponent };
const mockEdgeTypes = { elkrouted: ElkRoutedEdge };

// ── Graph panel ───────────────────────────────────────────────

type GraphSelection = { kind: "entity"; id: string } | { kind: "edge"; id: string } | null;

function MockGraphPanel({ selection, onSelect }: { selection: GraphSelection; onSelect: (sel: GraphSelection) => void }) {
  const [layout, setLayout] = useState<LayoutResult>({ nodes: [], edges: [] });

  useEffect(() => {
    const sizes = measureNodeDefs(MOCK_ENTITIES);
    layoutMockGraph(MOCK_ENTITIES, MOCK_EDGES, sizes).then(setLayout);
  }, []);

  const nodesWithSelection = useMemo(() =>
    layout.nodes.map((n) => ({
      ...n,
      data: { ...n.data, selected: selection?.kind === "entity" && n.id === selection.id },
    })),
    [layout.nodes, selection],
  );

  const edgesWithSelection = useMemo(() =>
    layout.edges.map((e) => ({
      ...e,
      selected: selection?.kind === "edge" && e.id === selection.id,
    })),
    [layout.edges, selection],
  );

  return (
    <div className="mockup-graph-panel">
      <div className="mockup-graph-toolbar">
        <div className="mockup-graph-toolbar-left">
          <span className="mockup-graph-stat">{MOCK_ENTITIES.length} entities</span>
          <span className="mockup-graph-stat">{MOCK_EDGES.length} edges</span>
        </div>
      </div>
      <div className="mockup-graph-flow">
        <ReactFlowProvider>
          <ReactFlow
            nodes={nodesWithSelection}
            edges={edgesWithSelection}
            nodeTypes={mockNodeTypes}
            edgeTypes={mockEdgeTypes}
            onNodeClick={(_event, node) => onSelect({ kind: "entity", id: node.id })}
            onEdgeClick={(_event, edge) => onSelect({ kind: "edge", id: edge.id })}
            onPaneClick={() => onSelect(null)}
            fitView
            fitViewOptions={{ padding: 0.3, maxZoom: 1.2 }}
            proOptions={{ hideAttribution: true }}
            minZoom={0.3}
            maxZoom={3}
            panOnDrag
            nodesDraggable={false}
            nodesConnectable={false}
            elementsSelectable
          >
            <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
            <Controls showInteractive={false} />
          </ReactFlow>
        </ReactFlowProvider>
      </div>
    </div>
  );
}

// ── Inspector ─────────────────────────────────────────────────

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
    case "Semaphore":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Permits available">
            <span className="mockup-inspector-mono">{body.permits_available} / {body.permits_total}</span>
          </KeyValueRow>
        </div>
      );
    case "Notify":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Waiters">
            <span className="mockup-inspector-mono">{body.waiters}</span>
          </KeyValueRow>
        </div>
      );
    case "OnceCell":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="State" icon={<CircleNotch size={12} weight="bold" />}>
            <Badge tone={body.state === "initialized" ? "ok" : "warn"}>
              {body.state}
            </Badge>
          </KeyValueRow>
        </div>
      );
    case "Command":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Program">
            <span className="mockup-inspector-mono">{body.program}</span>
          </KeyValueRow>
          <KeyValueRow label="Args">
            <span className="mockup-inspector-mono">{body.args?.join(" ") ?? "(none)"}</span>
          </KeyValueRow>
        </div>
      );
    case "FileOp":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Operation">
            <span className="mockup-inspector-mono">{body.op}</span>
          </KeyValueRow>
          <KeyValueRow label="Path">
            <span className="mockup-inspector-mono">{body.path}</span>
          </KeyValueRow>
        </div>
      );
    case "NetConnect":
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Address">
            <span className="mockup-inspector-mono">{body.addr}</span>
          </KeyValueRow>
          <KeyValueRow label="Transport">
            <span className="mockup-inspector-mono">{body.transport}</span>
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

const EDGE_KIND_LABELS: Record<MockEdgeDef["kind"], string> = {
  needs: "Causal dependency",
  polls: "Non-blocking observation",
  closed_by: "Closure cause",
  channel_link: "Channel pairing",
  rpc_link: "RPC pairing",
};

function EdgeInspectorContent({ edge }: { edge: MockEdgeDef }) {
  const srcEntity = MOCK_ENTITIES.find((e) => e.id === edge.source);
  const dstEntity = MOCK_ENTITIES.find((e) => e.id === edge.target);
  const tooltip = edgeTooltip(edge.kind, srcEntity?.name ?? edge.source, dstEntity?.name ?? edge.target);
  const isStructural = edge.kind === "rpc_link" || edge.kind === "channel_link";

  return (
    <>
      <div className="mockup-inspector-node-header">
        <span className={`mockup-inspector-node-icon${isStructural ? "" : " mockup-inspector-node-icon--causal"}`}>
          <LinkSimple size={16} weight="bold" />
        </span>
        <div className="mockup-inspector-node-header-text">
          <div className="mockup-inspector-node-kind">{edge.kind}</div>
          <div className="mockup-inspector-node-label">{EDGE_KIND_LABELS[edge.kind]}</div>
        </div>
      </div>

      <div className="mockup-inspector-alert-slot" />

      <div className="mockup-inspector-section">
        <KeyValueRow label="From" icon={srcEntity ? kindIcon(srcEntity.kind, 12) : undefined}>
          <span className="mockup-inspector-mono">{srcEntity?.name ?? edge.source}</span>
        </KeyValueRow>
        <KeyValueRow label="To" icon={dstEntity ? kindIcon(dstEntity.kind, 12) : undefined}>
          <span className="mockup-inspector-mono">{dstEntity?.name ?? edge.target}</span>
        </KeyValueRow>
      </div>

      <div className="mockup-inspector-section">
        <KeyValueRow label="Meaning">
          <span className="mockup-inspector-mono">{tooltip}</span>
        </KeyValueRow>
        <KeyValueRow label="Type">
          <Badge tone={isStructural ? "neutral" : edge.kind === "needs" ? "crit" : "warn"}>
            {isStructural ? "structural" : "causal"}
          </Badge>
        </KeyValueRow>
      </div>
    </>
  );
}

function MockInspectorPanel({
  collapsed,
  onToggleCollapse,
  selection,
}: {
  collapsed: boolean;
  onToggleCollapse: () => void;
  selection: GraphSelection;
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

  let content: React.ReactNode;
  if (selection?.kind === "entity") {
    const entity = MOCK_ENTITIES.find((e) => e.id === selection.id);
    content = entity ? <EntityInspectorContent entity={entity} /> : null;
  } else if (selection?.kind === "edge") {
    const edge = MOCK_EDGES.find((e) => e.id === selection.id);
    content = edge ? <EdgeInspectorContent edge={edge} /> : null;
  } else {
    content = <div className="mockup-inspector-empty">Select an entity or edge</div>;
  }

  return (
    <div className="mockup-inspector">
      <div className="mockup-inspector-header">
        <MagnifyingGlass size={14} weight="bold" />
        <span>Inspector</span>
        <ActionButton size="sm" onPress={onToggleCollapse} aria-label="Collapse inspector">
          <CaretRight size={14} weight="bold" />
        </ActionButton>
      </div>
      <div className="mockup-inspector-body">
        {content}
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

export function DeadlockDetectorMockup() {
  const [inspectorWidth, setInspectorWidth] = useState(340);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
  const [selection, setSelection] = useState<GraphSelection>({ kind: "entity", id: "resp_sleepy" });

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
            selection={selection}
            onSelect={setSelection}
          />
        }
        right={
          <MockInspectorPanel
            collapsed={inspectorCollapsed}
            onToggleCollapse={() => setInspectorCollapsed((v) => !v)}
            selection={selection}
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

// ── Page ──────────────────────────────────────────────────────

export function DeadlockDetectorPage() {
  return <DeadlockDetectorMockup />;
}
