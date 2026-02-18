import React from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { ReactFlowProvider, MarkerType, type Node, type Edge } from "@xyflow/react";
import ELK from "elkjs/lib/elk-api.js";
import elkWorkerUrl from "elkjs/lib/elk-worker.min.js?url";
import type { EntityDef, EdgeDef } from "./snapshot";

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

// ── Edge styling ──────────────────────────────────────────────

export function edgeStyle(kind: EdgeDef["kind"]) {
  switch (kind) {
    case "needs":
      return { stroke: "light-dark(#d7263d, #ff6b81)", strokeWidth: 2.4 };
    case "holds":
      return { stroke: "light-dark(#2f6fed, #7aa2ff)", strokeWidth: 2 };
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

export function edgeTooltip(kind: EdgeDef["kind"], sourceName: string, targetName: string): string {
  switch (kind) {
    case "needs":
      return `${sourceName} is blocked waiting for ${targetName}`;
    case "holds":
      return `${sourceName} currently grants permits to ${targetName}`;
    case "polls":
      return `${sourceName} polls ${targetName} (non-blocking)`;
    case "closed_by":
      return `${sourceName} was closed by ${targetName}`;
    case "channel_link":
      return `Channel endpoint: ${sourceName} → ${targetName}`;
    case "rpc_link":
      return `RPC pair: ${sourceName} → ${targetName}`;
  }
}

export function edgeMarkerSize(kind: EdgeDef["kind"]): number {
  return kind === "needs" ? 12 : 8;
}

// ── Types ─────────────────────────────────────────────────────

export type ElkPoint = { x: number; y: number };
export type LayoutResult = { nodes: Node[]; edges: Edge[] };

/** Callback that renders a measurement-mode React node for a given EntityDef. */
export type RenderNodeForMeasure = (def: EntityDef) => React.ReactNode;

// ── Measurement ───────────────────────────────────────────────

export async function measureNodeDefs(
  defs: EntityDef[],
  renderNode: RenderNodeForMeasure,
): Promise<Map<string, { width: number; height: number }>> {
  // Escape React's useEffect lifecycle so flushSync works on our measurement roots.
  await Promise.resolve();

  const container = document.createElement("div");
  container.style.cssText =
    "position:fixed;top:-9999px;left:-9999px;visibility:hidden;pointer-events:none;display:flex;flex-direction:column;align-items:flex-start;gap:4px;";
  document.body.appendChild(container);

  const sizes = new Map<string, { width: number; height: number }>();

  for (const def of defs) {
    const el = document.createElement("div");
    container.appendChild(el);
    const root = createRoot(el);

    const node = renderNode(def);

    flushSync(() => {
      root.render(<ReactFlowProvider>{node}</ReactFlowProvider>);
    });

    const w = el.offsetWidth;
    const h = el.offsetHeight;
    console.log("[measure]", def.id, w, h);
    sizes.set(def.id, { width: w, height: h });
    root.unmount();
  }

  document.body.removeChild(container);
  return sizes;
}

// ── Layout ────────────────────────────────────────────────────

export async function layoutGraph(
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

  const posMap = new Map((result.children ?? []).map((c) => [c.id, { x: c.x ?? 0, y: c.y ?? 0 }]));
  const elkEdgeMap = new Map((result.edges ?? []).map((e: any) => [e.id, e.sections ?? []]));

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
    if (def.rpcPair) {
      return {
        id: def.id,
        type: "rpcPairNode",
        position,
        data: {
          req: def.rpcPair.req,
          resp: def.rpcPair.resp,
          rpcName: def.name,
          selected: false,
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
