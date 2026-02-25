import React, { useId, useMemo } from "react";
import type { GeometryEdge, GeometryNode, Point, Rect } from "../geometry";
import { polylineToPath, hitTestPath } from "./edgePath";
import "./EdgeLayer.css";

export interface EdgeLayerProps {
  edges: GeometryEdge[];
  nodes?: GeometryNode[];
  selectedEdgeId?: string | null;
  hoveredEdgeId?: string | null;
  onEdgeClick?: (id: string) => void;
  onEdgeHover?: (id: string | null) => void;
  ghostEdgeIds?: Set<string>;
  edgeOpacityById?: Map<string, number>;
  portAnchors?: Map<string, Point>;
}

export function EdgeLayer({
  edges,
  nodes,
  selectedEdgeId,
  hoveredEdgeId: _hoveredEdgeId,
  onEdgeClick,
  onEdgeHover,
  ghostEdgeIds,
  edgeOpacityById,
  portAnchors,
}: EdgeLayerProps) {
  const markerBaseId = useId();
  const markerSizes = useMemo(() => {
    const sizes = new Set<number>();
    for (const edge of edges) {
      sizes.add((edge.data?.markerSize as number | undefined) ?? 8);
    }
    return [...sizes].sort((a, b) => a - b);
  }, [edges]);
  const nodeRectById = useMemo(() => {
    const map = new Map<string, Rect>();
    for (const node of nodes ?? []) map.set(node.id, node.worldRect);
    return map;
  }, [nodes]);

  return (
    <svg className="graph-layer edge-layer" aria-hidden="true">
      <defs>
        {markerSizes.map((size) => {
          const half = size / 2;
          return (
            <marker
              key={size}
              id={`${markerBaseId}-${size}`}
              markerWidth={size}
              markerHeight={size}
              refX="0"
              refY={half}
              orient="auto"
              markerUnits="userSpaceOnUse"
            >
              <path d={`M 0 0 L ${size} ${half} L 0 ${size} Z`} fill="context-stroke" />
            </marker>
          );
        })}
      </defs>
      <g>
        {edges.map((edge) => {
          if (edge.polyline.length < 2) return null;

          const isSelected = selectedEdgeId === edge.id;
          const isGhost = ghostEdgeIds?.has(edge.id) ?? false;
          const transitionOpacity = edgeOpacityById?.get(edge.id) ?? 1;
          const sourcePortRef = edge.data?.sourcePortRef as string | undefined;
          const targetPortRef = edge.data?.targetPortRef as string | undefined;
          const sourceAnchor = sourcePortRef ? portAnchors?.get(sourcePortRef) : undefined;
          const targetAnchor = targetPortRef ? portAnchors?.get(targetPortRef) : undefined;
          const sourceNodeRect = nodeRectById.get(edge.sourceId);
          const targetNodeRect = nodeRectById.get(edge.targetId);
          const polyline = edge.polyline.map((p) => ({ ...p }));
          if (polyline.length > 1) {
            const sourceNeighbor = polyline[1];
            const targetNeighbor = polyline[polyline.length - 2];
            const resolvedSourceAnchor = resolveEdgeEndpointAnchor({
              nodeRect: sourceNodeRect,
              portRef: sourcePortRef,
              portAnchor: sourceAnchor,
              neighbor: sourceNeighbor,
              isSource: true,
            });
            const resolvedTargetAnchor = resolveEdgeEndpointAnchor({
              nodeRect: targetNodeRect,
              portRef: targetPortRef,
              portAnchor: targetAnchor,
              neighbor: targetNeighbor,
              isSource: false,
            });
            if (resolvedSourceAnchor) polyline[0] = resolvedSourceAnchor;
            if (resolvedTargetAnchor) polyline[polyline.length - 1] = resolvedTargetAnchor;
          }
          const edgeStyle = edge.data?.style ?? {};
          const stroke = isSelected
            ? "var(--accent)"
            : (edgeStyle.stroke ?? "var(--edge-stroke-muted)");
          const markerSize = (edge.data?.markerSize as number | undefined) ?? 8;
          const markerEnd = `url(#${markerBaseId}-${markerSize})`;

          // Shorten the path end so the stroke terminates at the arrowhead base,
          // not the tip. Combined with refX=0 on the marker, the tip lands exactly
          // at the original target anchor (node border) with no bleed-through.
          const visPolyline = polyline.map((p) => ({ ...p }));
          if (visPolyline.length >= 2) {
            const tip = visPolyline[visPolyline.length - 1];
            const prev = visPolyline[visPolyline.length - 2];
            const dx = tip.x - prev.x;
            const dy = tip.y - prev.y;
            const len = Math.sqrt(dx * dx + dy * dy);
            if (len > markerSize) {
              visPolyline[visPolyline.length - 1] = {
                x: tip.x - (dx / len) * markerSize,
                y: tip.y - (dy / len) * markerSize,
              };
            }
          }

          const d = polylineToPath(visPolyline);
          const hitD = hitTestPath(polyline);

          const visibleStyle: React.CSSProperties = isSelected
            ? { stroke, strokeWidth: 2.5, strokeLinecap: "round" }
            : {
                stroke,
                strokeWidth: edgeStyle.strokeWidth,
                strokeDasharray: edgeStyle.strokeDasharray,
                strokeLinecap: "round",
              };

          const edgeClass = [
            "edge",
            isSelected ? "edge--selected" : "",
            isGhost ? "edge--ghost" : "",
          ]
            .filter(Boolean)
            .join(" ");

          return (
            <g
              key={edge.id}
              className={edgeClass}
              style={
                isGhost
                  ? { opacity: 0.2 * transitionOpacity, pointerEvents: "none" }
                  : { opacity: transitionOpacity }
              }
            >
              {/* Wide invisible hit area */}
              <path
                d={hitD}
                fill="none"
                stroke="transparent"
                strokeWidth={14}
                data-pan-block="true"
                className="edge-hit"
                style={{ cursor: "pointer", pointerEvents: isGhost ? "none" : "all" }}
                onClick={() => onEdgeClick?.(edge.id)}
                onMouseEnter={() => onEdgeHover?.(edge.id)}
                onMouseLeave={() => onEdgeHover?.(null)}
              />

              {/* Selection glow */}
              {isSelected && (
                <>
                  <path
                    d={d}
                    fill="none"
                    stroke="var(--accent)"
                    strokeWidth={10}
                    strokeLinecap="round"
                    opacity={0.18}
                    className="edge-glow"
                  />
                  <path
                    d={d}
                    fill="none"
                    stroke="var(--accent)"
                    strokeWidth={5}
                    strokeLinecap="round"
                    opacity={0.45}
                    className="edge-glow-inner"
                  />
                </>
              )}

              {/* Visible edge path */}
              <path
                id={edge.id}
                d={d}
                fill="none"
                className="edge-path"
                style={visibleStyle}
                markerEnd={markerEnd}
              />
            </g>
          );
        })}
      </g>
    </svg>
  );
}

type PortFace = "north" | "south" | "east" | "west";

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function inferFaceFromPortRef(portRef: string | undefined, isSource: boolean): PortFace | null {
  if (!portRef) return null;
  if (portRef.includes(":in:") || portRef.endsWith(":rx") || portRef.endsWith(":resp")) {
    return "north";
  }
  if (portRef.includes(":out:") || portRef.endsWith(":tx") || portRef.endsWith(":req")) {
    return "south";
  }
  return isSource ? "south" : "north";
}

function closestRectFace(rect: Rect, point: Point): PortFace {
  const dNorth = Math.abs(point.y - rect.y);
  const dSouth = Math.abs(point.y - (rect.y + rect.height));
  const dWest = Math.abs(point.x - rect.x);
  const dEast = Math.abs(point.x - (rect.x + rect.width));
  const min = Math.min(dNorth, dSouth, dWest, dEast);
  if (min === dNorth) return "north";
  if (min === dSouth) return "south";
  if (min === dWest) return "west";
  return "east";
}

function inferFaceFromNeighbor(rect: Rect, neighbor: Point, isSource: boolean): PortFace {
  const cx = rect.x + rect.width / 2;
  const cy = rect.y + rect.height / 2;
  const dx = neighbor.x - cx;
  const dy = neighbor.y - cy;
  if (Math.abs(dx) > Math.abs(dy)) {
    if (dx >= 0) return isSource ? "east" : "west";
    return isSource ? "west" : "east";
  }
  if (dy >= 0) return isSource ? "south" : "north";
  return isSource ? "north" : "south";
}

function anchorPointForFace(rect: Rect, face: PortFace, hint: Point | null): Point {
  const centerX = rect.x + rect.width / 2;
  const centerY = rect.y + rect.height / 2;
  const hintX = hint ? clamp(hint.x, rect.x, rect.x + rect.width) : centerX;
  const hintY = hint ? clamp(hint.y, rect.y, rect.y + rect.height) : centerY;
  if (face === "north") return { x: hintX, y: rect.y };
  if (face === "south") return { x: hintX, y: rect.y + rect.height };
  if (face === "west") return { x: rect.x, y: hintY };
  return { x: rect.x + rect.width, y: hintY };
}

function resolveEdgeEndpointAnchor({
  nodeRect,
  portRef,
  portAnchor,
  neighbor,
  isSource,
}: {
  nodeRect: Rect | undefined;
  portRef: string | undefined;
  portAnchor: Point | undefined;
  neighbor: Point;
  isSource: boolean;
}): Point | undefined {
  if (!nodeRect) return portAnchor;
  const face =
    (portAnchor ? closestRectFace(nodeRect, portAnchor) : null) ??
    inferFaceFromPortRef(portRef, isSource) ??
    inferFaceFromNeighbor(nodeRect, neighbor, isSource);
  return anchorPointForFace(nodeRect, face, portAnchor ?? neighbor);
}
