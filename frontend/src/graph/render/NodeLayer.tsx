import React from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import type { GeometryNode } from "../geometry";
import type { EntityDef } from "../../snapshot";
import { GraphNode, type GraphNodeData } from "../../components/graph/GraphNode";
import { ChannelPairNode, type ChannelPairNodeData } from "../../components/graph/ChannelPairNode";
import { RpcPairNode, type RpcPairNodeData } from "../../components/graph/RpcPairNode";
import "./NodeLayer.css";

export interface NodeLayerProps {
  nodes: GeometryNode[];
  selectedNodeId?: string | null;
  hoveredNodeId?: string | null;
  onNodeClick?: (id: string) => void;
  onNodeHover?: (id: string | null) => void;
  ghostNodeIds?: Set<string>;
}

// ── Measurement ───────────────────────────────────────────────

/** Render each entity's card in a hidden off-screen container and return measured sizes. */
export async function measureEntityDefs(
  defs: EntityDef[],
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

    let card: React.ReactNode;
    if (def.channelPair) {
      card = (
        <ChannelPairNode
          data={{
            nodeId: def.id,
            tx: def.channelPair.tx,
            rx: def.channelPair.rx,
            channelName: def.name,
            selected: false,
            statTone: def.statTone,
          }}
        />
      );
    } else if (def.rpcPair) {
      card = (
        <RpcPairNode
          data={{
            req: def.rpcPair.req,
            resp: def.rpcPair.resp,
            rpcName: def.name,
            selected: false,
          }}
        />
      );
    } else {
      card = (
        <GraphNode
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
    }

    flushSync(() => root.render(card));
    sizes.set(def.id, { width: el.offsetWidth, height: el.offsetHeight });
    root.unmount();
  }

  document.body.removeChild(container);
  return sizes;
}

// ── NodeLayer ──────────────────────────────────────────────────

export function NodeLayer({
  nodes,
  selectedNodeId,
  hoveredNodeId: _hoveredNodeId,
  onNodeClick,
  onNodeHover,
  ghostNodeIds,
}: NodeLayerProps) {
  if (nodes.length === 0) return null;

  return (
    <>
      {nodes.map((node) => {
        const { x, y, width, height } = node.worldRect;
        const selected = node.id === selectedNodeId;
        const isGhost = !!(node.data?.ghost as boolean | undefined) || !!ghostNodeIds?.has(node.id);

        let cardContent: React.ReactNode;
        if (node.kind === "channelPairNode") {
          cardContent = (
            <ChannelPairNode
              data={{ ...(node.data as ChannelPairNodeData), selected, ghost: isGhost }}
            />
          );
        } else if (node.kind === "rpcPairNode") {
          cardContent = (
            <RpcPairNode
              data={{ ...(node.data as RpcPairNodeData), selected, ghost: isGhost }}
            />
          );
        } else {
          cardContent = (
            <GraphNode
              data={{ ...(node.data as GraphNodeData), selected, ghost: isGhost }}
            />
          );
        }

        return (
          <foreignObject
            key={node.id}
            x={x}
            y={y}
            width={width}
            height={height}
            data-pan-block="true"
            style={{ overflow: "visible" }}
            onClick={() => onNodeClick?.(node.id)}
            onMouseEnter={() => onNodeHover?.(node.id)}
            onMouseLeave={() => onNodeHover?.(null)}
          >
            {/* xmlns required for HTML content inside SVG foreignObject */}
            <div
              // @ts-expect-error xmlns is valid in SVG foreignObject context
              xmlns="http://www.w3.org/1999/xhtml"
              className="nl-fo-wrapper"
            >
              {cardContent}
            </div>
          </foreignObject>
        );
      })}
    </>
  );
}
