import React from "react";
import type { GeometryEdge } from "../geometry";
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
        const d = polylineToPath(edge.polyline);
        const hitD = hitTestPath(edge.polyline);
        const edgeStyle = edge.data?.style ?? {};
        const stroke = isSelected
          ? "var(--accent, #3b82f6)"
          : (edgeStyle.stroke ?? "light-dark(#666, #999)");

        const visibleStyle: React.CSSProperties = isSelected
          ? { stroke, strokeWidth: 2.5 }
          : {
              stroke,
              strokeWidth: edgeStyle.strokeWidth,
              strokeDasharray: edgeStyle.strokeDasharray,
            };

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

            {/* Visible edge path */}
            <path
              id={edge.id}
              d={d}
              fill="none"
              style={visibleStyle}
            />

          </g>
        );
      })}
    </g>
  );
}
