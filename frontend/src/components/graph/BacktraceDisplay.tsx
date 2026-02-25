import React, { useEffect, useMemo, useState } from "react";
import type { GraphFrameData } from "./graphNodeData";
import { FrameLine } from "./GraphNode";
import { cachedFetchSourcePreviews, getSourcePreviewSync } from "../../api/sourceCache";
import "./BacktraceDisplay.css";

export type BacktraceDisplayProps = {
  frames: GraphFrameData[];
  allFrames: GraphFrameData[];
  framesLoading: boolean;
  showSource?: boolean;
};

export function BacktraceDisplay({
  frames,
  allFrames,
  framesLoading,
  showSource,
}: BacktraceDisplayProps) {
  const [showSystem, setShowSystem] = useState(false);
  const [, setSourceVersion] = useState(0);

  const hasSystemFrames = allFrames.length > frames.length;
  const displayFrames = showSystem ? allFrames : frames;

  const frameIds = useMemo(
    () => displayFrames.map((f) => f.frame_id).filter((id): id is number => id != null),
    [displayFrames],
  );

  useEffect(() => {
    if (!showSource || frameIds.length === 0) return;
    const missing = frameIds.filter((id) => getSourcePreviewSync(id) == null);
    if (missing.length === 0) return;
    let cancelled = false;
    void cachedFetchSourcePreviews(missing).then(() => {
      if (!cancelled) setSourceVersion((v) => v + 1);
    });
    return () => {
      cancelled = true;
    };
  }, [frameIds, showSource]);

  if (displayFrames.length === 0) {
    if (!framesLoading) return null;
    return (
      <div className="graph-node-frames">
        <div className="graph-node-frame-section graph-node-frame-section--loading">
          <div className="graph-node-frame-sep graph-node-frame-sep--loading">
            <span className="graph-node-frame-sep__name">symbolicating…</span>
            <span className="graph-node-frame-sep__loc">loading source</span>
          </div>
          <pre className="graph-node-frame graph-node-frame--text graph-node-frame--fallback">…</pre>
        </div>
      </div>
    );
  }

  return (
    <>
      <div className="graph-node-frames">
        {displayFrames.map((frame, index) => (
          <FrameLine
            key={`${frame.frame_id ?? "none"}:${index}`}
            frame={frame}
            expanded={true}
            showSource={showSource}
          />
        ))}
      </div>
      {hasSystemFrames && (
        <div className="backtrace-toolbar" onClick={(e) => e.stopPropagation()}>
          <label className="backtrace-system-toggle">
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
