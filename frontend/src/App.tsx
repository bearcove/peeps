import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import "./components/graph/graph.css";
import "./components/inspector/inspector.css";
import "./components/requests/requests.css";
import "./components/timeline/timeline.css";
import {
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  Handle,
  Position,
  Background,
  BackgroundVariant,
  Controls,
  type Node,
  type Edge,
  type EdgeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  CaretDown,
  CaretLeft,
  CaretRight,
  Camera,
  Aperture,
  CheckCircle,
  CircleNotch,
  CopySimple,
  DownloadSimple,
  FileRs,
  Ghost,
  LinkSimple,
  MagnifyingGlass,
  PaperPlaneTilt,
  Record,
  Stop,
  Timer,
  Crosshair,
  UploadSimple,
} from "@phosphor-icons/react";
import { SplitLayout } from "./ui/layout/SplitLayout";
import { Badge } from "./ui/primitives/Badge";
import { KeyValueRow } from "./ui/primitives/KeyValueRow";
import { DurationDisplay } from "./ui/primitives/DurationDisplay";
import { ActionButton } from "./ui/primitives/ActionButton";
import { FilterMenu, type FilterMenuItem } from "./ui/primitives/FilterMenu";
import { kindIcon, kindDisplayName } from "./nodeKindSpec";
import { apiClient, apiMode } from "./api";
import type {
  ConnectedProcessInfo,
  ConnectionsResponse,
  EntityBody,
  FrameSummary,
} from "./api/types";
import { Table, type Column } from "./ui/primitives/Table";
import { RecordingTimeline, formatElapsed } from "./components/timeline/RecordingTimeline";
import {
  convertSnapshot,
  getConnectedSubgraph,
  type EntityDef,
  type EdgeDef,
  type Tone,
  type MetaValue,
} from "./snapshot";
import {
  measureNodeDefs,
  layoutGraph,
  edgeTooltip,
  type ElkPoint,
  type LayoutResult,
  type RenderNodeForMeasure,
  type SubgraphScopeMode,
} from "./layout";
import {
  buildUnionLayout,
  computeChangeFrames,
  computeChangeSummaries,
  diffEntityBetweenFrames,
  nearestProcessedFrame,
  renderFrameFromUnion,
  type EntityDiff,
  type FrameChangeSummary,
  type UnionLayout,
} from "./recording/unionGraph";

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

// Body type helpers used locally for the inspector
type RequestBody = Extract<EntityBody, { request: unknown }>;
type ResponseBody = Extract<EntityBody, { response: unknown }>;

// ── Custom node component ──────────────────────────────────────

const hiddenHandle: React.CSSProperties = {
  opacity: 0,
  width: 0,
  height: 0,
  minWidth: 0,
  minHeight: 0,
  position: "absolute",
  top: "50%",
  left: "50%",
  pointerEvents: "none",
};

type MockNodeData = {
  kind: string;
  label: string;
  inCycle: boolean;
  selected: boolean;
  status: { label: string; tone: Tone };
  ageMs: number;
  stat?: string;
  statTone?: Tone;
  scopeHue?: number;
  ghost?: boolean;
};

function MockNodeComponent({ data }: { data: MockNodeData }) {
  const showScopeColor =
    data.scopeHue !== undefined && !data.inCycle && data.statTone !== "crit" && data.statTone !== "warn";
  return (
    <>
      <Handle type="target" position={Position.Top} style={hiddenHandle} />
      <Handle type="source" position={Position.Bottom} style={hiddenHandle} />
      <div
        className={[
          "mockup-node",
          data.inCycle && "mockup-node--cycle",
          data.selected && "mockup-node--selected",
          data.statTone === "crit" && "mockup-node--stat-crit",
          data.statTone === "warn" && "mockup-node--stat-warn",
          showScopeColor && "mockup-node--scope",
          data.ghost && "mockup-node--ghost",
        ]
          .filter(Boolean)
          .join(" ")}
        style={
          showScopeColor
            ? ({
                "--scope-h": String(data.scopeHue),
              } as React.CSSProperties)
            : undefined
        }
      >
        <span className="mockup-node-icon">{kindIcon(data.kind, 18)}</span>
        <div className="mockup-node-content">
          <div className="mockup-node-main">
            <span className="mockup-node-label">{data.label}</span>
            {data.ageMs > 3000 && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <DurationDisplay ms={data.ageMs} />
              </>
            )}
            {data.stat && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <span
                  className={[
                    "mockup-node-stat",
                    data.statTone === "crit" && "mockup-node-stat--crit",
                    data.statTone === "warn" && "mockup-node-stat--warn",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                >
                  {data.stat}
                </span>
              </>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

type ChannelPairNodeData = {
  tx: EntityDef;
  rx: EntityDef;
  channelName: string;
  selected: boolean;
  statTone?: Tone;
  scopeHue?: number;
  ghost?: boolean;
};

const visibleHandleTop: React.CSSProperties = {
  width: 10,
  height: 6,
  minWidth: 0,
  minHeight: 0,
  background: "var(--text-tertiary)",
  border: "none",
  borderRadius: "0 0 3px 3px",
  opacity: 0.5,
  top: 0,
  left: "50%",
  transform: "translateX(-50%)",
  pointerEvents: "none",
};

const visibleHandleBottom: React.CSSProperties = {
  width: 10,
  height: 6,
  minWidth: 0,
  minHeight: 0,
  background: "var(--text-tertiary)",
  border: "none",
  borderRadius: "3px 3px 0 0",
  opacity: 0.5,
  bottom: 0,
  left: "50%",
  transform: "translateX(-50%)",
  pointerEvents: "none",
};

function ChannelPairNode({ data }: { data: ChannelPairNodeData }) {
  const { tx, rx, channelName, selected, statTone, scopeHue, ghost } = data;
  const txEp = typeof tx.body !== "string" && "channel_tx" in tx.body ? tx.body.channel_tx : null;
  const rxEp = typeof rx.body !== "string" && "channel_rx" in rx.body ? rx.body.channel_rx : null;

  const mpscBuffer = txEp && "mpsc" in txEp.details ? txEp.details.mpsc.buffer : null;

  const txLifecycle = txEp ? (txEp.lifecycle === "open" ? "open" : "closed") : "?";
  const rxLifecycle = rxEp ? (rxEp.lifecycle === "open" ? "open" : "closed") : "?";
  const txTone: Tone = txLifecycle === "open" ? "ok" : "neutral";
  const rxTone: Tone = rxLifecycle === "open" ? "ok" : "neutral";

  const bufferStat = mpscBuffer ? `${mpscBuffer.occupancy}/${mpscBuffer.capacity ?? "∞"}` : tx.stat;
  const showScopeColor = scopeHue !== undefined && statTone !== "crit" && statTone !== "warn";

  return (
    <>
      <Handle type="target" position={Position.Top} style={visibleHandleTop} />
      <Handle type="source" position={Position.Bottom} style={visibleHandleBottom} />
      <div
        className={[
          "mockup-channel-pair",
          selected && "mockup-channel-pair--selected",
          statTone === "crit" && "mockup-channel-pair--stat-crit",
          statTone === "warn" && "mockup-channel-pair--stat-warn",
          showScopeColor && "mockup-channel-pair--scope",
          ghost && "mockup-channel-pair--ghost",
        ]
          .filter(Boolean)
          .join(" ")}
        style={
          showScopeColor
            ? ({
                "--scope-h": String(scopeHue),
              } as React.CSSProperties)
            : undefined
        }
      >
        <div className="mockup-channel-pair-header">
          <span className="mockup-channel-pair-icon">{kindIcon("channel_pair", 14)}</span>
          <span className="mockup-channel-pair-name">{channelName}</span>
        </div>
        <div className="mockup-channel-pair-rows">
          <div className="mockup-channel-pair-row">
            <span className="mockup-channel-pair-row-label">TX</span>
            <Badge tone={txTone}>{txLifecycle}</Badge>
            {tx.ageMs > 3000 && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <DurationDisplay ms={tx.ageMs} />
              </>
            )}
            {bufferStat && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <span
                  className={[
                    "mockup-node-stat",
                    statTone === "crit" && "mockup-node-stat--crit",
                    statTone === "warn" && "mockup-node-stat--warn",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                >
                  {bufferStat}
                </span>
              </>
            )}
          </div>
          <div className="mockup-channel-pair-row">
            <span className="mockup-channel-pair-row-label">RX</span>
            <Badge tone={rxTone}>{rxLifecycle}</Badge>
            {rx.ageMs > 3000 && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <DurationDisplay ms={rx.ageMs} />
              </>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

type RpcPairNodeData = {
  req: EntityDef;
  resp: EntityDef;
  rpcName: string;
  selected: boolean;
  scopeHue?: number;
  ghost?: boolean;
};

function RpcPairNode({ data }: { data: RpcPairNodeData }) {
  const { req, resp, rpcName, selected, scopeHue, ghost } = data;

  const reqBody = typeof req.body !== "string" && "request" in req.body ? req.body.request : null;
  const respBody =
    typeof resp.body !== "string" && "response" in resp.body ? resp.body.response : null;

  const respStatus = respBody ? respBody.status : "pending";
  const respTone: Tone = respStatus === "ok" ? "ok" : respStatus === "error" ? "crit" : "warn";
  const method = respBody?.method ?? reqBody?.method ?? "?";
  const showScopeColor = scopeHue !== undefined && respStatus !== "error";

  return (
    <>
      <Handle type="target" position={Position.Top} style={visibleHandleTop} />
      <Handle type="source" position={Position.Bottom} style={visibleHandleBottom} />
      <div
        className={[
          "mockup-channel-pair",
          selected && "mockup-channel-pair--selected",
          respStatus === "error" && "mockup-channel-pair--stat-crit",
          showScopeColor && "mockup-channel-pair--scope",
          ghost && "mockup-channel-pair--ghost",
        ]
          .filter(Boolean)
          .join(" ")}
        style={
          showScopeColor
            ? ({
                "--scope-h": String(scopeHue),
              } as React.CSSProperties)
            : undefined
        }
      >
        <div className="mockup-channel-pair-header">
          <span className="mockup-channel-pair-icon">{kindIcon("rpc_pair", 14)}</span>
          <span className="mockup-channel-pair-name">{rpcName}</span>
        </div>
        <div className="mockup-channel-pair-rows">
          <div className="mockup-channel-pair-row">
            <span className="mockup-channel-pair-row-label">fn</span>
            <span className="mockup-inspector-mono" style={{ fontSize: "11px" }}>
              {method}
            </span>
          </div>
          <div className="mockup-channel-pair-row">
            <span className="mockup-channel-pair-row-label">→</span>
            <Badge tone={respTone}>{respStatus}</Badge>
            {resp.ageMs > 3000 && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <DurationDisplay ms={resp.ageMs} />
              </>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

type ScopeGroupNodeData = {
  isScopeGroup: true;
  label: string;
  count: number;
  selected: boolean;
};

function ScopeGroupNode({ data }: { data: ScopeGroupNodeData }) {
  return (
    <div className="mockup-scope-group">
      <div className="mockup-scope-group-header">
        <span className="mockup-scope-group-label">{data.label}</span>
        <span className="mockup-scope-group-meta">{data.count}</span>
      </div>
    </div>
  );
}

function ElkRoutedEdge({ id, data, style, markerEnd, selected }: EdgeProps) {
  const edgeData = data as { points?: ElkPoint[]; tooltip?: string; ghost?: boolean } | undefined;
  const points = edgeData?.points ?? [];
  const ghost = edgeData?.ghost ?? false;
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
    <g style={ghost ? { opacity: 0.2, pointerEvents: "none" } : undefined}>
      <path
        d={d}
        fill="none"
        stroke="transparent"
        strokeWidth={14}
        style={{ cursor: "pointer", pointerEvents: ghost ? "none" : "all" }}
      />
      {selected && (
        <>
          <path
            d={d}
            fill="none"
            stroke="var(--accent, #3b82f6)"
            strokeWidth={10}
            strokeLinecap="round"
            opacity={0.18}
            className="mockup-edge-glow"
          />
          <path
            d={d}
            fill="none"
            stroke="var(--accent, #3b82f6)"
            strokeWidth={5}
            strokeLinecap="round"
            opacity={0.45}
          />
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

const mockNodeTypes = {
  mockNode: MockNodeComponent,
  channelPairNode: ChannelPairNode,
  rpcPairNode: RpcPairNode,
  scopeGroupNode: ScopeGroupNode,
};
const mockEdgeTypes = { elkrouted: ElkRoutedEdge };

// ── Render callback for layout measurement ────────────────────

const renderNodeForMeasure: RenderNodeForMeasure = (def) => {
  if (def.channelPair) {
    return (
      <ChannelPairNode
        data={{
          tx: def.channelPair.tx,
          rx: def.channelPair.rx,
          channelName: def.name,
          selected: false,
          statTone: def.statTone,
        }}
      />
    );
  }
  if (def.rpcPair) {
    return (
      <RpcPairNode
        data={{
          req: def.rpcPair.req,
          resp: def.rpcPair.resp,
          rpcName: def.name,
          selected: false,
        }}
      />
    );
  }
  return (
    <MockNodeComponent
      data={{
        kind: def.kind,
        label: def.name,
        inCycle: def.inCycle,
        selected: false,
        status: def.status,
        ageMs: def.ageMs,
        stat: def.stat,
        statTone: def.statTone,
      }}
    />
  );
};

// ── Graph panel ────────────────────────────────────────────────

type GraphSelection = { kind: "entity"; id: string } | { kind: "edge"; id: string } | null;
type SnapPhase = "idle" | "cutting" | "loading" | "ready" | "error";

const GRAPH_EMPTY_MESSAGES: Record<SnapPhase, string> = {
  idle: "Take a snapshot to see the current state",
  cutting: "Waiting for all processes to sync…",
  loading: "Loading snapshot data…",
  ready: "No entities in snapshot",
  error: "Snapshot failed",
};

type ScopeColorMode = "none" | "process" | "crate";

const SCOPE_COLOR_HUES = [208, 158, 34, 276, 18, 124, 332, 248, 54, 188, 14, 300] as const;

function hashString(value: string): number {
  let h = 0;
  for (let i = 0; i < value.length; i++) {
    h = (h * 31 + value.charCodeAt(i)) >>> 0;
  }
  return h;
}

function scopeHueForKey(scopeKey: string): number {
  return SCOPE_COLOR_HUES[hashString(scopeKey) % SCOPE_COLOR_HUES.length];
}

function GraphFlow({
  nodes,
  edges,
  onSelect,
  suppressAutoFit,
}: {
  nodes: Node[];
  edges: Edge[];
  onSelect: (sel: GraphSelection) => void;
  /** When true, skip automatic fitView on structure changes (used during scrubbing). */
  suppressAutoFit?: boolean;
}) {
  const { fitView } = useReactFlow();
  const hasFittedRef = useRef(false);

  // Only refit when the graph structure changes (nodes/edges added or removed),
  // not on selection changes which also mutate the nodes array.
  const layoutKey = useMemo(
    () => nodes.map((n) => n.id).join(",") + "|" + edges.map((e) => e.id).join(","),
    [nodes, edges],
  );
  useEffect(() => {
    if (suppressAutoFit && hasFittedRef.current) return;
    fitView({ padding: 0.3, maxZoom: 1.2, duration: 0 });
    hasFittedRef.current = true;
  }, [layoutKey, fitView, suppressAutoFit]);

  // Press F to fit the view.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "f" && !e.metaKey && !e.ctrlKey && !e.altKey) {
        const tag = (e.target as HTMLElement).tagName;
        if (tag === "INPUT" || tag === "TEXTAREA") return;
        fitView({ padding: 0.3, maxZoom: 1.2, duration: 300 });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [fitView]);

  return (
    <ReactFlow
      nodes={nodes}
      edges={edges}
      nodeTypes={mockNodeTypes}
      edgeTypes={mockEdgeTypes}
      onNodeClick={(_event, node) => {
        if ((node.data as { isScopeGroup?: boolean } | undefined)?.isScopeGroup) return;
        onSelect({ kind: "entity", id: node.id });
      }}
      onEdgeClick={(_event, edge) => onSelect({ kind: "edge", id: edge.id })}
      onPaneClick={() => onSelect(null)}
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
  );
}

function GraphPanel({
  entityDefs,
  edgeDefs,
  snapPhase,
  selection,
  onSelect,
  focusedEntityId,
  onExitFocus,
  waitingForProcesses,
  crateItems,
  hiddenKrates,
  onKrateToggle,
  onKrateSolo,
  processItems,
  hiddenProcesses,
  onProcessToggle,
  onProcessSolo,
  scopeColorMode,
  onToggleProcessColorBy,
  onToggleCrateColorBy,
  useSubgraphs,
  onToggleProcessSubgraphs,
  onToggleCrateSubgraphs,
  unionFrameLayout,
}: {
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  snapPhase: SnapPhase;
  selection: GraphSelection;
  onSelect: (sel: GraphSelection) => void;
  focusedEntityId: string | null;
  onExitFocus: () => void;
  waitingForProcesses: boolean;
  crateItems: FilterMenuItem[];
  hiddenKrates: ReadonlySet<string>;
  onKrateToggle: (krate: string) => void;
  onKrateSolo: (krate: string) => void;
  processItems: FilterMenuItem[];
  hiddenProcesses: ReadonlySet<string>;
  onProcessToggle: (pid: string) => void;
  onProcessSolo: (pid: string) => void;
  scopeColorMode: ScopeColorMode;
  onToggleProcessColorBy: () => void;
  onToggleCrateColorBy: () => void;
  useSubgraphs: boolean;
  onToggleProcessSubgraphs: () => void;
  onToggleCrateSubgraphs: () => void;
  /** When provided, use this pre-computed layout (union mode) instead of measuring + ELK. */
  unionFrameLayout?: LayoutResult;
}) {
  const [layout, setLayout] = useState<LayoutResult>({ nodes: [], edges: [] });
  const subgraphScopeMode: SubgraphScopeMode = useSubgraphs ? scopeColorMode : "none";

  // In snapshot mode (no unionFrameLayout), measure and lay out from scratch.
  React.useEffect(() => {
    if (unionFrameLayout) return; // skip — union mode provides layout directly
    if (entityDefs.length === 0) return;
    measureNodeDefs(entityDefs, renderNodeForMeasure)
      .then((sizes) => layoutGraph(entityDefs, edgeDefs, sizes, subgraphScopeMode))
      .then(setLayout)
      .catch(console.error);
  }, [entityDefs, edgeDefs, subgraphScopeMode, unionFrameLayout]);

  const effectiveLayout = unionFrameLayout ?? layout;

  const entityById = useMemo(() => new Map(entityDefs.map((entity) => [entity.id, entity])), [entityDefs]);

  const nodesWithSelection = useMemo(
    () =>
      effectiveLayout.nodes.map((n) => {
        const entity = entityById.get(n.id);
        const scopeKey =
          !entity
            ? undefined
            : scopeColorMode === "process"
              ? entity.processId
              : scopeColorMode === "crate"
                ? (entity.krate ?? "~no-crate")
                : undefined;
        return {
          ...n,
          data: {
            ...n.data,
            selected: selection?.kind === "entity" && n.id === selection.id,
            scopeHue: scopeKey ? scopeHueForKey(scopeKey) : undefined,
          },
        };
      }),
    [effectiveLayout.nodes, entityById, scopeColorMode, selection],
  );

  const edgesWithSelection = useMemo(
    () =>
      effectiveLayout.edges.map((e) => ({
        ...e,
        selected: selection?.kind === "edge" && e.id === selection.id,
      })),
    [effectiveLayout.edges, selection],
  );

  const isBusy = snapPhase === "cutting" || snapPhase === "loading";
  const showToolbar = crateItems.length > 1 || processItems.length > 0 || focusedEntityId;

  return (
    <div className="mockup-graph-panel">
      {showToolbar && (
        <div className="mockup-graph-toolbar">
          <div className="mockup-graph-toolbar-left">
            {entityDefs.length > 0 && (
              <>
                <span className="mockup-graph-stat">{entityDefs.length} entities</span>
                <span className="mockup-graph-stat">{edgeDefs.length} edges</span>
              </>
            )}
          </div>
          <div className="mockup-graph-toolbar-right">
            {processItems.length > 0 && (
              <FilterMenu
                label="Process"
                items={processItems}
                hiddenIds={hiddenProcesses}
                onToggle={onProcessToggle}
                onSolo={onProcessSolo}
                colorByActive={scopeColorMode === "process"}
                onToggleColorBy={onToggleProcessColorBy}
                colorByLabel="Use process colors"
                subgraphsActive={scopeColorMode === "process" && useSubgraphs}
                onToggleSubgraphs={onToggleProcessSubgraphs}
                subgraphsLabel="Use subgraphs"
              />
            )}
            {crateItems.length > 1 && (
              <FilterMenu
                label="Crate"
                items={crateItems}
                hiddenIds={hiddenKrates}
                onToggle={onKrateToggle}
                onSolo={onKrateSolo}
                colorByActive={scopeColorMode === "crate"}
                onToggleColorBy={onToggleCrateColorBy}
                colorByLabel="Use crate colors"
                subgraphsActive={scopeColorMode === "crate" && useSubgraphs}
                onToggleSubgraphs={onToggleCrateSubgraphs}
                subgraphsLabel="Use subgraphs"
              />
            )}
            {focusedEntityId && (
              <ActionButton onPress={onExitFocus}>
                <Crosshair size={14} weight="bold" />
                Exit Focus
              </ActionButton>
            )}
          </div>
        </div>
      )}
      {entityDefs.length === 0 ? (
        <div className="mockup-graph-empty">
          {isBusy ? (
            <>
              <CircleNotch size={24} weight="bold" className="spinning mockup-graph-empty-icon" />{" "}
              {GRAPH_EMPTY_MESSAGES[snapPhase]}
            </>
          ) : snapPhase === "idle" && waitingForProcesses ? (
            <>
              <CircleNotch size={24} weight="bold" className="spinning mockup-graph-empty-icon" />
              <span>Waiting for a process to connect…</span>
            </>
          ) : snapPhase === "idle" ? (
            <>
              <Camera size={32} weight="thin" className="mockup-graph-empty-icon" />
              <span>{GRAPH_EMPTY_MESSAGES[snapPhase]}</span>
              <span className="mockup-graph-empty-hint">
                Press "Take Snapshot" to capture the current state of all connected processes
              </span>
            </>
          ) : (
            GRAPH_EMPTY_MESSAGES[snapPhase]
          )}
        </div>
      ) : (
        <div className="mockup-graph-flow">
          <ReactFlowProvider>
            <GraphFlow
              nodes={nodesWithSelection}
              edges={edgesWithSelection}
              onSelect={onSelect}
              suppressAutoFit={!!unionFrameLayout}
            />
          </ReactFlowProvider>
        </div>
      )}
    </div>
  );
}

// ── Inspector ──────────────────────────────────────────────────

function EntityBodySection({ entity }: { entity: EntityDef }) {
  const { body } = entity;

  if (typeof body === "string") {
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Body">
          <span className="mockup-inspector-mono mockup-inspector-muted">
            Future (no body fields)
          </span>
        </KeyValueRow>
      </div>
    );
  }

  if ("request" in body) {
    const req = (body as RequestBody).request;
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Args">
          <span
            className={`mockup-inspector-mono${req.args_preview === "(no args)" ? " mockup-inspector-muted" : ""}`}
          >
            {req.args_preview}
          </span>
        </KeyValueRow>
      </div>
    );
  }

  if ("response" in body) {
    const resp = (body as ResponseBody).response;
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Method" icon={<PaperPlaneTilt size={12} weight="bold" />}>
          <span className="mockup-inspector-mono">{resp.method}</span>
        </KeyValueRow>
        <KeyValueRow label="Status">
          <Badge tone={resp.status === "ok" ? "ok" : resp.status === "error" ? "crit" : "warn"}>
            {resp.status}
          </Badge>
        </KeyValueRow>
      </div>
    );
  }

  if ("lock" in body) {
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Lock kind">
          <span className="mockup-inspector-mono">{body.lock.kind}</span>
        </KeyValueRow>
      </div>
    );
  }

  if ("channel_tx" in body || "channel_rx" in body) {
    const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
    const lc = ep.lifecycle;
    const lifecycleLabel = typeof lc === "string" ? lc : `closed (${Object.values(lc)[0]})`;
    const lifecycleTone: Tone = lc === "open" ? "ok" : "neutral";
    const channelKind =
      "mpsc" in ep.details
        ? "mpsc"
        : "broadcast" in ep.details
          ? "broadcast"
          : "watch" in ep.details
            ? "watch"
            : "oneshot";
    const mpscBuffer = "mpsc" in ep.details ? ep.details.mpsc.buffer : null;
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Lifecycle">
          <Badge tone={lifecycleTone}>{lifecycleLabel}</Badge>
        </KeyValueRow>
        <KeyValueRow label="Channel kind">
          <span className="mockup-inspector-mono">{channelKind}</span>
        </KeyValueRow>
        {mpscBuffer && (
          <>
            <KeyValueRow label="Capacity">
              <span className="mockup-inspector-mono">{mpscBuffer.capacity ?? "∞"}</span>
            </KeyValueRow>
            <KeyValueRow label="Queue length">
              <span className="mockup-inspector-mono">{mpscBuffer.occupancy}</span>
            </KeyValueRow>
          </>
        )}
      </div>
    );
  }

  if ("semaphore" in body) {
    const { max_permits, handed_out_permits } = body.semaphore;
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Permits available">
          <span className="mockup-inspector-mono">
            {max_permits - handed_out_permits} / {max_permits}
          </span>
        </KeyValueRow>
      </div>
    );
  }

  if ("notify" in body) {
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Waiters">
          <span className="mockup-inspector-mono">{body.notify.waiter_count}</span>
        </KeyValueRow>
      </div>
    );
  }

  if ("once_cell" in body) {
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="State">
          <Badge tone={body.once_cell.state === "initialized" ? "ok" : "warn"}>
            {body.once_cell.state}
          </Badge>
        </KeyValueRow>
        {body.once_cell.waiter_count > 0 && (
          <KeyValueRow label="Waiters">
            <span className="mockup-inspector-mono">{body.once_cell.waiter_count}</span>
          </KeyValueRow>
        )}
      </div>
    );
  }

  if ("command" in body) {
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Program">
          <span className="mockup-inspector-mono">{body.command.program}</span>
        </KeyValueRow>
        <KeyValueRow label="Args">
          <span className="mockup-inspector-mono">{body.command.args.join(" ") || "(none)"}</span>
        </KeyValueRow>
      </div>
    );
  }

  if ("file_op" in body) {
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Operation">
          <span className="mockup-inspector-mono">{body.file_op.op}</span>
        </KeyValueRow>
        <KeyValueRow label="Path">
          <span className="mockup-inspector-mono">{body.file_op.path}</span>
        </KeyValueRow>
      </div>
    );
  }

  for (const netKey of ["net_connect", "net_accept", "net_read", "net_write"] as const) {
    if (netKey in body) {
      const net = (body as Record<string, { addr: string }>)[netKey];
      return (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Address">
            <span className="mockup-inspector-mono">{net.addr}</span>
          </KeyValueRow>
        </div>
      );
    }
  }

  return null;
}

function ChannelPairInspectorContent({
  entity,
  onFocus,
}: {
  entity: EntityDef;
  onFocus: (id: string) => void;
}) {
  const { tx, rx } = entity.channelPair!;
  const txEp = typeof tx.body !== "string" && "channel_tx" in tx.body ? tx.body.channel_tx : null;
  const rxEp = typeof rx.body !== "string" && "channel_rx" in rx.body ? rx.body.channel_rx : null;

  const channelKind = txEp
    ? "mpsc" in txEp.details
      ? "mpsc"
      : "broadcast" in txEp.details
        ? "broadcast"
        : "watch" in txEp.details
          ? "watch"
          : "oneshot"
    : null;

  const mpscBuffer = txEp && "mpsc" in txEp.details ? txEp.details.mpsc.buffer : null;

  function lifecycleLabel(ep: { lifecycle: "open" | { closed: string } } | null): string {
    if (!ep) return "?";
    const lc = ep.lifecycle;
    return typeof lc === "string" ? lc : `closed (${Object.values(lc)[0]})`;
  }
  function lifecycleTone(ep: { lifecycle: "open" | { closed: string } } | null): Tone {
    if (!ep) return "neutral";
    return ep.lifecycle === "open" ? "ok" : "neutral";
  }

  const bufferFill =
    mpscBuffer && mpscBuffer.capacity != null
      ? Math.min(100, (mpscBuffer.occupancy / mpscBuffer.capacity) * 100)
      : null;
  const bufferTone: Tone =
    mpscBuffer && mpscBuffer.capacity != null
      ? mpscBuffer.occupancy >= mpscBuffer.capacity
        ? "crit"
        : mpscBuffer.occupancy / mpscBuffer.capacity >= 0.75
          ? "warn"
          : "ok"
      : "ok";

  return (
    <>
      <div className="mockup-inspector-node-header">
        <span className="mockup-inspector-node-icon mockup-inspector-node-icon--channel-pair">
          <span style={{ fontSize: 9, lineHeight: 1 }}>TX</span>
          <span style={{ fontSize: 9, lineHeight: 1 }}>RX</span>
        </span>
        <div className="mockup-inspector-node-header-text">
          <div className="mockup-inspector-node-kind">Channel</div>
          <div className="mockup-inspector-node-label">{entity.name}</div>
        </div>
        <ActionButton onPress={() => onFocus(entity.id)}>
          <Crosshair size={14} weight="bold" />
          Focus
        </ActionButton>
      </div>

      <div className="mockup-inspector-alert-slot" />

      {channelKind && (
        <div className="mockup-inspector-section">
          <KeyValueRow label="Type">
            <span className="mockup-inspector-mono">{channelKind}</span>
          </KeyValueRow>
          {mpscBuffer && (
            <KeyValueRow label="Buffer">
              <span className="mockup-inspector-mono">
                {mpscBuffer.occupancy} / {mpscBuffer.capacity ?? "∞"}
              </span>
              {bufferFill != null && (
                <div className="mockup-inspector-buffer-bar">
                  <div
                    className={`mockup-inspector-buffer-fill mockup-inspector-buffer-fill--${bufferTone}`}
                    style={{ width: `${bufferFill}%` }}
                  />
                </div>
              )}
            </KeyValueRow>
          )}
        </div>
      )}

      <div className="mockup-inspector-subsection-label">TX</div>
      <div className="mockup-inspector-section">
        <KeyValueRow label="Lifecycle">
          <Badge tone={lifecycleTone(txEp)}>{lifecycleLabel(txEp)}</Badge>
        </KeyValueRow>
        <KeyValueRow label="Age" icon={<Timer size={12} weight="bold" />}>
          <DurationDisplay ms={tx.ageMs} />
        </KeyValueRow>
        <KeyValueRow label="Source" icon={<FileRs size={12} weight="bold" />}>
          <a
            className="mockup-inspector-source-link"
            href={`zed://file${tx.source}`}
            title="Open in Zed"
          >
            {tx.source}
          </a>
        </KeyValueRow>
        {tx.krate && (
          <KeyValueRow label="Crate">
            <span className="mockup-inspector-mono">{tx.krate}</span>
          </KeyValueRow>
        )}
      </div>

      <div className="mockup-inspector-subsection-label">RX</div>
      <div className="mockup-inspector-section">
        <KeyValueRow label="Lifecycle">
          <Badge tone={lifecycleTone(rxEp)}>{lifecycleLabel(rxEp)}</Badge>
        </KeyValueRow>
        <KeyValueRow label="Age" icon={<Timer size={12} weight="bold" />}>
          <DurationDisplay ms={rx.ageMs} />
        </KeyValueRow>
        <KeyValueRow label="Source" icon={<FileRs size={12} weight="bold" />}>
          <a
            className="mockup-inspector-source-link"
            href={`zed://file${rx.source}`}
            title="Open in Zed"
          >
            {rx.source}
          </a>
        </KeyValueRow>
        {rx.krate && (
          <KeyValueRow label="Crate">
            <span className="mockup-inspector-mono">{rx.krate}</span>
          </KeyValueRow>
        )}
      </div>
    </>
  );
}

function EntityInspectorContent({
  entity,
  onFocus,
  entityDiff,
}: {
  entity: EntityDef;
  onFocus: (id: string) => void;
  entityDiff?: EntityDiff | null;
}) {
  if (entity.channelPair) {
    return <ChannelPairInspectorContent entity={entity} onFocus={onFocus} />;
  }

  const ageTone: Tone =
    entity.ageMs > 600_000 ? "crit" : entity.ageMs > 60_000 ? "warn" : "neutral";

  return (
    <>
      <div className="mockup-inspector-node-header">
        <span className="mockup-inspector-node-icon">{kindIcon(entity.kind, 16)}</span>
        <div className="mockup-inspector-node-header-text">
          <div className="mockup-inspector-node-kind">{kindDisplayName(entity.kind)}</div>
          <div className="mockup-inspector-node-label">{entity.name}</div>
        </div>
        <ActionButton onPress={() => onFocus(entity.id)}>
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

      {entityDiff && (entityDiff.appeared || entityDiff.disappeared || entityDiff.statusChanged || entityDiff.statChanged) && (
        <div className="mockup-inspector-diff">
          {entityDiff.appeared && (
            <Badge tone="ok">appeared this frame</Badge>
          )}
          {entityDiff.disappeared && (
            <Badge tone="warn">disappeared this frame</Badge>
          )}
          {entityDiff.statusChanged && (
            <div className="mockup-inspector-diff-row">
              <span className="mockup-inspector-diff-label">Status</span>
              <span className="mockup-inspector-diff-from">{entityDiff.statusChanged.from}</span>
              <span className="mockup-inspector-diff-arrow">→</span>
              <span className="mockup-inspector-diff-to">{entityDiff.statusChanged.to}</span>
            </div>
          )}
          {entityDiff.statChanged && (
            <div className="mockup-inspector-diff-row">
              <span className="mockup-inspector-diff-label">Stat</span>
              <span className="mockup-inspector-diff-from">{entityDiff.statChanged.from ?? "—"}</span>
              <span className="mockup-inspector-diff-arrow">→</span>
              <span className="mockup-inspector-diff-to">{entityDiff.statChanged.to ?? "—"}</span>
            </div>
          )}
        </div>
      )}

      <div className="mockup-inspector-section">
        <KeyValueRow label="Process">
          <span className="mockup-inspector-mono">{entity.processName}</span>
          <span className="mockup-inspector-muted" style={{ fontSize: "0.75em", marginLeft: 4 }}>
            {entity.processId}
          </span>
        </KeyValueRow>
        <KeyValueRow label="Source" icon={<FileRs size={12} weight="bold" />}>
          <a
            className="mockup-inspector-source-link"
            href={`zed://file${entity.source}`}
            title="Open in Zed"
          >
            {entity.source}
          </a>
        </KeyValueRow>
        {entity.krate && (
          <KeyValueRow label="Crate">
            <span className="mockup-inspector-mono">{entity.krate}</span>
          </KeyValueRow>
        )}
        <KeyValueRow label="Age" icon={<Timer size={12} weight="bold" />}>
          <DurationDisplay ms={entity.ageMs} tone={ageTone} />
        </KeyValueRow>
        <KeyValueRow label="PTime birth">
          <span className="mockup-inspector-mono">{entity.birthPtime}ms</span>
        </KeyValueRow>
        {isFinite(entity.birthApproxUnixMs) && entity.birthApproxUnixMs > 0 && (
          <KeyValueRow label="Born ~">
            <span className="mockup-inspector-mono">
              {new Date(entity.birthApproxUnixMs).toLocaleTimeString()}
            </span>
          </KeyValueRow>
        )}
      </div>

      <EntityBodySection entity={entity} />
      <MetaSection meta={entity.meta} />
    </>
  );
}

const EDGE_KIND_LABELS: Record<EdgeDef["kind"], string> = {
  needs: "Causal dependency",
  holds: "Permit ownership",
  polls: "Non-blocking observation",
  closed_by: "Closure cause",
  channel_link: "Channel pairing",
  rpc_link: "RPC pairing",
};

function EdgeInspectorContent({ edge, entityDefs }: { edge: EdgeDef; entityDefs: EntityDef[] }) {
  const srcEntity = entityDefs.find((e) => e.id === edge.source);
  const dstEntity = entityDefs.find((e) => e.id === edge.target);
  const tooltip = edgeTooltip(
    edge.kind,
    srcEntity?.name ?? edge.source,
    dstEntity?.name ?? edge.target,
  );
  const isStructural = edge.kind === "rpc_link" || edge.kind === "channel_link";

  return (
    <>
      <div className="mockup-inspector-node-header">
        <span
          className={`mockup-inspector-node-icon${isStructural ? "" : " mockup-inspector-node-icon--causal"}`}
        >
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
          {srcEntity && (
            <span className="mockup-inspector-muted" style={{ fontSize: "0.75em", marginLeft: 4 }}>
              {srcEntity.processName}
            </span>
          )}
        </KeyValueRow>
        <KeyValueRow label="To" icon={dstEntity ? kindIcon(dstEntity.kind, 12) : undefined}>
          <span className="mockup-inspector-mono">{dstEntity?.name ?? edge.target}</span>
          {dstEntity && (
            <span className="mockup-inspector-muted" style={{ fontSize: "0.75em", marginLeft: 4 }}>
              {dstEntity.processName}
            </span>
          )}
        </KeyValueRow>
      </div>

      <div className="mockup-inspector-section">
        <KeyValueRow label="Meaning">
          <span className="mockup-inspector-mono">{tooltip}</span>
        </KeyValueRow>
        <KeyValueRow label="Type">
          <Badge
            tone={
              isStructural ? "neutral" : edge.kind === "needs" ? "crit" : edge.kind === "holds" ? "ok" : "warn"
            }
          >
            {isStructural ? "structural" : "causal"}
          </Badge>
        </KeyValueRow>
      </div>
    </>
  );
}

function InspectorPanel({
  collapsed,
  onToggleCollapse,
  selection,
  entityDefs,
  edgeDefs,
  onFocusEntity,
  scrubbingUnionLayout,
  currentFrameIndex,
}: {
  collapsed: boolean;
  onToggleCollapse: () => void;
  selection: GraphSelection;
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  onFocusEntity: (id: string) => void;
  scrubbingUnionLayout?: UnionLayout;
  currentFrameIndex?: number;
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
    const entity = entityDefs.find((e) => e.id === selection.id);
    const entityDiff =
      entity && scrubbingUnionLayout && currentFrameIndex !== undefined && currentFrameIndex > 0
        ? diffEntityBetweenFrames(entity.id, currentFrameIndex, currentFrameIndex - 1, scrubbingUnionLayout)
        : null;
    content = entity ? <EntityInspectorContent entity={entity} onFocus={onFocusEntity} entityDiff={entityDiff} /> : null;
  } else if (selection?.kind === "edge") {
    const edge = edgeDefs.find((e) => e.id === selection.id);
    content = edge ? <EdgeInspectorContent edge={edge} entityDefs={entityDefs} /> : null;
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
      <div className="mockup-inspector-body">{content}</div>
    </div>
  );
}

function MetaTreeNode({
  name,
  value,
  depth = 0,
}: {
  name: string;
  value: MetaValue;
  depth?: number;
}) {
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
          style={{
            transform: expanded ? undefined : "rotate(-90deg)",
            transition: "transform 0.15s",
          }}
        />
        <span className="mockup-meta-key">{name}</span>
        <span className="mockup-meta-hint">
          {isArray ? `[${entries.length}]` : `{${entries.length}}`}
        </span>
      </button>
      {expanded &&
        entries.map(([k, v]) => <MetaTreeNode key={k} name={k} value={v} depth={depth + 1} />)}
    </div>
  );
}

function MetaSection({ meta }: { meta: Record<string, MetaValue> | null }) {
  if (!meta || Object.keys(meta).length === 0) return null;
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

// ── Snapshot state machine ─────────────────────────────────────

type SnapshotState =
  | { phase: "idle" }
  | { phase: "cutting" }
  | { phase: "loading" }
  | { phase: "ready"; entities: EntityDef[]; edges: EdgeDef[] }
  | { phase: "error"; message: string };

// ── Recording state ────────────────────────────────────────────

type RecordingState =
  | { phase: "idle" }
  | {
      phase: "recording";
      sessionId: string;
      startedAt: number;
      frameCount: number;
      elapsed: number;
      approxMemoryBytes: number;
      maxMemoryBytes: number;
    }
  | {
      phase: "stopped";
      sessionId: string;
      frameCount: number;
      frames: FrameSummary[];
      unionLayout: UnionLayout | null;
      buildingUnion: boolean;
      buildProgress?: [number, number];
      avgCaptureMs: number;
      maxCaptureMs: number;
      totalCaptureMs: number;
    }
  | {
      phase: "scrubbing";
      sessionId: string;
      frameCount: number;
      frames: FrameSummary[];
      currentFrameIndex: number;
      unionLayout: UnionLayout;
      avgCaptureMs: number;
      maxCaptureMs: number;
      totalCaptureMs: number;
    };

// ── Process modal ──────────────────────────────────────────────

const PROCESS_COLUMNS: readonly Column<ConnectedProcessInfo>[] = [
  { key: "conn_id", label: "Conn", width: "60px", render: (r) => r.conn_id },
  { key: "process_name", label: "Name", render: (r) => r.process_name },
  { key: "pid", label: "PID", width: "80px", render: (r) => r.pid },
];

function ProcessModal({
  connections,
  onClose,
}: {
  connections: ConnectionsResponse;
  onClose: () => void;
}) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="mockup-modal-backdrop" onClick={onClose}>
      <div
        className="mockup-modal"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Connected processes"
      >
        <div className="mockup-modal-header">
          <span className="mockup-modal-title">Connected processes</span>
          <ActionButton size="sm" onPress={onClose}>
            ✕
          </ActionButton>
        </div>
        <div className="mockup-modal-body">
          <Table
            columns={PROCESS_COLUMNS}
            rows={connections.processes}
            rowKey={(r) => String(r.conn_id)}
            aria-label="Connected processes"
          />
          {connections.processes.length === 0 && (
            <div className="mockup-modal-empty">No processes connected</div>
          )}
        </div>
      </div>
    </div>
  );
}

// ── App ────────────────────────────────────────────────────────

export function App() {
  const [snap, setSnap] = useState<SnapshotState>({ phase: "idle" });
  const [inspectorWidth, setInspectorWidth] = useState(340);
  const [inspectorCollapsed, setInspectorCollapsed] = useState(false);
  const [selection, setSelection] = useState<GraphSelection>(null);
  const [connections, setConnections] = useState<ConnectionsResponse | null>(null);
  const [showProcessModal, setShowProcessModal] = useState(false);
  const [focusedEntityId, setFocusedEntityId] = useState<string | null>(null);
  const [hiddenKrates, setHiddenKrates] = useState<ReadonlySet<string>>(new Set());
  const [hiddenProcesses, setHiddenProcesses] = useState<ReadonlySet<string>>(new Set());
  const [scopeColorMode, setScopeColorMode] = useState<ScopeColorMode>("none");
  const [useSubgraphs, setUseSubgraphs] = useState(false);
  const [recording, setRecording] = useState<RecordingState>({ phase: "idle" });
  const [isLive, setIsLive] = useState(true);
  const [ghostMode, setGhostMode] = useState(false);
  const [unionFrameLayout, setUnionFrameLayout] = useState<LayoutResult | undefined>(undefined);
  const [downsampleInterval, setDownsampleInterval] = useState(1);
  const [builtDownsampleInterval, setBuiltDownsampleInterval] = useState(1);
  const pollingRef = useRef<number | null>(null);
  const isLiveRef = useRef(isLive);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const allEntities = snap.phase === "ready" ? snap.entities : [];
  const allEdges = snap.phase === "ready" ? snap.edges : [];

  const crateItems = useMemo<FilterMenuItem[]>(() => {
    const counts = new Map<string, number>();
    for (const e of allEntities) {
      const k = e.krate ?? "~no-crate";
      counts.set(k, (counts.get(k) ?? 0) + 1);
    }
    return Array.from(counts.keys())
      .sort()
      .map((k) => ({
        id: k,
        label: k === "~no-crate" ? "(no crate)" : k,
        meta: counts.get(k),
      }));
  }, [allEntities]);

  const processItems = useMemo<FilterMenuItem[]>(() => {
    const counts = new Map<string, number>();
    for (const e of allEntities) {
      counts.set(e.processId, (counts.get(e.processId) ?? 0) + 1);
    }
    return Array.from(counts.keys())
      .sort()
      .map((pid) => {
        const name = allEntities.find((e) => e.processId === pid)?.processName ?? pid;
        return { id: pid, label: name, meta: counts.get(pid) };
      });
  }, [allEntities]);

  const handleKrateToggle = useCallback((krate: string) => {
    setHiddenKrates((prev) => {
      const next = new Set(prev);
      if (next.has(krate)) next.delete(krate);
      else next.add(krate);
      return next;
    });
  }, []);

  const handleKrateSolo = useCallback(
    (krate: string) => {
      setHiddenKrates((prev) => {
        const otherKrates = crateItems.filter((i) => i.id !== krate).map((i) => i.id);
        const alreadySolo = otherKrates.every((id) => prev.has(id)) && !prev.has(krate);
        if (alreadySolo) return new Set();
        return new Set(otherKrates);
      });
    },
    [crateItems],
  );

  const handleProcessToggle = useCallback((pid: string) => {
    setHiddenProcesses((prev) => {
      const next = new Set(prev);
      if (next.has(pid)) next.delete(pid);
      else next.add(pid);
      return next;
    });
  }, []);

  const handleProcessSolo = useCallback(
    (pid: string) => {
      setHiddenProcesses((prev) => {
        const otherProcesses = processItems.filter((i) => i.id !== pid).map((i) => i.id);
        const alreadySolo = otherProcesses.every((id) => prev.has(id)) && !prev.has(pid);
        if (alreadySolo) return new Set();
        return new Set(otherProcesses);
      });
    },
    [processItems],
  );

  const handleToggleProcessColorBy = useCallback(() => {
    setScopeColorMode((prev) => {
      const next = prev === "process" ? "none" : "process";
      if (next === "none") setUseSubgraphs(false);
      return next;
    });
  }, []);

  const handleToggleCrateColorBy = useCallback(() => {
    setScopeColorMode((prev) => {
      const next = prev === "crate" ? "none" : "crate";
      if (next === "none") setUseSubgraphs(false);
      return next;
    });
  }, []);

  const handleToggleProcessSubgraphs = useCallback(() => {
    setScopeColorMode((prevScope) => {
      setUseSubgraphs((prevOn) => (prevScope === "process" ? !prevOn : true));
      return "process";
    });
  }, []);

  const handleToggleCrateSubgraphs = useCallback(() => {
    setScopeColorMode((prevScope) => {
      setUseSubgraphs((prevOn) => (prevScope === "crate" ? !prevOn : true));
      return "crate";
    });
  }, []);

  const { entities, edges } = useMemo(() => {
    const filtered = allEntities.filter(
      (e) =>
        (hiddenKrates.size === 0 || !hiddenKrates.has(e.krate ?? "~no-crate")) &&
        (hiddenProcesses.size === 0 || !hiddenProcesses.has(e.processId)),
    );
    if (!focusedEntityId) return { entities: filtered, edges: allEdges };
    return getConnectedSubgraph(focusedEntityId, filtered, allEdges);
  }, [focusedEntityId, allEntities, allEdges, hiddenKrates, hiddenProcesses]);

  const takeSnapshot = useCallback(async () => {
    setSnap({ phase: "cutting" });
    setSelection(null);
    setFocusedEntityId(null);
    try {
      const triggered = await apiClient.triggerCut();
      let status = await apiClient.fetchCutStatus(triggered.cut_id);
      while (status.pending_connections > 0) {
        await new Promise<void>((resolve) => window.setTimeout(resolve, 600));
        status = await apiClient.fetchCutStatus(triggered.cut_id);
      }
      setSnap({ phase: "loading" });
      const snapshot = await apiClient.fetchSnapshot();
      const converted = convertSnapshot(snapshot);
      setSnap({ phase: "ready", ...converted });
    } catch (err) {
      setSnap({ phase: "error", message: err instanceof Error ? err.message : String(err) });
    }
  }, []);

  const handleStartRecording = useCallback(async () => {
    try {
      const session = await apiClient.startRecording();
      const startedAt = Date.now();
      setRecording({
        phase: "recording",
        sessionId: session.session_id,
        startedAt,
        frameCount: session.frame_count,
        elapsed: 0,
        approxMemoryBytes: session.approx_memory_bytes,
        maxMemoryBytes: session.max_memory_bytes,
      });
      pollingRef.current = window.setInterval(() => {
        void (async () => {
          try {
            const current = await apiClient.fetchRecordingCurrent();
            if (!current.session) return;
            const elapsed = Date.now() - startedAt;
            setRecording((prev) => {
              if (prev.phase !== "recording") return prev;
              return {
                ...prev,
                frameCount: current.session!.frame_count,
                elapsed,
                approxMemoryBytes: current.session!.approx_memory_bytes,
              };
            });
            if (isLiveRef.current && current.session.frame_count > 0) {
              const frameIndex = current.session.frame_count - 1;
              const frame = await apiClient.fetchRecordingFrame(frameIndex);
              const converted = convertSnapshot(frame);
              setSnap({ phase: "ready", ...converted });
            }
          } catch (e) {
            console.error(e);
          }
        })();
      }, 1000);
    } catch (err) {
      console.error(err);
    }
  }, []);

  const handleStopRecording = useCallback(async () => {
    if (pollingRef.current !== null) {
      window.clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
    try {
      const session = await apiClient.stopRecording();
      const autoInterval =
        session.frame_count > 500 ? 5 : session.frame_count >= 100 ? 2 : 1;
      setDownsampleInterval(autoInterval);
      setBuiltDownsampleInterval(autoInterval);
      setRecording({
        phase: "stopped",
        sessionId: session.session_id,
        frameCount: session.frame_count,
        frames: session.frames,
        unionLayout: null,
        buildingUnion: true,
        buildProgress: [0, session.frame_count],
        avgCaptureMs: session.avg_capture_ms,
        maxCaptureMs: session.max_capture_ms,
        totalCaptureMs: session.total_capture_ms,
      });
      if (session.frame_count > 0) {
        // Show last frame while union builds.
        const lastFrameIndex = session.frame_count - 1;
        const lastFrame = await apiClient.fetchRecordingFrame(lastFrameIndex);
        const converted = convertSnapshot(lastFrame);
        setSnap({ phase: "ready", ...converted });

        // Build union layout.
        const union = await buildUnionLayout(
          session.frames,
          apiClient,
          renderNodeForMeasure,
          (loaded, total) => {
            setRecording((prev) => {
              if (prev.phase !== "stopped") return prev;
              return { ...prev, buildProgress: [loaded, total] };
            });
          },
          autoInterval,
        );
        setRecording((prev) => {
          if (prev.phase !== "stopped") return prev;
          return { ...prev, unionLayout: union, buildingUnion: false };
        });

        // Render last frame from union layout.
        const snappedLast = nearestProcessedFrame(lastFrameIndex, union.processedFrameIndices);
        const unionFrame = renderFrameFromUnion(
          snappedLast,
          union,
          hiddenKrates,
          hiddenProcesses,
          focusedEntityId,
          ghostMode,
        );
        setUnionFrameLayout(unionFrame);
      }
    } catch (err) {
      console.error(err);
    }
  }, [hiddenKrates, hiddenProcesses, focusedEntityId]);

  const handleExport = useCallback(async () => {
    try {
      const blob = await apiClient.exportRecording();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      const sessionId =
        recording.phase === "stopped" || recording.phase === "scrubbing"
          ? recording.sessionId.replace(/:/g, "_")
          : "recording";
      a.href = url;
      a.download = `recording-${sessionId}.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error(err);
    }
  }, [recording]);

  const handleImportFile = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (!file) return;
      e.target.value = "";
      try {
        const session = await apiClient.importRecording(file);
        const autoInterval =
          session.frame_count > 500 ? 5 : session.frame_count >= 100 ? 2 : 1;
        setDownsampleInterval(autoInterval);
        setBuiltDownsampleInterval(autoInterval);
        setRecording({
          phase: "stopped",
          sessionId: session.session_id,
          frameCount: session.frame_count,
          frames: session.frames,
          unionLayout: null,
          buildingUnion: true,
          buildProgress: [0, session.frame_count],
          avgCaptureMs: session.avg_capture_ms,
          maxCaptureMs: session.max_capture_ms,
          totalCaptureMs: session.total_capture_ms,
        });
        if (session.frames.length > 0) {
          const lastFrameIndex = session.frames[session.frames.length - 1].frame_index;
          const lastFrame = await apiClient.fetchRecordingFrame(lastFrameIndex);
          const converted = convertSnapshot(lastFrame);
          setSnap({ phase: "ready", ...converted });

          const union = await buildUnionLayout(
            session.frames,
            apiClient,
            renderNodeForMeasure,
            (loaded, total) => {
              setRecording((prev) => {
                if (prev.phase !== "stopped") return prev;
                return { ...prev, buildProgress: [loaded, total] };
              });
            },
            autoInterval,
          );
          setRecording((prev) => {
            if (prev.phase !== "stopped") return prev;
            return { ...prev, unionLayout: union, buildingUnion: false };
          });

          const snappedLast = nearestProcessedFrame(lastFrameIndex, union.processedFrameIndices);
          const unionFrame = renderFrameFromUnion(
            snappedLast,
            union,
            hiddenKrates,
            hiddenProcesses,
            focusedEntityId,
          );
          setUnionFrameLayout(unionFrame);
        }
      } catch (err) {
        console.error(err);
      }
    },
    [hiddenKrates, hiddenProcesses, focusedEntityId],
  );

  const handleScrub = useCallback(
    (frameIndex: number) => {
      setRecording((prev) => {
        if (prev.phase !== "stopped" && prev.phase !== "scrubbing") return prev;
        const { frames, frameCount, sessionId, avgCaptureMs, maxCaptureMs, totalCaptureMs } = prev;
        // Both "stopped" and "scrubbing" have unionLayout. Narrow for TS:
        const unionLayout =
          prev.phase === "stopped" ? prev.unionLayout : prev.unionLayout;
        if (!unionLayout) return prev; // union not ready yet

        // Render the frame from the union layout.
        const result = renderFrameFromUnion(
          frameIndex,
          unionLayout,
          hiddenKrates,
          hiddenProcesses,
          focusedEntityId,
          ghostMode,
        );
        setUnionFrameLayout(result);

        // Update snapshot state with this frame's entities/edges for the inspector.
        const snapped = nearestProcessedFrame(frameIndex, unionLayout.processedFrameIndices);
        const frameData = unionLayout.frameCache.get(snapped);
        if (frameData) {
          setSnap({ phase: "ready", entities: frameData.entities, edges: frameData.edges });
        }

        return {
          phase: "scrubbing" as const,
          sessionId,
          frameCount,
          frames,
          currentFrameIndex: frameIndex,
          unionLayout,
          avgCaptureMs,
          maxCaptureMs,
          totalCaptureMs,
        };
      });
    },
    [hiddenKrates, hiddenProcesses, focusedEntityId, ghostMode],
  );

  const handleRebuildUnion = useCallback(async () => {
    if (recording.phase !== "stopped" && recording.phase !== "scrubbing") return;
    const { frames, sessionId, frameCount, avgCaptureMs, maxCaptureMs, totalCaptureMs } =
      recording;
    setRecording({
      phase: "stopped",
      sessionId,
      frameCount,
      frames,
      unionLayout: null,
      buildingUnion: true,
      buildProgress: [0, frames.length],
      avgCaptureMs,
      maxCaptureMs,
      totalCaptureMs,
    });
    try {
      const union = await buildUnionLayout(
        frames,
        apiClient,
        renderNodeForMeasure,
        (loaded, total) => {
          setRecording((prev) => {
            if (prev.phase !== "stopped") return prev;
            return { ...prev, buildProgress: [loaded, total] };
          });
        },
        downsampleInterval,
      );
      setBuiltDownsampleInterval(downsampleInterval);
      setRecording((prev) => {
        if (prev.phase !== "stopped") return prev;
        return { ...prev, unionLayout: union, buildingUnion: false };
      });
      const lastFrameIdx = frames[frames.length - 1]?.frame_index ?? 0;
      const snapped = nearestProcessedFrame(lastFrameIdx, union.processedFrameIndices);
      const unionFrame = renderFrameFromUnion(
        snapped,
        union,
        hiddenKrates,
        hiddenProcesses,
        focusedEntityId,
        ghostMode,
      );
      setUnionFrameLayout(unionFrame);
      const frameData = union.frameCache.get(snapped);
      if (frameData) {
        setSnap({ phase: "ready", entities: frameData.entities, edges: frameData.edges });
      }
    } catch (err) {
      console.error(err);
    }
  }, [recording, downsampleInterval, hiddenKrates, hiddenProcesses, focusedEntityId, ghostMode]);

  // Re-render union frame when filters change during playback.
  useEffect(() => {
    if (recording.phase === "scrubbing") {
      const result = renderFrameFromUnion(
        recording.currentFrameIndex,
        recording.unionLayout,
        hiddenKrates,
        hiddenProcesses,
        focusedEntityId,
        ghostMode,
      );
      setUnionFrameLayout(result);
    } else if (recording.phase === "stopped" && recording.unionLayout) {
      const lastFrame = recording.frames.length - 1;
      const result = renderFrameFromUnion(
        recording.frames[lastFrame]?.frame_index ?? 0,
        recording.unionLayout,
        hiddenKrates,
        hiddenProcesses,
        focusedEntityId,
        ghostMode,
      );
      setUnionFrameLayout(result);
    }
  }, [hiddenKrates, hiddenProcesses, focusedEntityId, ghostMode, recording]);

  // Clear union frame layout when going back to idle or starting a new recording.
  useEffect(() => {
    if (recording.phase === "idle" || recording.phase === "recording") {
      setUnionFrameLayout(undefined);
    }
  }, [recording.phase]);

  useEffect(() => {
    let cancelled = false;
    async function poll() {
      while (!cancelled) {
        try {
          const conns = await apiClient.fetchConnections();
          if (cancelled) break;
          setConnections(conns);
          if (conns.connected_processes > 0) {
            takeSnapshot();
            break;
          }
        } catch (e) {
          console.error(e);
        }
        await new Promise<void>((resolve) => setTimeout(resolve, 2000));
      }
    }
    poll();
    return () => {
      cancelled = true;
    };
  }, [takeSnapshot]);

  useEffect(() => {
    isLiveRef.current = isLive;
  }, [isLive]);

  useEffect(() => {
    return () => {
      if (pollingRef.current !== null) {
        window.clearInterval(pollingRef.current);
      }
    };
  }, []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && focusedEntityId) {
        setFocusedEntityId(null);
      } else if (e.key === "f" || e.key === "F") {
        if (selection?.kind === "entity") {
          setFocusedEntityId(selection.id);
        }
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [focusedEntityId, selection]);

  const isBusy = snap.phase === "cutting" || snap.phase === "loading";

  const buttonLabel =
    snap.phase === "cutting" ? "Syncing…" : snap.phase === "loading" ? "Loading…" : "Take Snapshot";

  const connCount = connections?.connected_processes ?? 0;
  const waitingForProcesses = connCount === 0 && snap.phase === "idle";

  const currentFrameIndex =
    recording.phase === "scrubbing"
      ? recording.currentFrameIndex
      : recording.phase === "stopped"
        ? recording.frames.length - 1
        : 0;

  const unionLayoutForDerived =
    (recording.phase === "stopped" || recording.phase === "scrubbing") && recording.unionLayout
      ? recording.unionLayout
      : null;

  const snappedFrameIndex = unionLayoutForDerived
    ? nearestProcessedFrame(currentFrameIndex, unionLayoutForDerived.processedFrameIndices)
    : currentFrameIndex;

  const processedFrameCount = unionLayoutForDerived?.processedFrameIndices.length;

  const changeSummaries = useMemo<Map<number, FrameChangeSummary> | null>(() => {
    return unionLayoutForDerived ? computeChangeSummaries(unionLayoutForDerived) : null;
  }, [recording]); // eslint-disable-line react-hooks/exhaustive-deps

  const changeFrames = useMemo<number[] | null>(() => {
    return unionLayoutForDerived ? computeChangeFrames(unionLayoutForDerived) : null;
  }, [recording]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (recording.phase !== "stopped" && recording.phase !== "scrubbing") return;
    function onKey(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      if (e.key === "[" && changeFrames) {
        const prev = changeFrames.filter((f) => f < currentFrameIndex).at(-1);
        if (prev !== undefined) handleScrub(prev);
      } else if (e.key === "]" && changeFrames) {
        const next = changeFrames.find((f) => f > currentFrameIndex);
        if (next !== undefined) handleScrub(next);
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [recording.phase, changeFrames, currentFrameIndex, handleScrub]);

  return (
    <div className="mockup-app">
      {showProcessModal && connections && (
        <ProcessModal connections={connections} onClose={() => setShowProcessModal(false)} />
      )}
      <div className="mockup-header">
        <Aperture size={16} weight="bold" />
        <span className="mockup-header-title">peeps</span>
        <button
          type="button"
          className={`mockup-proc-pill${connCount > 0 ? " mockup-proc-pill--connected" : " mockup-proc-pill--disconnected"}`}
          onClick={() => setShowProcessModal(true)}
          title="Click to see connected processes"
        >
          {waitingForProcesses ? (
            <>
              <CircleNotch size={11} weight="bold" className="spinning" /> waiting…
            </>
          ) : (
            <>
              {connCount} {connCount === 1 ? "process" : "processes"}
            </>
          )}
        </button>
        {apiMode === "lab" ? (
          <span className="mockup-header-badge">mock data</span>
        ) : snap.phase === "ready" ? (
          <span className="mockup-header-badge mockup-header-badge--active">
            <CheckCircle size={12} weight="bold" />
            snapshot
          </span>
        ) : null}
        {snap.phase === "error" && <span className="mockup-header-error">{snap.message}</span>}
        <span className="mockup-header-spacer" />
        {recording.phase === "recording" && (
          <span
            className={[
              "mockup-header-badge",
              recording.approxMemoryBytes >= recording.maxMemoryBytes * 0.75
                ? "mockup-header-badge--recording-warn"
                : "mockup-header-badge--recording",
            ].join(" ")}
          >
            <span className="recording-dot" />
            {formatElapsed(recording.elapsed)} · {recording.frameCount} frames ·{" "}
            {formatBytes(recording.approxMemoryBytes)}
          </span>
        )}
        {recording.phase === "recording" && (
          <ActionButton
            variant={isLive ? "primary" : "default"}
            onPress={() => setIsLive((v) => !v)}
          >
            Live
          </ActionButton>
        )}
        {(recording.phase === "stopped" || recording.phase === "scrubbing") && (
          <>
            <ActionButton variant="default" onPress={handleExport}>
              <DownloadSimple size={14} weight="bold" />
              Export
            </ActionButton>
            <ActionButton variant="default" onPress={() => fileInputRef.current?.click()}>
              <UploadSimple size={14} weight="bold" />
              Import
            </ActionButton>
          </>
        )}
        {recording.phase === "idle" ||
        recording.phase === "stopped" ||
        recording.phase === "scrubbing" ? (
          <ActionButton
            variant="default"
            onPress={handleStartRecording}
            isDisabled={isBusy || connCount === 0}
          >
            <Record size={14} weight="fill" />
            Record
          </ActionButton>
        ) : (
          <ActionButton variant="default" onPress={handleStopRecording}>
            <Stop size={14} weight="fill" />
            Stop
          </ActionButton>
        )}
        <ActionButton variant="primary" onPress={takeSnapshot} isDisabled={isBusy}>
          {isBusy ? <CircleNotch size={14} weight="bold" /> : <Camera size={14} weight="bold" />}
          {buttonLabel}
        </ActionButton>
        <input
          ref={fileInputRef}
          type="file"
          accept=".json,application/json"
          style={{ display: "none" }}
          onChange={handleImportFile}
        />
      </div>
      {(recording.phase === "stopped" || recording.phase === "scrubbing") &&
        recording.frames.length > 0 && (
          <RecordingTimeline
            frames={recording.frames}
            frameCount={recording.frameCount}
            currentFrameIndex={currentFrameIndex}
            onScrub={handleScrub}
            buildingUnion={recording.phase === "stopped" && recording.buildingUnion}
            buildProgress={recording.phase === "stopped" ? recording.buildProgress : undefined}
            changeSummary={changeSummaries?.get(snappedFrameIndex)}
            changeFrames={changeFrames ?? undefined}
            avgCaptureMs={recording.avgCaptureMs}
            maxCaptureMs={recording.maxCaptureMs}
            totalCaptureMs={recording.totalCaptureMs}
            ghostMode={ghostMode}
            onGhostToggle={() => setGhostMode((v) => !v)}
            processedFrameCount={processedFrameCount}
            downsampleInterval={downsampleInterval}
            onDownsampleChange={setDownsampleInterval}
            canRebuild={downsampleInterval !== builtDownsampleInterval}
            onRebuild={handleRebuildUnion}
          />
        )}
      <SplitLayout
        left={
          <GraphPanel
            entityDefs={entities}
            edgeDefs={edges}
            snapPhase={snap.phase}
            selection={selection}
            onSelect={setSelection}
            focusedEntityId={focusedEntityId}
            onExitFocus={() => setFocusedEntityId(null)}
            waitingForProcesses={waitingForProcesses}
            crateItems={crateItems}
            hiddenKrates={hiddenKrates}
            onKrateToggle={handleKrateToggle}
            onKrateSolo={handleKrateSolo}
            processItems={processItems}
            hiddenProcesses={hiddenProcesses}
            onProcessToggle={handleProcessToggle}
            onProcessSolo={handleProcessSolo}
            scopeColorMode={scopeColorMode}
            onToggleProcessColorBy={handleToggleProcessColorBy}
            onToggleCrateColorBy={handleToggleCrateColorBy}
            useSubgraphs={useSubgraphs}
            onToggleProcessSubgraphs={handleToggleProcessSubgraphs}
            onToggleCrateSubgraphs={handleToggleCrateSubgraphs}
            unionFrameLayout={unionFrameLayout}
          />
        }
        right={
          <InspectorPanel
            collapsed={inspectorCollapsed}
            onToggleCollapse={() => setInspectorCollapsed((v) => !v)}
            selection={selection}
            entityDefs={allEntities}
            edgeDefs={allEdges}
            onFocusEntity={setFocusedEntityId}
            scrubbingUnionLayout={recording.phase === "scrubbing" ? recording.unionLayout : undefined}
            currentFrameIndex={recording.phase === "scrubbing" ? recording.currentFrameIndex : undefined}
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
