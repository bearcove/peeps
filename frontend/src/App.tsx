import React, { useCallback, useEffect, useMemo, useState } from "react";
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
  MarkerType,
  type Node,
  type Edge,
  type EdgeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import ELK from "elkjs/lib/elk-api.js";
import elkWorkerUrl from "elkjs/lib/elk-worker.min.js?url";
import {
  ArrowLineDown,
  ArrowLineUp,
  CaretDown,
  CaretLeft,
  CaretRight,
  Camera,
  Aperture,
  CheckCircle,
  CircleNotch,
  CopySimple,
  FileRs,
  LinkSimple,
  MagnifyingGlass,
  PaperPlaneTilt,
  Timer,
  Crosshair,
} from "@phosphor-icons/react";
import { SplitLayout } from "./ui/layout/SplitLayout";
import { Badge } from "./ui/primitives/Badge";
import { KeyValueRow } from "./ui/primitives/KeyValueRow";
import { DurationDisplay } from "./ui/primitives/DurationDisplay";
import { ActionButton } from "./ui/primitives/ActionButton";
import { kindIcon, kindDisplayName } from "./nodeKindSpec";
import { apiClient, apiMode } from "./api";
import type { ConnectedProcessInfo, ConnectionsResponse, EntityBody, SnapshotEdgeKind, SnapshotCutResponse } from "./api/types";
import { Table, type Column } from "./ui/primitives/Table";

// ── Body type helpers ──────────────────────────────────────────

// TypeScript's `in` narrowing on complex union types produces `unknown` for
// nested property types. Use `Extract` to safely reference specific variants.
type RequestBody = Extract<EntityBody, { request: unknown }>;
type ResponseBody = Extract<EntityBody, { response: unknown }>;

// ── Display types ──────────────────────────────────────────────

type Tone = "ok" | "warn" | "crit" | "neutral";

type MetaValue = string | number | boolean | null | MetaValue[] | { [key: string]: MetaValue };

export type EntityDef = {
  /** Composite identity: "${processId}/${rawEntityId}". Unique across all processes. */
  id: string;
  /** Original entity ID as reported by the process. */
  rawEntityId: string;
  processId: string;
  processName: string;
  name: string;
  kind: string;
  body: EntityBody;
  source: string;
  /** Process-relative birth time in ms (PTime). Not comparable across processes. */
  birthPtime: number;
  /** Age at capture time: ptime_now_ms - birthPtime (clamped to 0). */
  ageMs: number;
  /** Approximate wall-clock birth: (captured_at_unix_ms - ptime_now_ms) + birthPtime. */
  birthApproxUnixMs: number;
  meta: Record<string, MetaValue>;
  inCycle: boolean;
  status: { label: string; tone: Tone };
  stat?: string;
  statTone?: Tone;
  /** Present when this is a merged TX/RX channel pair node. */
  channelPair?: { tx: EntityDef; rx: EntityDef };
};

export type EdgeDef = {
  id: string;
  source: string;
  target: string;
  kind: SnapshotEdgeKind;
  /** ELK port ID on the source node, when the source is a merged channel pair. */
  sourcePort?: string;
  /** ELK port ID on the target node, when the target is a merged channel pair. */
  targetPort?: string;
};

// ── Snapshot conversion ────────────────────────────────────────

function bodyToKind(body: EntityBody): string {
  return typeof body === "string" ? body : Object.keys(body)[0];
}

function deriveStatus(body: EntityBody): { label: string; tone: Tone } {
  if (typeof body === "string") return { label: "polling", tone: "neutral" };
  if ("request" in body) return { label: "in_flight", tone: "warn" };
  if ("response" in body) {
    const s = (body as ResponseBody).response.status;
    if (s === "ok") return { label: "ok", tone: "ok" };
    if (s === "error") return { label: "error", tone: "crit" };
    if (s === "cancelled") return { label: "cancelled", tone: "neutral" };
    return { label: "pending", tone: "warn" };
  }
  if ("lock" in body) return { label: "held", tone: "crit" };
  if ("channel_tx" in body || "channel_rx" in body) {
    const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
    return ep.lifecycle === "open"
      ? { label: "open", tone: "ok" }
      : { label: "closed", tone: "neutral" };
  }
  if ("semaphore" in body) {
    const { max_permits, handed_out_permits } = body.semaphore;
    const available = max_permits - handed_out_permits;
    return { label: `${available}/${max_permits} permits`, tone: handed_out_permits > 0 ? "warn" : "ok" };
  }
  if ("notify" in body) return { label: "waiting", tone: "neutral" };
  if ("once_cell" in body) {
    const s = body.once_cell.state;
    if (s === "initialized") return { label: "initialized", tone: "ok" };
    if (s === "initializing") return { label: "initializing", tone: "warn" };
    return { label: "empty", tone: "neutral" };
  }
  if ("command" in body) return { label: "running", tone: "neutral" };
  if ("file_op" in body) return { label: body.file_op.op, tone: "ok" };
  if ("net_connect" in body || "net_accept" in body || "net_read" in body || "net_write" in body) {
    return { label: "connected", tone: "ok" };
  }
  return { label: "unknown", tone: "neutral" };
}

function deriveStat(body: EntityBody): string | undefined {
  if (typeof body === "string") return undefined;
  if ("semaphore" in body) {
    const { max_permits, handed_out_permits } = body.semaphore;
    return `${max_permits - handed_out_permits}/${max_permits}`;
  }
  if ("channel_tx" in body || "channel_rx" in body) {
    const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
    if ("mpsc" in ep.details && ep.details.mpsc.buffer) {
      const { occupancy, capacity } = ep.details.mpsc.buffer;
      return `${occupancy}/${capacity ?? "∞"}`;
    }
  }
  if ("notify" in body) {
    return body.notify.waiter_count > 0 ? `${body.notify.waiter_count} waiters` : undefined;
  }
  if ("once_cell" in body) {
    return body.once_cell.waiter_count > 0 ? `${body.once_cell.waiter_count} waiter` : undefined;
  }
  return undefined;
}

function deriveStatTone(body: EntityBody): Tone | undefined {
  if (typeof body === "string") return undefined;
  if ("channel_tx" in body || "channel_rx" in body) {
    const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
    if ("mpsc" in ep.details && ep.details.mpsc.buffer) {
      const { occupancy, capacity } = ep.details.mpsc.buffer;
      if (capacity == null) return undefined;
      if (occupancy >= capacity) return "crit";
      if (occupancy / capacity >= 0.75) return "warn";
    }
  }
  return undefined;
}

function detectCycleNodes(entities: EntityDef[], edges: EdgeDef[]): Set<string> {
  const adj = new Map<string, string[]>();
  for (const e of edges) {
    if (e.kind !== "needs") continue;
    if (!adj.has(e.source)) adj.set(e.source, []);
    adj.get(e.source)!.push(e.target);
  }

  const inCycle = new Set<string>();
  const color = new Map<string, "gray" | "black">();

  function dfs(id: string, stack: string[]) {
    color.set(id, "gray");
    stack.push(id);
    for (const neighbor of adj.get(id) ?? []) {
      if (color.get(neighbor) === "gray") {
        const start = stack.indexOf(neighbor);
        for (const n of stack.slice(start)) inCycle.add(n);
      } else if (!color.has(neighbor)) {
        dfs(neighbor, stack);
      }
    }
    stack.pop();
    color.set(id, "black");
  }

  for (const entity of entities) {
    if (!color.has(entity.id)) dfs(entity.id, []);
  }
  return inCycle;
}

function mergeChannelPairs(
  entities: EntityDef[],
  edges: EdgeDef[],
): { entities: EntityDef[]; edges: EdgeDef[] } {
  const channelLinks = edges.filter((e) => e.kind === "channel_link");
  const entityById = new Map(entities.map((e) => [e.id, e]));

  // Maps from original TX/RX entity id → merged id and port id
  const mergedIdFor = new Map<string, string>();
  const portIdFor = new Map<string, string>();
  const removedIds = new Set<string>();
  const mergedEntities: EntityDef[] = [];

  for (const link of channelLinks) {
    const txEntity = entityById.get(link.source);
    const rxEntity = entityById.get(link.target);
    if (!txEntity || !rxEntity) continue;
    // Guard against a TX or RX being part of multiple links (shouldn't happen)
    if (mergedIdFor.has(link.source) || mergedIdFor.has(link.target)) continue;

    const mergedId = `pair:${link.source}:${link.target}`;
    const txPortId = `${mergedId}:tx`;
    const rxPortId = `${mergedId}:rx`;

    mergedIdFor.set(link.source, mergedId);
    mergedIdFor.set(link.target, mergedId);
    portIdFor.set(link.source, txPortId);
    portIdFor.set(link.target, rxPortId);
    removedIds.add(link.source);
    removedIds.add(link.target);

    const channelName = txEntity.name.endsWith(":tx")
      ? txEntity.name.slice(0, -3)
      : txEntity.name;

    const mergedStatus =
      txEntity.status.tone === "ok" && rxEntity.status.tone === "ok"
        ? ({ label: "open", tone: "ok" } as const)
        : ({ label: "closed", tone: "neutral" } as const);

    mergedEntities.push({
      ...txEntity,
      id: mergedId,
      name: channelName,
      kind: "channel_pair",
      status: mergedStatus,
      stat: txEntity.stat,
      statTone: txEntity.statTone,
      inCycle: false, // set later by detectCycleNodes
      channelPair: { tx: txEntity, rx: rxEntity },
    });
  }

  const filteredEntities = entities.filter((e) => !removedIds.has(e.id));
  const newEntities = [...filteredEntities, ...mergedEntities];

  // Remove channel_link edges; remap sources/targets that pointed at TX/RX entities
  const newEdges = edges
    .filter((e) => e.kind !== "channel_link")
    .map((e) => {
      const origSource = e.source;
      const origTarget = e.target;
      const newSource = mergedIdFor.get(origSource) ?? origSource;
      const newTarget = mergedIdFor.get(origTarget) ?? origTarget;
      const sourcePort = mergedIdFor.has(origSource) ? portIdFor.get(origSource) : undefined;
      const targetPort = mergedIdFor.has(origTarget) ? portIdFor.get(origTarget) : undefined;
      if (newSource === origSource && newTarget === origTarget) return e;
      return { ...e, source: newSource, target: newTarget, sourcePort, targetPort };
    });

  return { entities: newEntities, edges: newEdges };
}

function convertSnapshot(snapshot: SnapshotCutResponse): { entities: EntityDef[]; edges: EdgeDef[] } {
  const allEntities: EntityDef[] = [];
  const allEdges: EdgeDef[] = [];

  for (const proc of snapshot.processes) {
    const { process_id, process_name, ptime_now_ms } = proc;
    const anchorUnixMs = snapshot.captured_at_unix_ms - ptime_now_ms;

    for (const e of proc.snapshot.entities) {
      const compositeId = `${process_id}/${e.id}`;
      const ageMs = Math.max(0, ptime_now_ms - e.birth);
      allEntities.push({
        id: compositeId,
        rawEntityId: e.id,
        processId: String(process_id),
        processName: process_name,
        name: e.name,
        kind: bodyToKind(e.body),
        body: e.body,
        source: e.source,
        birthPtime: e.birth,
        ageMs,
        birthApproxUnixMs: anchorUnixMs + e.birth,
        meta: (e.meta ?? {}) as Record<string, MetaValue>,
        inCycle: false,
        status: deriveStatus(e.body),
        stat: deriveStat(e.body),
        statTone: deriveStatTone(e.body),
      });
    }

    for (let i = 0; i < proc.snapshot.edges.length; i++) {
      const e = proc.snapshot.edges[i];
      const srcComposite = `${process_id}/${e.src}`;
      const dstComposite = `${process_id}/${e.dst}`;
      allEdges.push({
        id: `e${i}-${srcComposite}-${dstComposite}-${e.kind}`,
        source: srcComposite,
        target: dstComposite,
        kind: e.kind,
      });
    }
  }

  const { entities: mergedEntities, edges: mergedEdges } = mergeChannelPairs(allEntities, allEdges);

  const cycleIds = detectCycleNodes(mergedEntities, mergedEdges);
  for (const entity of mergedEntities) {
    entity.inCycle = cycleIds.has(entity.id);
  }

  return { entities: mergedEntities, edges: mergedEdges };
}

// ── ELK layout ────────────────────────────────────────────────

const elk = new ELK({ workerUrl: elkWorkerUrl });

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "24",
  "elk.layered.spacing.nodeNodeBetweenLayers": "48",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
  "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
};

function measureNodeDefs(defs: EntityDef[]): Map<string, { width: number; height: number }> {
  const container = document.createElement("div");
  container.style.cssText = "position:fixed;top:-9999px;left:-9999px;visibility:hidden;pointer-events:none;display:flex;flex-direction:column;align-items:flex-start;gap:4px;";
  document.body.appendChild(container);

  const elements: { id: string; el: HTMLDivElement }[] = [];
  for (const def of defs) {
    if (def.channelPair) {
      const el = document.createElement("div");
      el.className = [
        "mockup-channel-pair",
        def.statTone === "crit" && "mockup-channel-pair--stat-crit",
        def.statTone === "warn" && "mockup-channel-pair--stat-warn",
      ].filter(Boolean).join(" ");

      const header = document.createElement("div");
      header.className = "mockup-channel-pair-header";
      const nameEl = document.createElement("span");
      nameEl.className = "mockup-channel-pair-name";
      nameEl.textContent = def.name;
      header.appendChild(nameEl);
      el.appendChild(header);

      const rows = document.createElement("div");
      rows.className = "mockup-channel-pair-rows";
      for (const [rowLabel, rowDef] of [["TX", def.channelPair.tx], ["RX", def.channelPair.rx]] as const) {
        const row = document.createElement("div");
        row.className = "mockup-channel-pair-row";
        const lbl = document.createElement("span");
        lbl.className = "mockup-channel-pair-row-label";
        lbl.textContent = rowLabel;
        row.appendChild(lbl);
        const badge = document.createElement("span");
        badge.className = `badge badge--${rowDef.status.tone}`;
        badge.textContent = rowDef.status.label;
        row.appendChild(badge);
        const dot = document.createElement("span");
        dot.className = "mockup-node-dot";
        dot.textContent = "·";
        row.appendChild(dot);
        const dur = document.createElement("span");
        dur.className = "ui-duration-display";
        dur.textContent = "00m00s";
        row.appendChild(dur);
        if (rowLabel === "TX" && def.stat) {
          const dot2 = document.createElement("span");
          dot2.className = "mockup-node-dot";
          dot2.textContent = "·";
          row.appendChild(dot2);
          const statEl = document.createElement("span");
          statEl.className = "mockup-node-stat";
          statEl.textContent = def.stat;
          row.appendChild(statEl);
        }
        rows.appendChild(row);
      }
      el.appendChild(rows);
      container.appendChild(el);
      elements.push({ id: def.id, el });
      continue;
    }

    const el = document.createElement("div");
    el.className = [
      "mockup-node",
      def.inCycle && "mockup-node--cycle",
      def.statTone === "crit" && "mockup-node--stat-crit",
      def.statTone === "warn" && "mockup-node--stat-warn",
    ].filter(Boolean).join(" ");

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
      statEl.className = [
        "mockup-node-stat",
        def.statTone === "crit" && "mockup-node-stat--crit",
        def.statTone === "warn" && "mockup-node-stat--warn",
      ].filter(Boolean).join(" ");
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

function edgeStyle(kind: EdgeDef["kind"]) {
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

function edgeTooltip(kind: EdgeDef["kind"], sourceName: string, targetName: string): string {
  switch (kind) {
    case "needs": return `${sourceName} is blocked waiting for ${targetName}`;
    case "polls": return `${sourceName} polls ${targetName} (non-blocking)`;
    case "closed_by": return `${sourceName} was closed by ${targetName}`;
    case "channel_link": return `Channel endpoint: ${sourceName} → ${targetName}`;
    case "rpc_link": return `RPC pair: ${sourceName} → ${targetName}`;
  }
}

function edgeMarkerSize(kind: EdgeDef["kind"]): number {
  return kind === "needs" ? 12 : 8;
}

type ElkPoint = { x: number; y: number };
type LayoutResult = { nodes: Node[]; edges: Edge[] };

async function layoutGraph(
  entityDefs: EntityDef[],
  edgeDefs: EdgeDef[],
  nodeSizes: Map<string, { width: number; height: number }>,
): Promise<LayoutResult> {
  const nodeIds = new Set(entityDefs.map((n) => n.id));
  const validEdges = edgeDefs.filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target));

  const result = await elk.layout({
    id: "root",
    layoutOptions: elkOptions,
    children: entityDefs.map((n) => {
      const sz = nodeSizes.get(n.id);
      const base = { id: n.id, width: sz?.width || 150, height: sz?.height || 36 };
      return base;
    }),
    edges: validEdges.map((e) => ({
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

  const nodes: Node[] = entityDefs.map((def) => {
    const position = posMap.get(def.id) ?? { x: 0, y: 0 };
    if (def.channelPair) {
      return {
        id: def.id,
        type: "channelPairNode",
        position,
        data: {
          tx: def.channelPair.tx,
          rx: def.channelPair.rx,
          channelName: def.name,
          selected: false,
          statTone: def.statTone,
        },
      };
    }
    return {
      id: def.id,
      type: "mockNode",
      position,
      data: {
        kind: def.kind,
        label: def.name,
        inCycle: def.inCycle,
        selected: false,
        status: def.status,
        ageMs: def.ageMs,
        stat: def.stat,
        statTone: def.statTone,
      },
    };
  });

  const entityNameMap = new Map(entityDefs.map((e) => [e.id, e.name]));
  const edges: Edge[] = validEdges.map((def) => {
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

// ── Custom node component ──────────────────────────────────────

const hiddenHandle: React.CSSProperties = {
  opacity: 0, width: 0, height: 0, minWidth: 0, minHeight: 0,
  position: "absolute", top: "50%", left: "50%", pointerEvents: "none",
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
};

function MockNodeComponent({ data }: { data: MockNodeData }) {
  return (
    <>
      <Handle type="target" position={Position.Top} style={hiddenHandle} />
      <Handle type="source" position={Position.Bottom} style={hiddenHandle} />
      <div className={[
        "mockup-node",
        data.inCycle && "mockup-node--cycle",
        data.selected && "mockup-node--selected",
        data.statTone === "crit" && "mockup-node--stat-crit",
        data.statTone === "warn" && "mockup-node--stat-warn",
      ].filter(Boolean).join(" ")}>
        <span className="mockup-node-icon">{kindIcon(data.kind, 18)}</span>
        <div className="mockup-node-content">
          <div className="mockup-node-main">
            <span className="mockup-node-label">{data.label}</span>
          </div>
          <div className="mockup-node-details">
            <Badge tone={data.status.tone}>{data.status.label}</Badge>
            <span className="mockup-node-dot">&middot;</span>
            <DurationDisplay ms={data.ageMs} />
            {data.stat && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <span className={[
                  "mockup-node-stat",
                  data.statTone === "crit" && "mockup-node-stat--crit",
                  data.statTone === "warn" && "mockup-node-stat--warn",
                ].filter(Boolean).join(" ")}>{data.stat}</span>
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
};

const visibleHandleTop: React.CSSProperties = {
  width: 10, height: 6, minWidth: 0, minHeight: 0,
  background: "var(--text-tertiary)",
  border: "none", borderRadius: "0 0 3px 3px",
  opacity: 0.5, top: 0, left: "50%",
  transform: "translateX(-50%)",
  pointerEvents: "none",
};

const visibleHandleBottom: React.CSSProperties = {
  width: 10, height: 6, minWidth: 0, minHeight: 0,
  background: "var(--text-tertiary)",
  border: "none", borderRadius: "3px 3px 0 0",
  opacity: 0.5, bottom: 0, left: "50%",
  transform: "translateX(-50%)",
  pointerEvents: "none",
};

function ChannelPairNode({ data }: { data: ChannelPairNodeData }) {
  const { tx, rx, channelName, selected, statTone } = data;
  const txEp = typeof tx.body !== "string" && "channel_tx" in tx.body ? tx.body.channel_tx : null;
  const rxEp = typeof rx.body !== "string" && "channel_rx" in rx.body ? rx.body.channel_rx : null;

  const mpscBuffer = txEp && "mpsc" in txEp.details ? txEp.details.mpsc.buffer : null;

  const txLifecycle = txEp ? (txEp.lifecycle === "open" ? "open" : "closed") : "?";
  const rxLifecycle = rxEp ? (rxEp.lifecycle === "open" ? "open" : "closed") : "?";
  const txTone: Tone = txLifecycle === "open" ? "ok" : "neutral";
  const rxTone: Tone = rxLifecycle === "open" ? "ok" : "neutral";

  const bufferStat = mpscBuffer
    ? `${mpscBuffer.occupancy}/${mpscBuffer.capacity ?? "∞"}`
    : tx.stat;

  return (
    <>
      <Handle type="target" position={Position.Top} style={visibleHandleTop} />
      <Handle type="source" position={Position.Bottom} style={visibleHandleBottom} />
      <div className={[
        "mockup-channel-pair",
        selected && "mockup-channel-pair--selected",
        statTone === "crit" && "mockup-channel-pair--stat-crit",
        statTone === "warn" && "mockup-channel-pair--stat-warn",
      ].filter(Boolean).join(" ")}>
        <div className="mockup-channel-pair-header">
          <span className="mockup-channel-pair-icon">
            <ArrowLineUp size={9} weight="bold" />
            <ArrowLineDown size={9} weight="bold" />
          </span>
          <span className="mockup-channel-pair-name">{channelName}</span>
        </div>
        <div className="mockup-channel-pair-rows">
          <div className="mockup-channel-pair-row">
            <span className="mockup-channel-pair-row-label">TX</span>
            <Badge tone={txTone}>{txLifecycle}</Badge>
            <span className="mockup-node-dot">&middot;</span>
            <DurationDisplay ms={tx.ageMs} />
            {bufferStat && (
              <>
                <span className="mockup-node-dot">&middot;</span>
                <span className={[
                  "mockup-node-stat",
                  statTone === "crit" && "mockup-node-stat--crit",
                  statTone === "warn" && "mockup-node-stat--warn",
                ].filter(Boolean).join(" ")}>{bufferStat}</span>
              </>
            )}
          </div>
          <div className="mockup-channel-pair-row">
            <span className="mockup-channel-pair-row-label">RX</span>
            <Badge tone={rxTone}>{rxLifecycle}</Badge>
            <span className="mockup-node-dot">&middot;</span>
            <DurationDisplay ms={rx.ageMs} />
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

const mockNodeTypes = { mockNode: MockNodeComponent, channelPairNode: ChannelPairNode };
const mockEdgeTypes = { elkrouted: ElkRoutedEdge };

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

function GraphFlow({
  nodes,
  edges,
  onSelect,
}: {
  nodes: Node[];
  edges: Edge[];
  onSelect: (sel: GraphSelection) => void;
}) {
  const { fitView } = useReactFlow();

  // Only refit when the graph structure changes (nodes/edges added or removed),
  // not on selection changes which also mutate the nodes array.
  const layoutKey = useMemo(
    () => nodes.map((n) => n.id).join(",") + "|" + edges.map((e) => e.id).join(","),
    [nodes, edges],
  );
  useEffect(() => {
    fitView({ padding: 0.3, maxZoom: 1.2, duration: 0 });
  }, [layoutKey, fitView]);

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
      onNodeClick={(_event, node) => onSelect({ kind: "entity", id: node.id })}
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

function getConnectedSubgraph(entityId: string, entities: EntityDef[], edges: EdgeDef[]): { entities: EntityDef[]; edges: EdgeDef[] } {
  const connectedEdges = edges.filter((e) => e.source === entityId || e.target === entityId);
  const connectedIds = new Set([entityId]);
  for (const e of connectedEdges) {
    connectedIds.add(e.source);
    connectedIds.add(e.target);
  }
  return {
    entities: entities.filter((e) => connectedIds.has(e.id)),
    edges: connectedEdges,
  };
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
}: {
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  snapPhase: SnapPhase;
  selection: GraphSelection;
  onSelect: (sel: GraphSelection) => void;
  focusedEntityId: string | null;
  onExitFocus: () => void;
  waitingForProcesses: boolean;
}) {
  const [layout, setLayout] = useState<LayoutResult>({ nodes: [], edges: [] });

  React.useEffect(() => {
    if (entityDefs.length === 0) return;
    const sizes = measureNodeDefs(entityDefs);
    layoutGraph(entityDefs, edgeDefs, sizes).then(setLayout).catch(console.error);
  }, [entityDefs, edgeDefs]);

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

  if (entityDefs.length === 0) {
    const isBusy = snapPhase === "cutting" || snapPhase === "loading";
    return (
      <div className="mockup-graph-panel">
        <div className="mockup-graph-empty">
          {isBusy
            ? <><CircleNotch size={24} weight="bold" className="spinning mockup-graph-empty-icon" /> {GRAPH_EMPTY_MESSAGES[snapPhase]}</>
            : snapPhase === "idle" && waitingForProcesses
              ? (
                <>
                  <CircleNotch size={24} weight="bold" className="spinning mockup-graph-empty-icon" />
                  <span>Waiting for a process to connect…</span>
                </>
              )
              : snapPhase === "idle"
                ? (
                  <>
                    <Camera size={32} weight="thin" className="mockup-graph-empty-icon" />
                    <span>{GRAPH_EMPTY_MESSAGES[snapPhase]}</span>
                    <span className="mockup-graph-empty-hint">Press "Take Snapshot" to capture the current state of all connected processes</span>
                  </>
                )
                : GRAPH_EMPTY_MESSAGES[snapPhase]
          }
        </div>
      </div>
    );
  }

  return (
    <div className="mockup-graph-panel">
      <div className="mockup-graph-toolbar">
        <div className="mockup-graph-toolbar-left">
          <span className="mockup-graph-stat">{entityDefs.length} entities</span>
          <span className="mockup-graph-stat">{edgeDefs.length} edges</span>
        </div>
        {focusedEntityId && (
          <div className="mockup-graph-toolbar-right">
            <ActionButton onPress={onExitFocus}>
              <Crosshair size={14} weight="bold" />
              Exit Focus
            </ActionButton>
          </div>
        )}
      </div>
      <div className="mockup-graph-flow">
        <ReactFlowProvider>
          <GraphFlow
            nodes={nodesWithSelection}
            edges={edgesWithSelection}
            onSelect={onSelect}
          />
        </ReactFlowProvider>
      </div>
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
          <span className="mockup-inspector-mono mockup-inspector-muted">Future (no body fields)</span>
        </KeyValueRow>
      </div>
    );
  }

  if ("request" in body) {
    const req = (body as RequestBody).request;
    return (
      <div className="mockup-inspector-section">
        <KeyValueRow label="Args">
          <span className={`mockup-inspector-mono${req.args_preview === "(no args)" ? " mockup-inspector-muted" : ""}`}>
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
    const channelKind = "mpsc" in ep.details ? "mpsc"
      : "broadcast" in ep.details ? "broadcast"
      : "watch" in ep.details ? "watch"
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
          <span className="mockup-inspector-mono">{max_permits - handed_out_permits} / {max_permits}</span>
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

function ChannelPairInspectorContent({ entity, onFocus }: { entity: EntityDef; onFocus: (id: string) => void }) {
  const { tx, rx } = entity.channelPair!;
  const txEp = typeof tx.body !== "string" && "channel_tx" in tx.body ? tx.body.channel_tx : null;
  const rxEp = typeof rx.body !== "string" && "channel_rx" in rx.body ? rx.body.channel_rx : null;

  const channelKind = txEp
    ? "mpsc" in txEp.details ? "mpsc"
    : "broadcast" in txEp.details ? "broadcast"
    : "watch" in txEp.details ? "watch"
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

  const bufferFill = mpscBuffer && mpscBuffer.capacity != null
    ? Math.min(100, (mpscBuffer.occupancy / mpscBuffer.capacity) * 100)
    : null;
  const bufferTone: Tone = mpscBuffer && mpscBuffer.capacity != null
    ? mpscBuffer.occupancy >= mpscBuffer.capacity ? "crit"
    : mpscBuffer.occupancy / mpscBuffer.capacity >= 0.75 ? "warn"
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
          <a className="mockup-inspector-source-link" href={`zed://file${tx.source}`} title="Open in Zed">
            {tx.source}
          </a>
        </KeyValueRow>
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
          <a className="mockup-inspector-source-link" href={`zed://file${rx.source}`} title="Open in Zed">
            {rx.source}
          </a>
        </KeyValueRow>
      </div>
    </>
  );
}

function EntityInspectorContent({ entity, onFocus }: { entity: EntityDef; onFocus: (id: string) => void }) {
  if (entity.channelPair) {
    return <ChannelPairInspectorContent entity={entity} onFocus={onFocus} />;
  }

  const ageTone: Tone = entity.ageMs > 600_000 ? "crit" : entity.ageMs > 60_000 ? "warn" : "neutral";

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

      <div className="mockup-inspector-section">
        <KeyValueRow label="Process">
          <span className="mockup-inspector-mono">{entity.processName}</span>
          <span className="mockup-inspector-muted" style={{ fontSize: "0.75em", marginLeft: 4 }}>{entity.processId}</span>
        </KeyValueRow>
        <KeyValueRow label="Source" icon={<FileRs size={12} weight="bold" />}>
          <a className="mockup-inspector-source-link" href={`zed://file${entity.source}`} title="Open in Zed">
            {entity.source}
          </a>
        </KeyValueRow>
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
  polls: "Non-blocking observation",
  closed_by: "Closure cause",
  channel_link: "Channel pairing",
  rpc_link: "RPC pairing",
};

function EdgeInspectorContent({ edge, entityDefs }: { edge: EdgeDef; entityDefs: EntityDef[] }) {
  const srcEntity = entityDefs.find((e) => e.id === edge.source);
  const dstEntity = entityDefs.find((e) => e.id === edge.target);
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
          {srcEntity && <span className="mockup-inspector-muted" style={{ fontSize: "0.75em", marginLeft: 4 }}>{srcEntity.processName}</span>}
        </KeyValueRow>
        <KeyValueRow label="To" icon={dstEntity ? kindIcon(dstEntity.kind, 12) : undefined}>
          <span className="mockup-inspector-mono">{dstEntity?.name ?? edge.target}</span>
          {dstEntity && <span className="mockup-inspector-muted" style={{ fontSize: "0.75em", marginLeft: 4 }}>{dstEntity.processName}</span>}
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

function InspectorPanel({
  collapsed,
  onToggleCollapse,
  selection,
  entityDefs,
  edgeDefs,
  onFocusEntity,
}: {
  collapsed: boolean;
  onToggleCollapse: () => void;
  selection: GraphSelection;
  entityDefs: EntityDef[];
  edgeDefs: EdgeDef[];
  onFocusEntity: (id: string) => void;
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
    content = entity ? <EntityInspectorContent entity={entity} onFocus={onFocusEntity} /> : null;
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
      <div className="mockup-inspector-body">
        {content}
      </div>
    </div>
  );
}

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
          <ActionButton size="sm" onPress={onClose}>✕</ActionButton>
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

  const allEntities = snap.phase === "ready" ? snap.entities : [];
  const allEdges = snap.phase === "ready" ? snap.edges : [];

  const { entities, edges } = useMemo(() => {
    if (!focusedEntityId) return { entities: allEntities, edges: allEdges };
    return getConnectedSubgraph(focusedEntityId, allEntities, allEdges);
  }, [focusedEntityId, allEntities, allEdges]);

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
    return () => { cancelled = true; };
  }, [takeSnapshot]);

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
    snap.phase === "cutting" ? "Syncing…"
    : snap.phase === "loading" ? "Loading…"
    : "Take Snapshot";

  const connCount = connections?.connected_processes ?? 0;
  const waitingForProcesses = connCount === 0 && snap.phase === "idle";

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
          {waitingForProcesses
            ? <><CircleNotch size={11} weight="bold" className="spinning" /> waiting…</>
            : <>{connCount} {connCount === 1 ? "process" : "processes"}</>
          }
        </button>
        {apiMode === "lab" ? (
          <span className="mockup-header-badge">mock data</span>
        ) : snap.phase === "ready" ? (
          <span className="mockup-header-badge mockup-header-badge--active">
            <CheckCircle size={12} weight="bold" />
            snapshot
          </span>
        ) : null}
        {snap.phase === "error" && (
          <span className="mockup-header-error">{snap.message}</span>
        )}
        <span className="mockup-header-spacer" />
        <ActionButton variant="primary" onPress={takeSnapshot} isDisabled={isBusy}>
          {isBusy
            ? <CircleNotch size={14} weight="bold" />
            : <Camera size={14} weight="bold" />
          }
          {buttonLabel}
        </ActionButton>
      </div>
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
