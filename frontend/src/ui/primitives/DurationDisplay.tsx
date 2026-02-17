import type React from "react";

export type DurationTone = "neutral" | "ok" | "warn" | "crit";

export type DurationDisplayProps = {
  /** Duration in milliseconds */
  ms: number;
  /** Override automatic tone */
  tone?: DurationTone;
};

function formatDuration(ms: number): string {
  if (!isFinite(ms)) return "—";
  const totalMs = Math.max(0, ms);
  if (totalMs < 1000) {
    return `${totalMs}ms`;
  }

  const totalSeconds = totalMs / 1000;
  if (totalSeconds < 60) {
    if (totalSeconds < 10) {
      return `${Number(totalSeconds.toFixed(1))}s`;
    }
    return `${Math.floor(totalSeconds)}s`;
  }

  if (totalMs < 3600000) {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = Math.floor(totalSeconds % 60);
    return `${minutes}m${seconds}s`;
  }

  const hours = Math.floor(totalMs / 3600000);
  const minutes = Math.floor((totalMs % 3600000) / 60000);
  return `${hours}h${minutes}m`;
}

function pickTone(ms: number): DurationTone {
  if (ms < 5000) return "neutral";
  if (ms < 60000) return "ok";
  if (ms < 300000) return "warn";
  return "crit";
}

export function DurationDisplay({ ms, tone }: DurationDisplayProps) {
  const resolvedTone = tone ?? pickTone(ms);
  return (
    <span className={["ui-duration-display", `ui-duration-display--${resolvedTone}`].join(" ")}>
      {formatDuration(ms)}
    </span>
  );
}
