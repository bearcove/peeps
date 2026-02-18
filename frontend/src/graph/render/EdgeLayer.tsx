import React from "react";
import type { GeometryEdge, Point } from "../geometry";
import { polylineToPath, hitTestPath } from "./edgePath";
import "./EdgeLayer.css";

export interface EdgeLayerProps {
  edges: GeometryEdge[];
  selectedEdgeId?: string | null;
  hoveredEdgeId?: string | null;
  onEdgeClick?: (id: string) => void;
  onEdgeHover?: (id: string | null) => void;
  ghostEdgeIds?: Set<string>;
}

// Compute the midpoint along the polyline with its local direction vector,
// then offset the label perpendicularly by 20 units (matches App.tsx behavior).
function computeLabelPos(points: Point[]): { x: number; y: number } {
  if (points.length < 2) {
    const p = points[0] ?? { x: 0, y: 0 };
    return { x: p.x, y: p.y };
  }

  let totalLength = 0;
  for (let i = 1; i < points.length; i++) {
    const dx = points[i].x - points[i - 1].x;
    const dy = points[i].y - points[i - 1].y;
    totalLength += Math.hypot(dx, dy);
  }
  if (totalLength <= 0) return points[0];

  const halfway = totalLength / 2;
  let traversed = 0;
  for (let i = 1; i < points.length; i++) {
    const seg0 = points[i - 1];
    const seg1 = points[i];
    const segLength = Math.hypot(seg1.x - seg0.x, seg1.y - seg0.y);
    if (traversed + segLength >= halfway) {
      const remain = halfway - traversed;
      const t = segLength <= 0 ? 0 : remain / segLength;
      const midX = seg0.x + (seg1.x - seg0.x) * t;
      const midY = seg0.y + (seg1.y - seg0.y) * t;
      const dx = seg1.x - seg0.x;
      const dy = seg1.y - seg0.y;
      const dirLen = Math.hypot(dx, dy) || 1;
      // Perpendicular normal (rotate direction 90°)
      const nx = -dy / dirLen;
      const ny = dx / dirLen;
      return { x: midX + nx * 20, y: midY + ny * 20 };
    }
    traversed += segLength;
  }
  const mid = points[Math.floor(points.length / 2)];
  return { x: mid.x, y: mid.y };
}

function markerIdForEdge(edge: GeometryEdge, isSelected: boolean): string {
  if (isSelected) return "el-arrow-selected";
  const kind = edge.kind;
  if (kind === "needs" && edge.data?.edgePending) return "el-arrow-needs-pending";
  switch (kind) {
    case "needs":        return "el-arrow-needs";
    case "holds":        return "el-arrow-holds";
    case "polls":        return "el-arrow-polls";
    case "closed_by":    return "el-arrow-closed-by";
    case "channel_link": return "el-arrow-channel-link";
    case "rpc_link":     return "el-arrow-rpc-link";
    default:             return "el-arrow-holds";
  }
}

// Closed arrowhead path in a normalized 10×10 viewBox, tip at (10,5).
const ARROW_PATH = "M 0 0 L 10 5 L 0 10 Z";

function ArrowMarker({
  id,
  fill,
  size,
}: {
  id: string;
  fill: string;
  size: number;
}) {
  return (
    <marker
      id={id}
      viewBox="0 0 10 10"
      refX="10"
      refY="5"
      markerWidth={size}
      markerHeight={size}
      markerUnits="userSpaceOnUse"
      orient="auto"
    >
      <path d={ARROW_PATH} style={{ fill }} />
    </marker>
  );
}

function EdgeMarkerDefs() {
  return (
    <defs>
      <ArrowMarker id="el-arrow-needs"         fill="light-dark(#d7263d, #ff6b81)" size={10} />
      <ArrowMarker id="el-arrow-needs-pending" fill="light-dark(#d7263d, #ff6b81)" size={14} />
      <ArrowMarker id="el-arrow-holds"         fill="light-dark(#2f6fed, #7aa2ff)" size={8}  />
      <ArrowMarker id="el-arrow-polls"         fill="light-dark(#8e7cc3, #b4a7d6)" size={8}  />
      <ArrowMarker id="el-arrow-closed-by"     fill="light-dark(#e08614, #f0a840)" size={8}  />
      <ArrowMarker id="el-arrow-channel-link"  fill="light-dark(#888, #666)"       size={8}  />
      <ArrowMarker id="el-arrow-rpc-link"      fill="light-dark(#888, #666)"       size={8}  />
      <ArrowMarker id="el-arrow-selected"      fill="var(--accent, #3b82f6)"       size={10} />
    </defs>
  );
}

export function EdgeLayer({
  edges,
  selectedEdgeId,
  hoveredEdgeId: _hoveredEdgeId,
  onEdgeClick,
  onEdgeHover,
  ghostEdgeIds,
}: EdgeLayerProps) {
  return (
    <g>
      <EdgeMarkerDefs />
      {edges.map((edge) => {
        if (edge.polyline.length < 2) return null;

        const isSelected = selectedEdgeId === edge.id;
        const isGhost = ghostEdgeIds?.has(edge.id) ?? false;
        const d = polylineToPath(edge.polyline);
        const hitD = hitTestPath(edge.polyline);
        const markerId = markerIdForEdge(edge, isSelected);
        const edgeStyle = edge.data?.style ?? {};
        const edgeLabel = edge.data?.edgeLabel as string | undefined;
        const edgePending = edge.data?.edgePending as boolean | undefined;

        const visibleStyle: React.CSSProperties = isSelected
          ? { stroke: "var(--accent, #3b82f6)", strokeWidth: 2.5 }
          : {
              stroke: edgeStyle.stroke,
              strokeWidth: edgeStyle.strokeWidth,
              strokeDasharray: edgeStyle.strokeDasharray,
            };

        const labelPos = edgeLabel ? computeLabelPos(edge.polyline) : null;

        return (
          <g
            key={edge.id}
            style={isGhost ? { opacity: 0.2, pointerEvents: "none" } : undefined}
          >
            {/* Wide invisible hit area */}
            <path
              d={hitD}
              fill="none"
              stroke="transparent"
              strokeWidth={14}
              data-pan-block="true"
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
                  stroke="var(--accent, #3b82f6)"
                  strokeWidth={10}
                  strokeLinecap="round"
                  opacity={0.18}
                  className="edge-glow"
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

            {/* Visible edge path with arrow marker */}
            <path
              id={edge.id}
              d={d}
              fill="none"
              style={visibleStyle}
              markerEnd={`url(#${markerId})`}
            />

            {/* Label at midpoint */}
            {edgeLabel && labelPos && (
              <g
                className="edge-label"
                transform={`translate(${labelPos.x}, ${labelPos.y})`}
              >
                <text
                  className="edge-label-text"
                  textAnchor="middle"
                  dominantBaseline="middle"
                >
                  {edgeLabel}
                  {edgePending && (
                    <tspan className="edge-label-symbol"> ⏳</tspan>
                  )}
                </text>
              </g>
            )}
          </g>
        );
      })}
    </g>
  );
}
