import React, { useMemo, useState } from "react";
import type { SnapshotBacktraceFrame } from "../../api/types.generated";
import type { ResolvedSnapshotBacktrace } from "../../snapshot";
import "./BacktraceRenderer.css";

const SYSTEM_PREFIXES = [
  "std::",
  "core::",
  "alloc::",
  "tokio::",
  "tokio_util::",
  "futures::",
  "futures_core::",
  "futures_util::",
  "moire::",
  "moire_trace_capture::",
];

function isResolved(frame: SnapshotBacktraceFrame): frame is { resolved: { module_path: string; function_name: string; source_file: string; line?: number } } {
  return "resolved" in frame;
}

function isSystemFrame(frame: SnapshotBacktraceFrame): boolean {
  const modulePath = isResolved(frame) ? frame.resolved.module_path : frame.unresolved.module_path;
  return SYSTEM_PREFIXES.some((prefix) => modulePath.startsWith(prefix));
}

function frameKey(frame: SnapshotBacktraceFrame, index: number): string {
  if (isResolved(frame)) {
    return `r:${index}:${frame.resolved.module_path}:${frame.resolved.function_name}:${frame.resolved.source_file}:${frame.resolved.line ?? ""}`;
  }
  return `u:${index}:${frame.unresolved.module_path}:${frame.unresolved.rel_pc}`;
}

function frameText(frame: SnapshotBacktraceFrame): string {
  if (isResolved(frame)) {
    return [
      frame.resolved.function_name,
      frame.resolved.module_path,
      frame.resolved.source_file,
      frame.resolved.line != null ? String(frame.resolved.line) : "",
    ]
      .join(" ")
      .toLowerCase();
  }
  return [
    frame.unresolved.module_path,
    String(frame.unresolved.rel_pc),
    frame.unresolved.reason,
  ]
    .join(" ")
    .toLowerCase();
}

function renderFrameLabel(frame: SnapshotBacktraceFrame): React.ReactNode {
  if (isResolved(frame)) {
    const location = frame.resolved.line != null
      ? `${frame.resolved.source_file}:${frame.resolved.line}`
      : frame.resolved.source_file;
    return (
      <>
        <span className="bt-frame-fn">{frame.resolved.function_name}</span>
        <span className="bt-frame-meta">{frame.resolved.module_path}</span>
        <span className="bt-frame-src">{location}</span>
      </>
    );
  }

  return (
    <>
      <span className="bt-frame-fn bt-frame-fn--unresolved">unresolved</span>
      <span className="bt-frame-meta">{frame.unresolved.module_path}+0x{frame.unresolved.rel_pc.toString(16)}</span>
      <span className="bt-frame-src">{frame.unresolved.reason}</span>
    </>
  );
}

export function BacktraceRenderer({
  backtrace,
  title = "Backtrace",
}: {
  backtrace: ResolvedSnapshotBacktrace;
  title?: string;
}) {
  const [showUserFrames, setShowUserFrames] = useState(false);
  const [showSystemFrames, setShowSystemFrames] = useState(false);
  const [includeFilter, setIncludeFilter] = useState("");
  const [excludeFilter, setExcludeFilter] = useState("");

  const topFrame = useMemo(() => {
    return backtrace.frames.find((frame) => !isSystemFrame(frame)) ?? backtrace.frames[0];
  }, [backtrace.frames]);

  const userFrames = useMemo(
    () => backtrace.frames.filter((frame) => !isSystemFrame(frame)),
    [backtrace.frames],
  );
  const systemFrames = useMemo(
    () => backtrace.frames.filter((frame) => isSystemFrame(frame)),
    [backtrace.frames],
  );

  const includeNeedle = includeFilter.trim().toLowerCase();
  const excludeNeedle = excludeFilter.trim().toLowerCase();
  const filtered = useMemo(() => {
    const keep = (frame: SnapshotBacktraceFrame) => {
      const hay = frameText(frame);
      if (includeNeedle && !hay.includes(includeNeedle)) return false;
      if (excludeNeedle && hay.includes(excludeNeedle)) return false;
      return true;
    };
    return {
      user: userFrames.filter(keep),
      system: systemFrames.filter(keep),
    };
  }, [userFrames, systemFrames, includeNeedle, excludeNeedle]);

  return (
    <section className="bt-panel" aria-label={`${title} ${backtrace.backtrace_id}`}>
      <div className="bt-header">
        <div className="bt-title">
          <span>{title}</span>
          <span className="bt-id">#{backtrace.backtrace_id}</span>
        </div>
        <span className="bt-count">{backtrace.frames.length} frames</span>
      </div>

      {topFrame && (
        <div className="bt-top">
          <div className="bt-top-label">Top frame</div>
          <div className="bt-frame">{renderFrameLabel(topFrame)}</div>
        </div>
      )}

      <div className="bt-filters">
        <input
          className="bt-input"
          value={includeFilter}
          onChange={(event) => setIncludeFilter(event.target.value)}
          placeholder="include filter (function/module/path)"
          aria-label="Include frame filter"
        />
        <input
          className="bt-input"
          value={excludeFilter}
          onChange={(event) => setExcludeFilter(event.target.value)}
          placeholder="exclude filter"
          aria-label="Exclude frame filter"
        />
      </div>

      <div className="bt-controls">
        <button type="button" className="bt-toggle" onClick={() => setShowUserFrames((v) => !v)}>
          {showUserFrames ? "Hide user frames" : "Show user frames"} ({filtered.user.length})
        </button>
        {showUserFrames && (
          <button type="button" className="bt-toggle" onClick={() => setShowSystemFrames((v) => !v)}>
            {showSystemFrames ? "Hide system frames" : "Show system frames"} ({filtered.system.length})
          </button>
        )}
      </div>

      {showUserFrames && (
        <div className="bt-list">
          {filtered.user.map((frame, index) => (
            <div className="bt-frame" key={frameKey(frame, index)}>
              {renderFrameLabel(frame)}
            </div>
          ))}
        </div>
      )}

      {showUserFrames && showSystemFrames && (
        <div className="bt-list bt-list--system">
          {filtered.system.map((frame, index) => (
            <div className="bt-frame" key={frameKey(frame, index)}>
              {renderFrameLabel(frame)}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
