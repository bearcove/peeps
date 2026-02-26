import React from "react";
import type { GraphFrameData, GraphNodeData } from "./graphNodeData";
import { FrameLine } from "./GraphNode";
import { BacktraceDisplay } from "./BacktraceDisplay";
import "./FrameList.css";

type FrameListProps = {
  data: GraphNodeData;
  expanded: boolean;
  collapsedShowSource: boolean;
  collapsedFrameSlotCount: number;
  collapsedUseBacktraceDisplay?: boolean;
  activeFrameIndex?: number;
  /** Frames to show in collapsed mode (pre-sliced by caller). */
  collapsedFrames: GraphFrameData[];
};

export function FrameList({
  data,
  expanded,
  collapsedShowSource,
  collapsedFrameSlotCount,
  collapsedUseBacktraceDisplay,
  activeFrameIndex,
  collapsedFrames,
}: FrameListProps) {
  if (!expanded) {
    if (collapsedFrames.length === 0) {
      if (data.framesLoading && collapsedShowSource && collapsedFrameSlotCount > 0) {
        return (
          <div className="graph-node-frames">
            <div className="graph-node-frame-section graph-node-frame-section--loading">
              <div className="graph-node-frame-sep graph-node-frame-sep--loading">
                <span className="graph-node-frame-sep__name">symbolicating…</span>
                <span className="graph-node-frame-sep__loc">loading source</span>
              </div>
              <pre className="graph-node-frame graph-node-frame--text graph-node-frame--fallback">
                …
              </pre>
            </div>
          </div>
        );
      }
      return null;
    }
    if (collapsedUseBacktraceDisplay && collapsedShowSource) {
      return (
        <div className="graph-node-frames">
          <BacktraceDisplay
            frames={collapsedFrames}
            allFrames={collapsedFrames}
            framesLoading={data.framesLoading}
            showSource={true}
            useCompactContext={true}
            hideLocation={true}
          />
        </div>
      );
    }
    return (
      <div className="graph-node-frames">
        {collapsedFrames.map((frame) => (
          <FrameLine
            key={frame.frame_id}
            frame={frame}
            expanded={false}
            showSource={collapsedShowSource}
          />
        ))}
      </div>
    );
  }

  // Expanded mode — delegate to BacktraceDisplay
  const skip = data.skipEntryFrames;
  const frames = skip > 0 ? data.frames.slice(skip) : data.frames;
  const allFrames = skip > 0 ? data.allFrames.slice(skip) : data.allFrames;

  return (
    <div className="graph-node-frames-scroll">
      <BacktraceDisplay
        frames={frames}
        allFrames={allFrames}
        framesLoading={data.framesLoading}
        showSource={data.showSource}
        activeFrameIndex={activeFrameIndex}
      />
    </div>
  );
}
