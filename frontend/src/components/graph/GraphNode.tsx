import React from "react";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import { kindIcon } from "../../nodeKindSpec";
import { useSourceLine } from "../../api/useSourceLine";
import { langIcon } from "./langIcon";
import type { GraphFrameData, GraphNodeData } from "./graphNodeData";
import "./GraphNode.css";

function pickCollapsedFrames(frames: GraphFrameData[]): GraphFrameData[] {
  return frames.slice(0, 2);
}

function formatFileLocation(f: GraphFrameData): string {
  const file = f.source_file.split("/").pop() ?? f.source_file;
  return f.line != null ? `${file}:${f.line}` : file;
}

function shortFnName(fn_name: string): string {
  // Strip crate:: prefix and keep the last 2 segments
  const parts = fn_name.split("::");
  if (parts.length <= 2) return fn_name;
  return parts.slice(-2).join("::");
}

function FrameLine({ frame, showSource }: { frame: GraphFrameData; showSource?: boolean }) {
  const sourceHtml = useSourceLine(showSource ? frame.frame_id : undefined);
  const location = formatFileLocation(frame);

  return (
    <div className="graph-node-frame-row">
      {langIcon(frame.source_file, 10, "graph-node-frame-icon")}
      {sourceHtml ? (
        <pre
          className="graph-node-frame arborium-hl"
          dangerouslySetInnerHTML={{ __html: sourceHtml }}
        />
      ) : (
        <pre className="graph-node-frame graph-node-frame--text">
          <span className="graph-node-frame-fn">{shortFnName(frame.function_name)}</span>
          <span className="graph-node-frame-dot">&middot;</span>
          <span className="graph-node-frame-loc">{location}</span>
        </pre>
      )}
    </div>
  );
}

export function GraphNode({
  data,
  expanded = false,
}: {
  data: GraphNodeData;
  expanded?: boolean;
}) {
  const showScopeColor =
    data.scopeRgbLight !== undefined && data.scopeRgbDark !== undefined && !data.inCycle;

  const visibleFrames = expanded ? data.frames : pickCollapsedFrames(data.frames);

  const topFrame = data.frames[0];
  const topLocation = topFrame ? formatFileLocation(topFrame) : undefined;

  return (
    <div
      className={[
        "graph-card",
        "graph-node",
        expanded && "graph-node--expanded",
        data.inCycle && "graph-node--cycle",
        data.statTone === "crit" && "graph-card--stat-crit",
        data.statTone === "warn" && "graph-card--stat-warn",
        showScopeColor && "graph-card--scope",
        data.ghost && "graph-card--ghost",
      ]
        .filter(Boolean)
        .join(" ")}
      style={
        showScopeColor
          ? ({
              "--scope-rgb-light": data.scopeRgbLight,
              "--scope-rgb-dark": data.scopeRgbDark,
            } as React.CSSProperties)
          : undefined
      }
    >
      {/* Header row: icon + main info + file:line badge */}
      <div className="graph-node-header">
        <span className="graph-node-icon">{kindIcon(data.kind, 14)}</span>
        <div className="graph-node-main">
          <span className="graph-node-label">{data.label}</span>
          {(data.ageMs ?? 0) > 3000 && (
            <>
              <span className="graph-node-dot">&middot;</span>
              <DurationDisplay ms={data.ageMs ?? 0} />
            </>
          )}
          {data.stat && (
            <>
              <span className="graph-node-dot">&middot;</span>
              <span
                className={[
                  "graph-node-stat",
                  data.statTone === "crit" && "graph-node-stat--crit",
                  data.statTone === "warn" && "graph-node-stat--warn",
                ]
                  .filter(Boolean)
                  .join(" ")}
              >
                {data.stat}
              </span>
            </>
          )}
        </div>
        {topLocation && <span className="graph-node-location">{topLocation}</span>}
      </div>
      {data.sublabel && <div className="graph-node-sublabel">{data.sublabel}</div>}
      {visibleFrames.length > 0 && (
        <div className="graph-node-frames">
          {visibleFrames.map((frame, _i) => (
            <FrameLine key={frame.frame_id} frame={frame} showSource={data.showSource} />
          ))}
        </div>
      )}
    </div>
  );
}
