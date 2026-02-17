import type React from "react";

export type RelativeTimestampTone = "neutral" | "ok" | "warn" | "crit";

export type RelativeTimestampProps = {
  /** "P" for process-relative, "N" for node-relative */
  basis: "P" | "N";
  /** Human label for what the basis is, e.g. "process started" or "connection opened" */
  basisLabel: string;
  /** Absolute time of the basis point, ISO string or epoch ms */
  basisTime: string;
  /** Absolute time of this event */
  eventTime: string;
  /** Semantic tone for coloring */
  tone?: RelativeTimestampTone;
};

function parseTime(value: string): number {
  const trimmed = value.trim();
  if (/^\d+$/.test(trimmed)) {
    const maybeMs = Number(trimmed);
    if (!Number.isNaN(maybeMs)) return maybeMs;
  }
  return Date.parse(trimmed);
}

function formatDelta(ms: number): string {
  const totalMs = Math.max(0, ms);
  if (totalMs < 1000) {
    return `${totalMs}ms`;
  }

  const totalSeconds = totalMs / 1000;
  if (totalSeconds < 60) {
    return `${Math.trunc(totalSeconds)}s`;
  }

  if (totalMs < 3600000) {
    const totalMinutes = Math.floor(totalSeconds / 60);
    const remainingSeconds = Math.floor(totalSeconds % 60);
    return `${totalMinutes}m ${remainingSeconds}s`;
  }

  const hours = Math.floor(totalMs / 3600000);
  const remainingMinutes = Math.floor((totalMs % 3600000) / 60000);
  return `${hours}h ${remainingMinutes}m`;
}

export function RelativeTimestamp({
  basis,
  basisLabel,
  basisTime,
  eventTime,
  tone,
}: RelativeTimestampProps) {
  const basisMs = parseTime(basisTime);
  const eventMs = parseTime(eventTime);
  const deltaMs = eventMs - basisMs;
  const prefixSign = deltaMs < 0 ? "-" : "+";
  const formattedDelta = formatDelta(Math.abs(deltaMs));

  const basisAbs = Number.isFinite(basisMs) ? new Date(basisMs).toLocaleString() : basisTime;
  const eventAbs = Number.isFinite(eventMs) ? new Date(eventMs).toLocaleString() : eventTime;

  const title = `${basis} (${basisLabel}) = ${basisAbs}\nThis event = ${eventAbs}\nDelta = ${basis}${prefixSign}${formattedDelta}`;

  return (
    <span
      className={["ui-relative-timestamp", tone && `ui-relative-timestamp--${tone}`].filter(Boolean).join(" ")}
      title={title}
    >
      <span className="ui-relative-timestamp__prefix">
        {basis}
      </span>
      <span className="ui-relative-timestamp__value">
        {prefixSign}
        {formattedDelta}
      </span>
    </span>
  );
}
