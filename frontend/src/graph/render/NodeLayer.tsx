import React from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import type { GeometryNode } from "../geometry";
import type { EntityDef } from "../../snapshot";
import { GraphNode, collapsedFrameCount } from "../../components/graph/GraphNode";
import {
  graphNodeDataFromEntity,
  computeNodeSublabel,
  type GraphNodeData,
} from "../../components/graph/graphNodeData";
import { canonicalNodeKind } from "../../nodeKindSpec";
import { scopeKindIcon } from "../../scopeKindSpec";
import type { GraphFilterLabelMode } from "../../graphFilter";
import type { NodeExpandState } from "../../components/graph/GraphViewport";
import { cachedFetchSourcePreview } from "../../api/sourceCache";
import "../../components/graph/ScopeGroupNode.css";
import "./NodeLayer.css";

export interface NodeLayerProps {
  nodes: GeometryNode[];
  prevNodes?: GeometryNode[];
  nodeExpandStates?: Map<string, NodeExpandState>;
  nodeOpacityById?: Map<string, number>;
  onNodeClick?: (id: string) => void;
  onNodeContextMenu?: (id: string, clientX: number, clientY: number) => void;
  onNodeHover?: (id: string | null) => void;
  ghostNodeIds?: Set<string>;
}

type SubgraphScopeMode = "none" | "process" | "crate";

export type GraphMeasureResult = {
  nodeSizes: Map<string, { width: number; height: number }>;
  subgraphHeaderHeight: number;
};

// ── Measurement ───────────────────────────────────────────────

/** Render each entity's card in a hidden off-screen container and return measured sizes. */
export async function measureEntityDefs(
  defs: EntityDef[],
): Promise<Map<string, { width: number; height: number }>> {
  const measurements = await measureGraphLayout(defs, "none");
  return measurements.nodeSizes;
}

/** Measure node cards plus subgraph header height (for ELK top padding). */
export async function measureGraphLayout(
  defs: EntityDef[],
  subgraphScopeMode: SubgraphScopeMode = "none",
  labelBy?: GraphFilterLabelMode,
  showSource?: boolean,
  expandedNodeIds?: Set<string>,
): Promise<GraphMeasureResult> {
  // Pre-fetch source data for collapsed nodes that need it (futures, showSource).
  // Expanded nodes are measured from DOM output and do not block on full source previews.
  {
    const fetches: Promise<unknown>[] = [];
    for (const def of defs) {
      const isExpanded = expandedNodeIds?.has(def.id) ?? false;
      if (isExpanded) continue;
      const needsSource = showSource || canonicalNodeKind(def.kind) === "future";
      if (!needsSource) continue;
      const frames = def.frames.slice(0, collapsedFrameCount(def.kind));
      for (const frame of frames) {
        if (frame.frame_id != null) {
          fetches.push(cachedFetchSourcePreview(frame.frame_id).catch(() => {}));
        }
      }
    }
    if (fetches.length > 0) await Promise.all(fetches);
  }

  // Escape React's useEffect lifecycle so flushSync works on our measurement roots.
  await Promise.resolve();
  // Ensure text measurement uses final webfont metrics.
  if (typeof document !== "undefined" && "fonts" in document) {
    try {
      await (document as Document & { fonts?: { ready?: Promise<unknown> } }).fonts?.ready;
    } catch {
      // Non-fatal: fallback metrics are still better than blocking.
    }
  }

  const container = document.createElement("div");
  container.className = "nl-measure-root";
  container.style.cssText =
    "position:fixed;top:-9999px;left:-9999px;visibility:hidden;pointer-events:none;display:flex;flex-direction:column;align-items:flex-start;gap:4px;";
  document.body.appendChild(container);

  const sizes = new Map<string, { width: number; height: number }>();

  try {
    for (const def of defs) {
      const isExpanded = expandedNodeIds?.has(def.id) ?? false;
      const el = document.createElement("div");
      container.appendChild(el);
      const root = createRoot(el);

      try {
        const sublabel = labelBy ? computeNodeSublabel(def, labelBy) : undefined;
        // During measurement, hooks won't complete async fetches in this render turn.
        flushSync(() =>
          root.render(
            <GraphNode
              data={{ ...graphNodeDataFromEntity(def), sublabel, showSource }}
              expanded={isExpanded}
            />,
          ),
        );
        const width = el.offsetWidth;
        const height = el.offsetHeight;
        if (width <= 0 || height <= 0) {
          throw new Error(
            `[graph-measure] invalid node size for ${def.id} (${def.kind}): ${width}x${height}`,
          );
        }
        sizes.set(def.id, { width, height });
      } finally {
        root.unmount();
        container.removeChild(el);
      }
    }

    let subgraphHeaderHeight = 0;
    if (subgraphScopeMode !== "none") {
      const el = document.createElement("div");
      container.appendChild(el);
      const root = createRoot(el);
      const sampleLabel =
        subgraphScopeMode === "process" ? "moire-examples(27139)" : "moire-example";

      try {
        flushSync(() =>
          root.render(
            <div className="scope-group" style={{ width: 320 }}>
              <div className="scope-group-header">
                <span className="scope-group-label">
                  <span className="scope-group-icon">{scopeKindIcon(subgraphScopeMode, 12)}</span>
                  <span>{sampleLabel}</span>
                </span>
              </div>
            </div>,
          ),
        );

        const headerEl = el.querySelector(".scope-group-header");
        if (headerEl instanceof HTMLElement) subgraphHeaderHeight = headerEl.offsetHeight;
      } finally {
        root.unmount();
        container.removeChild(el);
      }
    }

    return { nodeSizes: sizes, subgraphHeaderHeight };
  } finally {
    document.body.removeChild(container);
  }
}

// ── NodeLayer ──────────────────────────────────────────────────

export function NodeLayer({
  nodes,
  prevNodes: _prevNodes,
  nodeExpandStates,
  nodeOpacityById,
  onNodeClick,
  onNodeContextMenu,
  onNodeHover,
  ghostNodeIds,
}: NodeLayerProps) {
  if (nodes.length === 0) return null;

  // Render expanded/expanding nodes last so they paint on top (SVG has no z-index).
  const expandedOrExpandingId = nodeExpandStates
    ? ([...nodeExpandStates].find(([, s]) => s === "expanded" || s === "expanding")?.[0] ?? null)
    : null;
  const ordered = expandedOrExpandingId
    ? [...nodes].sort((a, b) =>
        a.id === expandedOrExpandingId ? 1 : b.id === expandedOrExpandingId ? -1 : 0,
      )
    : nodes;

  return (
    <>
      {ordered.map((node) => {
        const { x, y, width, height } = node.worldRect;
        const isGhost = !!(node.data?.ghost as boolean | undefined) || !!ghostNodeIds?.has(node.id);
        const transitionOpacity = nodeOpacityById?.get(node.id) ?? 1;
        const expandState = nodeExpandStates?.get(node.id);
        const isExpanded = expandState === "expanded";
        const isExpanding = expandState === "expanding";
        const cardContent = (
          <GraphNode
            data={{ ...(node.data as GraphNodeData), ghost: isGhost }}
            expanded={isExpanded}
            expanding={isExpanding}
          />
        );

        return (
          <div
            key={node.id}
            data-pan-block="true"
            className="nl-node-shell"
            style={{
              transform: `translate(${x}px, ${y}px)`,
              width,
              height,
              opacity: transitionOpacity,
              pointerEvents: transitionOpacity < 0.99 ? "none" : "all",
            }}
            onClick={() => onNodeClick?.(node.id)}
            onContextMenu={(event) => {
              event.preventDefault();
              event.stopPropagation();
              onNodeContextMenu?.(node.id, event.clientX, event.clientY);
            }}
            onMouseEnter={() => onNodeHover?.(node.id)}
            onMouseLeave={() => onNodeHover?.(null)}
          >
            <div className="nl-node-content">{cardContent}</div>
          </div>
        );
      })}
    </>
  );
}
