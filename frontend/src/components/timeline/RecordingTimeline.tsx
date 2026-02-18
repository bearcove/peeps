import "./RecordingTimeline.css";
import type { FrameSummary } from "../../api/types";
import { CircleNotch } from "@phosphor-icons/react";

export function formatElapsed(ms: number): string {
  const totalSeconds = Math.floor(Math.abs(ms) / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

interface RecordingTimelineProps {
  frames: FrameSummary[];
  frameCount: number;
  currentFrameIndex: number;
  onScrub: (index: number) => void;
  /** When true, union layout is being built — show progress and disable slider. */
  buildingUnion?: boolean;
  /** [loaded, total] progress for union build. */
  buildProgress?: [number, number];
}

export function RecordingTimeline({
  frames,
  frameCount,
  currentFrameIndex,
  onScrub,
  buildingUnion,
  buildProgress,
}: RecordingTimelineProps) {
  const firstMs = frames[0]?.captured_at_unix_ms ?? 0;
  const currentMs = frames[currentFrameIndex]?.captured_at_unix_ms ?? firstMs;
  const elapsedMs = currentMs - firstMs;

  return (
    <div className="recording-timeline">
      {buildingUnion ? (
        <span className="recording-timeline-label recording-timeline-building">
          <CircleNotch size={12} weight="bold" className="spinning" />
          Stabilizing layout…{" "}
          {buildProgress && (
            <span className="recording-timeline-progress">
              {buildProgress[0]}/{buildProgress[1]}
            </span>
          )}
        </span>
      ) : (
        <span className="recording-timeline-label">
          Frame {currentFrameIndex + 1} / {frameCount}
        </span>
      )}
      <input
        type="range"
        min={0}
        max={frameCount - 1}
        value={currentFrameIndex}
        onChange={(e) => onScrub(Number(e.target.value))}
        className="recording-timeline-slider"
        disabled={buildingUnion}
      />
      <span className="recording-timeline-time">
        {formatElapsed(elapsedMs)}
      </span>
    </div>
  );
}
