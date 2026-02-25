import { type EntityDef, type RenderTopFrame, type Tone } from "../../snapshot";
import type { GraphFilterLabelMode } from "../../graphFilter";

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
  /** True while symbolication is in progress and no user frames are resolved yet. */
  framesLoading: boolean;
  /** The entity's own crate, used to pick the "main crate" frame. */
  entityCrate?: string;
  /** Whether to show source lines (controlled by the graph panel's showSource toggle). */
  showSource?: boolean;
  /** Number of entry frames to skip when rendering (from FutureEntity.skip_entry_frames). */
  skipEntryFrames: number;
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
  const skipEntryFrames = "future" in def.body ? (def.body.future.skip_entry_frames ?? 0) : 0;
  const framesLoading = def.framesLoading;
  if (def.channelPair) {
    return {
      kind: "channel_pair",
      label: def.name,
      inCycle: def.inCycle,
      status: def.status,
      ageMs: def.ageMs,
      stat: def.stat,
      statTone: def.statTone,
      frames,
      framesLoading,
      entityCrate: def.krate,
      skipEntryFrames,
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
      status: def.status,
      ageMs: def.rpcPair.resp.ageMs,
      stat: `RESP ${respStatusKey}`,
      statTone: respTone,
      frames,
      framesLoading,
      entityCrate: def.krate,
      skipEntryFrames,
    };
  }
  return {
    kind: def.kind,
    label: def.name,
    inCycle: def.inCycle,
    status: def.status,
    ageMs: def.ageMs,
    stat: def.stat,
    statTone: def.statTone,
    frames,
    framesLoading,
    entityCrate: def.krate,
    skipEntryFrames,
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
