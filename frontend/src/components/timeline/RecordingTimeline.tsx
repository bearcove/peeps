import "./RecordingTimeline.css";
import type { FrameSummary } from "../../api/types";
import type { FrameChangeSummary } from "../../recording/unionGraph";
import { CircleNotch, Ghost, SkipBack, SkipForward } from "@phosphor-icons/react";
import { ActionButton } from "../../ui/primitives/ActionButton";

export function formatElapsed(ms: number): string {
  const totalSeconds = Math.floor(Math.abs(ms) / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function formatMs(ms: number): string {
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(1)}s`;
  }
  return `${ms.toFixed(1)}ms`;
}

function formatChangeSummary(s: FrameChangeSummary): string {
  const parts: string[] = [];
  if (s.nodesAdded > 0) parts.push(`+${s.nodesAdded} ${s.nodesAdded === 1 ? "node" : "nodes"}`);
  if (s.nodesRemoved > 0)
    parts.push(`-${s.nodesRemoved} ${s.nodesRemoved === 1 ? "node" : "nodes"}`);
  if (s.edgesAdded > 0) parts.push(`+${s.edgesAdded} ${s.edgesAdded === 1 ? "edge" : "edges"}`);
  if (s.edgesRemoved > 0)
    parts.push(`-${s.edgesRemoved} ${s.edgesRemoved === 1 ? "edge" : "edges"}`);
  return parts.join(", ");
}

const DOWNSAMPLE_OPTIONS: { value: number; label: string }[] = [
  { value: 1, label: "All frames" },
  { value: 2, label: "Every 2nd" },
  { value: 5, label: "Every 5th" },
  { value: 10, label: "Every 10th" },
];

interface RecordingTimelineProps {
  frames: FrameSummary[];
  frameCount: number;
  currentFrameIndex: number;
  onScrub: (index: number) => void;
  /** When true, union layout is being built — show progress and disable slider. */
  buildingUnion?: boolean;
  /** [loaded, total] progress for union build. */
  buildProgress?: [number, number];
  /** Change summary for the current frame. */
  changeSummary?: FrameChangeSummary;
  /** Sorted list of frame indices where the graph changed. */
  changeFrames?: number[];
  avgCaptureMs?: number;
  maxCaptureMs?: number;
  totalCaptureMs?: number;
  ghostMode?: boolean;
  onGhostToggle?: () => void;
  /** Number of frames actually processed in the union (may differ from frameCount when downsampling). */
  processedFrameCount?: number;
  /** Current downsample interval (1 = all frames). */
  downsampleInterval?: number;
  /** Called when user changes the downsample interval. */
  onDownsampleChange?: (interval: number) => void;
  /** When true, show a "Rebuild" button to rebuild the union with the new interval. */
  canRebuild?: boolean;
  /** Called when user clicks Rebuild. */
  onRebuild?: () => void;
}

export function RecordingTimeline({
  frames,
  frameCount,
  currentFrameIndex,
  onScrub,
  buildingUnion,
  buildProgress,
  changeSummary,
  changeFrames,
  avgCaptureMs,
  maxCaptureMs,
  totalCaptureMs,
  ghostMode,
  onGhostToggle,
  processedFrameCount,
  downsampleInterval,
  onDownsampleChange,
  canRebuild,
  onRebuild,
}: RecordingTimelineProps) {
  const firstMs = frames[0]?.captured_at_unix_ms ?? 0;
  const currentMs = frames[currentFrameIndex]?.captured_at_unix_ms ?? firstMs;
  const elapsedMs = currentMs - firstMs;
  const deltaText = changeSummary ? formatChangeSummary(changeSummary) : "";
  const hasStats =
    avgCaptureMs !== undefined &&
    maxCaptureMs !== undefined &&
    totalCaptureMs !== undefined;

  const prevChangeFrame = changeFrames
    ? changeFrames.filter((f) => f < currentFrameIndex).at(-1)
    : undefined;
  const nextChangeFrame = changeFrames
    ? changeFrames.find((f) => f > currentFrameIndex)
    : undefined;

  const isDownsampled =
    processedFrameCount !== undefined && processedFrameCount < frameCount;

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
          {isDownsampled && (
            <span className="recording-timeline-processed"> · {processedFrameCount} proc</span>
          )}
          {deltaText && (
            <span className="recording-timeline-delta">{deltaText}</span>
          )}
        </span>
      )}
      <ActionButton
        size="sm"
        variant="ghost"
        isDisabled={buildingUnion || prevChangeFrame === undefined}
        onPress={() => prevChangeFrame !== undefined && onScrub(prevChangeFrame)}
        aria-label="Prev change"
      >
        <SkipBack size={14} weight="bold" />
      </ActionButton>
      <input
        type="range"
        min={0}
        max={frameCount - 1}
        value={currentFrameIndex}
        onChange={(e) => onScrub(Number(e.target.value))}
        className="recording-timeline-slider"
        disabled={buildingUnion}
      />
      <ActionButton
        size="sm"
        variant="ghost"
        isDisabled={buildingUnion || nextChangeFrame === undefined}
        onPress={() => nextChangeFrame !== undefined && onScrub(nextChangeFrame)}
        aria-label="Next change"
      >
        <SkipForward size={14} weight="bold" />
      </ActionButton>
      {!buildingUnion && onGhostToggle && (
        <button
          type="button"
          className={`recording-timeline-ghost-btn${ghostMode ? " recording-timeline-ghost-btn--active" : ""}`}
          onClick={onGhostToggle}
          title="Ghost mode: dim non-active nodes"
        >
          <Ghost size={14} weight={ghostMode ? "fill" : "regular"} />
        </button>
      )}
      <span className="recording-timeline-time">
        {formatElapsed(elapsedMs)}
      </span>
      {onDownsampleChange && frameCount >= 100 && (
        <div className="recording-timeline-downsample">
          <select
            className="recording-timeline-select"
            value={downsampleInterval ?? 1}
            onChange={(e) => onDownsampleChange(Number(e.target.value))}
            disabled={buildingUnion}
          >
            {DOWNSAMPLE_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
          {canRebuild && onRebuild && (
            <ActionButton size="sm" variant="default" onPress={onRebuild}>
              Rebuild
            </ActionButton>
          )}
        </div>
      )}
      {hasStats && (
        <span className="recording-timeline-stats">
          Avg {formatMs(avgCaptureMs!)} · Max {formatMs(maxCaptureMs!)} · Total{" "}
          {formatMs(totalCaptureMs!)}
        </span>
      )}
    </div>
  );
}
