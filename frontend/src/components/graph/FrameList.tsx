import React, { useState } from "react";
import type { SourcePreviewResponse } from "../../api/types.generated";
import type { GraphFrameData, GraphNodeData } from "./graphNodeData";
import { FrameLineCollapsed, FrameLineExpanded } from "./GraphNode";
import "./FrameList.css";

type FrameListProps = {
  data: GraphNodeData;
  expanded: boolean;
  isFuture: boolean;
  collapsedShowSource: boolean;
  /** Frames to show in collapsed mode (pre-sliced by caller). */
  collapsedFrames: GraphFrameData[];
  sourcePreviewByFrameId?: Map<number, SourcePreviewResponse>;
};

export function FrameList({
  data,
  expanded,
  isFuture,
  collapsedShowSource,
  collapsedFrames,
  sourcePreviewByFrameId,
}: FrameListProps) {
  const [showSystem, setShowSystem] = useState(false);

  const hasSystemFrames = data.allFrames.length > data.frames.length;

  if (!expanded) {
    if (collapsedFrames.length === 0) {
      if (data.framesLoading && isFuture) {
        return (
          <div className="graph-node-frames graph-node-frames--loading">
            <div className="graph-node-frame-skeleton" />
          </div>
        );
      }
      return null;
    }
    return (
      <div className="graph-node-frames">
        {collapsedFrames.map((frame) => (
          <FrameLineCollapsed
            key={frame.frame_id}
            frame={frame}
            showSource={collapsedShowSource}
            preloadedPreview={
              frame.frame_id != null ? sourcePreviewByFrameId?.get(frame.frame_id) : undefined
            }
          />
        ))}
      </div>
    );
  }

  // Expanded mode
  const sourceFrames = showSystem ? data.allFrames : data.frames;
  const effectiveFrames =
    data.skipEntryFrames > 0 ? sourceFrames.slice(data.skipEntryFrames) : sourceFrames;

  return (
    <>
      <div className={hasSystemFrames ? "graph-node-frames-scroll" : undefined}>
        <div className="graph-node-frames">
          {effectiveFrames.map((frame) => (
            <FrameLineExpanded
              key={frame.frame_id}
              frame={frame}
              showSource={data.showSource}
              preloadedPreview={
                frame.frame_id != null ? sourcePreviewByFrameId?.get(frame.frame_id) : undefined
              }
            />
          ))}
        </div>
      </div>
      {hasSystemFrames && (
        <div className="frame-list-toolbar" onClick={(e) => e.stopPropagation()}>
          <label className="frame-list-system-toggle">
            <input
              type="checkbox"
              checked={showSystem}
              onChange={(e) => setShowSystem(e.target.checked)}
            />
            Show system frames
          </label>
        </div>
      )}
    </>
  );
}
