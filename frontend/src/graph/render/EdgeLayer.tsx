import React, { useId, useMemo } from "react";
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
  edgeOpacityById?: Map<string, number>;
  portAnchors?: Map<string, Point>;
}

export function EdgeLayer({
  edges,
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
          const polyline = edge.polyline.map((p) => ({ ...p }));
          if (polyline.length > 0 && sourceAnchor) polyline[0] = sourceAnchor;
          if (polyline.length > 0 && targetAnchor) polyline[polyline.length - 1] = targetAnchor;
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

          const edgeClass = ["edge", isSelected ? "edge--selected" : "", isGhost ? "edge--ghost" : ""]
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
