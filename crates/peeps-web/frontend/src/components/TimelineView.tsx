import { useMemo } from "react";
import { ClockCounterClockwise, FunnelSimple } from "@phosphor-icons/react";
import type { TimelineEvent, TimelineProcessOption } from "../types";

const WINDOW_OPTIONS_SECONDS = [30, 60, 300, 900, 1800] as const;

interface TimelineViewProps {
  events: TimelineEvent[];
  loading: boolean;
  error: string | null;
  selectedEventId: string | null;
  selectedProcKey: string | null;
  processOptions: TimelineProcessOption[];
  windowSeconds: number;
  snapshotCapturedAtNs: number | null;
  onSelectProcKey: (procKey: string | null) => void;
  onWindowSecondsChange: (seconds: number) => void;
  onRefresh: () => void;
  onSelectEvent: (event: TimelineEvent) => void;
}

type TimelineGroup = {
  key: string;
  label: string;
  kind: "correlation" | "entity";
  events: TimelineEvent[];
  latestTsNs: number;
  earliestTsNs: number;
};

function formatRelativeNs(tsNs: number, snapshotCapturedAtNs: number | null): string {
  if (snapshotCapturedAtNs == null) {
    return `${Math.round(tsNs / 1_000_000)}ms`;
  }
  const deltaNs = snapshotCapturedAtNs - tsNs;
  const sign = deltaNs >= 0 ? "-" : "+";
  const absNs = Math.abs(deltaNs);
  if (absNs >= 1_000_000_000) {
    return `${sign}${(absNs / 1_000_000_000).toFixed(3)}s`;
  }
  return `${sign}${Math.round(absNs / 1_000_000)}ms`;
}

function clamp01(value: number): number {
  if (value <= 0) return 0;
  if (value >= 1) return 1;
  return value;
}

function timelinePositionPct(tsNs: number, startNs: number, endNs: number): number {
  if (endNs <= startNs) return 100;
  return clamp01((tsNs - startNs) / (endNs - startNs)) * 100;
}

function formatAbsoluteTs(tsNs: number): string {
  if (!Number.isFinite(tsNs)) return "—";
  const date = new Date(Math.floor(tsNs / 1_000_000));
  const micros = Math.floor((tsNs % 1_000_000) / 1_000);
  return `${date.toLocaleTimeString()}.${String(micros).padStart(3, "0")}`;
}

function eventTone(name: string): "ok" | "warn" | "crit" | "neutral" {
  if (
    name.includes(".err") ||
    name.includes(".failed") ||
    name.includes(".cancelled") ||
    name.includes(".dropped")
  ) {
    return "crit";
  }
  if (
    name.includes(".try_") ||
    name.includes(".empty") ||
    name.includes(".blocked") ||
    name.includes(".waiting")
  ) {
    return "warn";
  }
  if (
    name.includes(".ok") ||
    name.includes(".send") ||
    name.includes(".recv") ||
    name.includes(".completed")
  ) {
    return "ok";
  }
  return "neutral";
}

function formatGroupLabel(event: TimelineEvent): { key: string; label: string; kind: "correlation" | "entity" } {
  if (event.correlation && event.correlation.length > 0) {
    return {
      key: `corr:${event.correlation}`,
      label: event.correlation,
      kind: "correlation",
    };
  }
  return {
    key: `entity:${event.entity_id}`,
    label: event.entity_id,
    kind: "entity",
  };
}

export function TimelineView({
  events,
  loading,
  error,
  selectedEventId,
  selectedProcKey,
  processOptions,
  windowSeconds,
  snapshotCapturedAtNs,
  onSelectProcKey,
  onWindowSecondsChange,
  onRefresh,
  onSelectEvent,
}: TimelineViewProps) {
  const groups = useMemo<TimelineGroup[]>(() => {
    const byKey = new Map<string, TimelineGroup>();

    for (const event of events) {
      const grouping = formatGroupLabel(event);
      const existing = byKey.get(grouping.key);
      if (existing) {
        existing.events.push(event);
        if (event.ts_ns > existing.latestTsNs) {
          existing.latestTsNs = event.ts_ns;
        }
        if (event.ts_ns < existing.earliestTsNs) {
          existing.earliestTsNs = event.ts_ns;
        }
        continue;
      }

      byKey.set(grouping.key, {
        key: grouping.key,
        label: grouping.label,
        kind: grouping.kind,
        events: [event],
        latestTsNs: event.ts_ns,
        earliestTsNs: event.ts_ns,
      });
    }

    const grouped = Array.from(byKey.values());
    for (const group of grouped) {
      group.events.sort((a, b) => b.ts_ns - a.ts_ns || b.id.localeCompare(a.id));
    }
    grouped.sort((a, b) => b.latestTsNs - a.latestTsNs || b.events.length - a.events.length);
    return grouped;
  }, [events]);

  const endNs = snapshotCapturedAtNs ?? Date.now() * 1_000_000;
  const windowNs = Math.max(1, Math.round(windowSeconds * 1_000_000_000));
  const startNs = Math.max(0, endNs - windowNs);

  return (
    <div className="panel panel--timeline">
      <div className="panel-header">
        <ClockCounterClockwise size={14} weight="bold" /> Timeline
        <span className="timeline-count">{events.length.toLocaleString()} event(s)</span>
        <button className="btn" onClick={onRefresh} disabled={loading}>
          Refresh
        </button>
      </div>

      <div className="timeline-controls">
        <label className="timeline-control timeline-control--grow">
          <span><FunnelSimple size={12} weight="bold" /> Process</span>
          <select
            value={selectedProcKey ?? ""}
            onChange={(e) => onSelectProcKey(e.target.value === "" ? null : e.target.value)}
          >
            <option value="">All processes</option>
            {processOptions.map((opt) => (
              <option key={opt.proc_key} value={opt.proc_key}>
                {opt.process} ({opt.proc_key})
              </option>
            ))}
          </select>
        </label>

        <label className="timeline-control">
          <span>Window</span>
          <select
            value={String(windowSeconds)}
            onChange={(e) => onWindowSecondsChange(Number(e.target.value))}
          >
            {WINDOW_OPTIONS_SECONDS.map((seconds) => (
              <option key={seconds} value={seconds}>
                {seconds >= 60 ? `${Math.round(seconds / 60)}m` : `${seconds}s`}
              </option>
            ))}
          </select>
        </label>
      </div>

      {error && <div className="timeline-error">{error}</div>}

      <div className="timeline-body">
        <div className="timeline-axis">
          <span>{`-${windowSeconds >= 60 ? `${Math.round(windowSeconds / 60)}m` : `${windowSeconds}s`}`}</span>
          <span>-mid</span>
          <span>now</span>
        </div>
        {loading ? (
          <div className="timeline-empty">Loading timeline events...</div>
        ) : groups.length === 0 ? (
          <div className="timeline-empty">No events in this process/window scope.</div>
        ) : (
          groups.map((group) => (
            <section key={group.key} className="timeline-group">
              <header className="timeline-group-header">
                <span className="timeline-group-kind">{group.kind === "correlation" ? "corr" : "entity"}</span>
                <span className="timeline-group-label" title={group.label}>{group.label}</span>
                <span className="timeline-group-count">{group.events.length}</span>
              </header>
              <div className="timeline-lane">
                <div className="timeline-lane-track" />
                <div
                  className="timeline-lane-span"
                  style={{
                    left: `${timelinePositionPct(group.earliestTsNs, startNs, endNs)}%`,
                    right: `${100 - timelinePositionPct(group.latestTsNs, startNs, endNs)}%`,
                  }}
                />
                {group.events.map((event) => (
                  <button
                    key={event.id}
                    className={`timeline-event-dot timeline-event-dot--${eventTone(event.name)}${
                      selectedEventId === event.id ? " timeline-event-dot--active" : ""
                    }`}
                    onClick={() => onSelectEvent(event)}
                    title={`${event.name}
${formatRelativeNs(event.ts_ns, snapshotCapturedAtNs)} from now
${formatAbsoluteTs(event.ts_ns)}
entity=${event.entity_id}`}
                    style={{
                      left: `${timelinePositionPct(event.ts_ns, startNs, endNs)}%`,
                    }}
                  >
                    <span className="timeline-event-dot-core" />
                  </button>
                ))}
              </div>
              <div className="timeline-group-meta">
                <span className="timeline-group-last">{group.events[0]?.name ?? "—"}</span>
                <span className="timeline-group-when">
                  {formatRelativeNs(group.latestTsNs, snapshotCapturedAtNs)}
                </span>
              </div>
            </section>
          ))
        )}
      </div>
    </div>
  );
}
