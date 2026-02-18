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
  portAnchors?: Map<string, Point>;
}

export function EdgeLayer({
  edges,
  selectedEdgeId,
  hoveredEdgeId: _hoveredEdgeId,
  onEdgeClick,
  onEdgeHover,
  ghostEdgeIds,
  portAnchors,
}: EdgeLayerProps) {
  return (
    <g>
      {edges.map((edge) => {
        if (edge.polyline.length < 2) return null;

        const isSelected = selectedEdgeId === edge.id;
        const isGhost = ghostEdgeIds?.has(edge.id) ?? false;
        const sourcePortRef = edge.data?.sourcePortRef as string | undefined;
        const targetPortRef = edge.data?.targetPortRef as string | undefined;
        const sourceAnchor = sourcePortRef ? portAnchors?.get(sourcePortRef) : undefined;
        const targetAnchor = targetPortRef ? portAnchors?.get(targetPortRef) : undefined;
        const polyline = edge.polyline.map((p) => ({ ...p }));
        if (polyline.length > 0 && sourceAnchor) polyline[0] = sourceAnchor;
        if (polyline.length > 0 && targetAnchor) polyline[polyline.length - 1] = targetAnchor;
        const d = polylineToPath(polyline);
        const hitD = hitTestPath(polyline);
        const edgeStyle = edge.data?.style ?? {};
        const stroke = isSelected
          ? "var(--accent)"
          : (edgeStyle.stroke ?? "var(--edge-stroke-muted)");

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
