import React, { useMemo } from "react";
import type { GraphFrameData } from "../../components/graph/graphNodeData";
import { BacktraceDisplay } from "../../components/graph/BacktraceDisplay";
import type { GeometryEdge, Point } from "../geometry";
import "./EdgeEventLayer.css";

type EdgeEventMeta = {
  eventLabel: string;
  backtraceId: number;
  frames: GraphFrameData[];
  allFrames: GraphFrameData[];
  framesLoading: boolean;
};

type EdgeEventEntry = {
  edge: GeometryEdge;
  point: Point;
  meta: EdgeEventMeta;
};

export interface EdgeEventLayerProps {
  edges: GeometryEdge[];
  selectedEdgeId?: string | null;
  onEdgeClick?: (id: string) => void;
}

export function EdgeEventLayer({ edges, selectedEdgeId, onEdgeClick }: EdgeEventLayerProps) {
  const entries = useMemo<EdgeEventEntry[]>(() => {
    const out: EdgeEventEntry[] = [];
    for (const edge of edges) {
      const meta = edgeEventMetaFromGeometryEdge(edge);
      if (!meta) continue;
      if (edge.polyline.length < 2) continue;
      out.push({
        edge,
        point: midpointAlongPolyline(edge.polyline),
        meta,
      });
    }
    return out;
  }, [edges]);

  const selectedEntry = useMemo(() => {
    if (!selectedEdgeId) return null;
    return entries.find((entry) => entry.edge.id === selectedEdgeId) ?? null;
  }, [entries, selectedEdgeId]);

  // When there are no user frames, fall back to all frames as primary display.
  const displayFrames = useMemo(() => {
    if (!selectedEntry) return [];
    const { frames, allFrames } = selectedEntry.meta;
    return frames.length > 0 ? frames : allFrames;
  }, [selectedEntry]);

  return (
    <div className="edge-event-layer">
      {entries.map((entry) => {
        const isSelected = selectedEdgeId === entry.edge.id;
        return (
          <button
            key={entry.edge.id}
            type="button"
            data-pan-block="true"
            className={`edge-event-chip${isSelected ? " edge-event-chip--selected" : ""}`}
            style={{ left: entry.point.x, top: entry.point.y }}
            onClick={(event) => {
              event.stopPropagation();
              onEdgeClick?.(entry.edge.id);
            }}
            title={`bt:${entry.meta.backtraceId}`}
          >
            {entry.meta.eventLabel}
          </button>
        );
      })}
      {selectedEntry && (
        <div
          data-pan-block="true"
          data-scroll-block="true"
          className="edge-event-inspector"
          style={{ left: selectedEntry.point.x, top: selectedEntry.point.y + 20 }}
          onClick={(event) => event.stopPropagation()}
          role="dialog"
          aria-label="Edge event details"
        >
          <div className="edge-event-inspector__header">
            <span className="edge-event-inspector__title">{selectedEntry.meta.eventLabel}</span>
            <span className="edge-event-inspector__bt">bt:{selectedEntry.meta.backtraceId}</span>
          </div>
          <BacktraceDisplay
            frames={displayFrames}
            allFrames={selectedEntry.meta.allFrames}
            framesLoading={selectedEntry.meta.framesLoading}
            showSource={true}
          />
        </div>
      )}
    </div>
  );
}

function midpointAlongPolyline(polyline: Point[]): Point {
  if (polyline.length === 0) return { x: 0, y: 0 };
  if (polyline.length === 1) return polyline[0];

  const segmentLengths: number[] = [];
  let totalLength = 0;
  for (let i = 1; i < polyline.length; i++) {
    const dx = polyline[i].x - polyline[i - 1].x;
    const dy = polyline[i].y - polyline[i - 1].y;
    const length = Math.hypot(dx, dy);
    segmentLengths.push(length);
    totalLength += length;
  }

  if (totalLength <= 0) {
    return polyline[Math.floor(polyline.length / 2)] ?? polyline[0];
  }

  const halfway = totalLength / 2;
  let traversed = 0;

  for (let i = 1; i < polyline.length; i++) {
    const segmentLength = segmentLengths[i - 1];
    if (traversed + segmentLength < halfway) {
      traversed += segmentLength;
      continue;
    }
    const t = (halfway - traversed) / segmentLength;
    return {
      x: polyline[i - 1].x + (polyline[i].x - polyline[i - 1].x) * t,
      y: polyline[i - 1].y + (polyline[i].y - polyline[i - 1].y) * t,
    };
  }

  return polyline[polyline.length - 1];
}

function edgeEventMetaFromGeometryEdge(edge: GeometryEdge): EdgeEventMeta | null {
  const data = edge.data as Record<string, unknown> | null | undefined;
  if (!data) return null;

  const eventLabel = typeof data.eventLabel === "string" ? data.eventLabel : null;
  const backtraceId = typeof data.backtraceId === "number" ? data.backtraceId : null;
  if (!eventLabel || backtraceId == null) return null;

  return {
    eventLabel,
    backtraceId,
    frames: parseGraphFrameArray(data.frames),
    allFrames: parseGraphFrameArray(data.allFrames),
    framesLoading: data.framesLoading === true,
  };
}

function parseGraphFrameArray(raw: unknown): GraphFrameData[] {
  if (!Array.isArray(raw)) return [];
  const out: GraphFrameData[] = [];
  for (const value of raw) {
    const frame = parseGraphFrame(value);
    if (frame) out.push(frame);
  }
  return out;
}

function parseGraphFrame(value: unknown): GraphFrameData | null {
  if (typeof value !== "object" || value == null) return null;
  const frame = value as Record<string, unknown>;
  if (typeof frame.function_name !== "string") return null;
  if (typeof frame.source_file !== "string") return null;

  return {
    function_name: frame.function_name,
    source_file: frame.source_file,
    line: typeof frame.line === "number" ? frame.line : undefined,
    frame_id: typeof frame.frame_id === "number" ? frame.frame_id : undefined,
  };
}
