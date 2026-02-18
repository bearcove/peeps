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

function polylineLength(points: Point[]): number {
  let total = 0;
  for (let i = 1; i < points.length; i++) {
    const dx = points[i].x - points[i - 1].x;
    const dy = points[i].y - points[i - 1].y;
    total += Math.hypot(dx, dy);
  }
  return total;
}

function trimPolylineEnd(points: Point[], trim: number): Point[] {
  if (points.length < 2 || trim <= 0) return [...points];
  const total = polylineLength(points);
  if (total <= trim + 1e-6) return [...points];
  const keep = total - trim;

  const out: Point[] = [points[0]];
  let traversed = 0;
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1];
    const b = points[i];
    const segLen = Math.hypot(b.x - a.x, b.y - a.y);
    if (segLen <= 1e-6) continue;
    if (traversed + segLen < keep - 1e-6) {
      out.push(b);
      traversed += segLen;
      continue;
    }
    const remain = keep - traversed;
    const t = Math.max(0, Math.min(1, remain / segLen));
    out.push({
      x: a.x + (b.x - a.x) * t,
      y: a.y + (b.y - a.y) * t,
    });
    return out;
  }
  return [...points];
}

function pointAtDistanceFromEnd(points: Point[], distanceFromEnd: number): Point {
  if (points.length === 0) return { x: 0, y: 0 };
  if (points.length === 1 || distanceFromEnd <= 0) return points[points.length - 1];

  const total = polylineLength(points);
  const targetFromStart = Math.max(0, total - distanceFromEnd);
  let traversed = 0;

  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1];
    const b = points[i];
    const segLen = Math.hypot(b.x - a.x, b.y - a.y);
    if (segLen <= 1e-6) continue;
    if (traversed + segLen < targetFromStart - 1e-6) {
      traversed += segLen;
      continue;
    }
    const remain = targetFromStart - traversed;
    const t = Math.max(0, Math.min(1, remain / segLen));
    return {
      x: a.x + (b.x - a.x) * t,
      y: a.y + (b.y - a.y) * t,
    };
  }

  return points[0];
}

type ArrowGeom = {
  tip: Point;
  left: Point;
  right: Point;
};

function computeArrow(points: Point[], shaftEnd: Point, size: number): ArrowGeom | null {
  if (points.length < 2) return null;
  const endpoint = points[points.length - 1];
  const lookback = Math.max(14, size * 2);
  const tail = pointAtDistanceFromEnd(points, lookback);
  const dx = endpoint.x - tail.x;
  const dy = endpoint.y - tail.y;
  const len = Math.hypot(dx, dy);
  if (len <= 1e-6) return null;

  const ux = dx / len;
  const uy = dy / len;
  const nx = -uy;
  const ny = ux;

  const arrowLength = Math.max(6, size);
  const halfWidth = arrowLength * 0.5;
  const base = shaftEnd;
  const tip = { x: base.x + ux * arrowLength, y: base.y + uy * arrowLength };

  return {
    tip,
    left: { x: base.x + nx * halfWidth, y: base.y + ny * halfWidth },
    right: { x: base.x - nx * halfWidth, y: base.y - ny * halfWidth },
  };
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
      {edges.map((edge) => {
        if (edge.polyline.length < 2) return null;

        const isSelected = selectedEdgeId === edge.id;
        const isGhost = ghostEdgeIds?.has(edge.id) ?? false;
        const markerSize = (edge.data?.markerSize as number | undefined) ?? 8;
        const arrowLength = Math.max(6, markerSize);
        const shaftPoints = trimPolylineEnd(edge.polyline, arrowLength);
        const shaftEnd = shaftPoints[shaftPoints.length - 1];
        const d = polylineToPath(shaftPoints);
        const hitD = hitTestPath(edge.polyline);
        const edgeStyle = edge.data?.style ?? {};
        const edgeLabel = edge.data?.edgeLabel as string | undefined;
        const edgePending = edge.data?.edgePending as boolean | undefined;
        const stroke = isSelected
          ? "var(--accent, #3b82f6)"
          : (edgeStyle.stroke ?? "light-dark(#666, #999)");
        const arrow = shaftEnd ? computeArrow(edge.polyline, shaftEnd, markerSize) : null;

        const visibleStyle: React.CSSProperties = isSelected
          ? { stroke, strokeWidth: 2.5 }
          : {
              stroke,
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
            />
            {arrow && (
              <path
                d={`M ${arrow.tip.x} ${arrow.tip.y} L ${arrow.left.x} ${arrow.left.y} L ${arrow.right.x} ${arrow.right.y} Z`}
                fill={stroke}
                style={{ pointerEvents: "none" }}
              />
            )}

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
