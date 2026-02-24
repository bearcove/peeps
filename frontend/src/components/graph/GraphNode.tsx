import React, { useState } from "react";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import { kindIcon } from "../../nodeKindSpec";
import { type EntityDef, type RenderTopFrame, type Tone } from "../../snapshot";
import type { GraphFilterLabelMode } from "../../graphFilter";
import { useSourceLine } from "../../api/useSourceLine";
import "./GraphNode.css";

export type GraphFrameData = {
  function_name: string;
  source_file: string;
  line?: number;
  frame_id?: number;
};

export type GraphNodeData = {
  kind: string;
  label: string;
  inCycle: boolean;
  selected: boolean;
  status?: { label: string; tone: Tone };
  ageMs?: number;
  stat?: string;
  statTone?: Tone;
  scopeRgbLight?: string;
  scopeRgbDark?: string;
  ghost?: boolean;
  sublabel?: string;
  /** All non-system frames from the backtrace. */
  frames: GraphFrameData[];
  /** The entity's own crate, used to pick the "main crate" frame. */
  entityCrate?: string;
  /** Whether to show source lines (controlled by the graph panel's showSource toggle). */
  showSource?: boolean;
};

function frameDataFromRenderTopFrame(f: RenderTopFrame): GraphFrameData {
  return {
    function_name: f.function_name,
    source_file: f.source_file,
    line: f.line,
    frame_id: f.frame_id,
  };
}

export function graphNodeDataFromEntity(def: EntityDef): GraphNodeData {
  const frames = def.frames.map(frameDataFromRenderTopFrame);
  if (def.channelPair) {
    return {
      kind: "channel_pair",
      label: def.name,
      inCycle: def.inCycle,
      selected: false,
      status: def.status,
      ageMs: def.ageMs,
      stat: def.stat,
      statTone: def.statTone,
      frames,
      entityCrate: def.krate,
    };
  }
  if (def.rpcPair) {
    const respBody =
      "response" in def.rpcPair.resp.body
        ? def.rpcPair.resp.body.response
        : null;
    const respStatus = respBody?.status;
    const respStatusKey = respStatus == null ? "pending" : typeof respStatus === "string" ? respStatus : Object.keys(respStatus)[0];
    const respTone: Tone = respStatus == null || typeof respStatus === "string" ? "warn" : "ok" in respStatus ? "ok" : "error" in respStatus ? "crit" : "warn";
    return {
      kind: "rpc_pair",
      label: def.name,
      inCycle: def.inCycle,
      selected: false,
      status: def.status,
      ageMs: def.rpcPair.resp.ageMs,
      stat: `RESP ${respStatusKey}`,
      statTone: respTone,
      frames,
      entityCrate: def.krate,
    };
  }
  return {
    kind: def.kind,
    label: def.name,
    inCycle: def.inCycle,
    selected: false,
    status: def.status,
    ageMs: def.ageMs,
    stat: def.stat,
    statTone: def.statTone,
    frames,
    entityCrate: def.krate,
  };
}

export function computeNodeSublabel(def: EntityDef, labelBy: GraphFilterLabelMode): string {
  if (labelBy === "crate") return def.topFrame?.crate_name ?? "";
  if (labelBy === "process") return def.processName;
  // location
  if (!def.topFrame) return "";
  return def.topFrame.line != null
    ? `${def.topFrame.source_file}:${def.topFrame.line}`
    : def.topFrame.source_file;
}

/** Pick which frames to show in collapsed mode:
 *  1. First non-system frame (index 0, same as topFrame)
 *  2. First frame from the entity's main crate (if different from #1)
 */
function pickCollapsedFrames(frames: GraphFrameData[], entityCrate?: string): GraphFrameData[] {
  if (frames.length === 0) return [];
  const first = frames[0];
  if (!entityCrate || frames.length === 1) return [first];

  const mainCrateFrame = frames.find((f) => {
    const crate = f.function_name.split("::")[0]?.trim();
    return crate === entityCrate;
  });

  if (!mainCrateFrame || mainCrateFrame === first) return [first];
  return [first, mainCrateFrame];
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

  if (sourceHtml) {
    return (
      <pre
        className="graph-node-frame arborium-hl"
        dangerouslySetInnerHTML={{ __html: sourceHtml }}
      />
    );
  }

  return (
    <div className="graph-node-frame graph-node-frame--text">
      <span className="graph-node-frame-fn">{shortFnName(frame.function_name)}</span>
      <span className="graph-node-frame-dot">&middot;</span>
      <span className="graph-node-frame-loc">{location}</span>
    </div>
  );
}

export function GraphNode({ data }: { data: GraphNodeData }) {
  const showScopeColor = data.scopeRgbLight !== undefined && data.scopeRgbDark !== undefined && !data.inCycle;
  const [expanded, setExpanded] = useState(false);

  const collapsedFrames = pickCollapsedFrames(data.frames, data.entityCrate);
  const visibleFrames = expanded ? data.frames : collapsedFrames;
  const canExpand = data.frames.length > collapsedFrames.length;

  const topFrame = data.frames[0];
  const topLocation = topFrame ? formatFileLocation(topFrame) : undefined;

  return (
    <div
      className={[
        "graph-card",
        "graph-node",
        data.inCycle && "graph-node--cycle",
        data.selected && "graph-card--selected",
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
        {topLocation && (
          <span className="graph-node-location">{topLocation}</span>
        )}
      </div>
      {data.sublabel && (
        <div className="graph-node-sublabel">{data.sublabel}</div>
      )}
      {/* Frame lines: click to expand/collapse */}
      {visibleFrames.length > 0 && (
        <div
          className={["graph-node-frames", canExpand && "graph-node-frames--expandable"].filter(Boolean).join(" ")}
          onClick={canExpand ? (e) => { e.stopPropagation(); setExpanded(!expanded); } : undefined}
        >
          {visibleFrames.map((frame, i) => (
            <FrameLine key={frame.frame_id ?? i} frame={frame} showSource={data.showSource} />
          ))}
          {canExpand && !expanded && (
            <span className="graph-node-frames-hint">+{data.frames.length - collapsedFrames.length} frames</span>
          )}
        </div>
      )}
    </div>
  );
}
