import React, { useEffect, useMemo, useState } from "react";
import { Stack, ArrowSquareOut } from "@phosphor-icons/react";
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
  if (!isResolved(frame)) return false;
  const modulePath = frame.resolved.module_path;
  return SYSTEM_PREFIXES.some((prefix) => modulePath.startsWith(prefix));
}

// Index is intentional: a backtrace can contain the same frame multiple times (recursion).
// eslint-disable-next-line react/no-array-index-key -- index disambiguates recursive frames
function frameKey(frame: SnapshotBacktraceFrame, index: number): string {
  if (isResolved(frame)) {
    return `r:${index}:${frame.resolved.module_path}:${frame.resolved.function_name}:${frame.resolved.source_file}:${frame.resolved.line ?? ""}`;
  }
  return `u:${index}:${frame.unresolved.module_path}:${frame.unresolved.rel_pc}`;
}

function shortFileName(path: string): string {
  return path.split("/").pop() ?? path;
}

function zedUrl(path: string, line?: number): string {
  return line != null ? `zed://file${path}:${line}` : `zed://file${path}`;
}

function shortFunctionName(name: string): string {
  // Strip generic parameters: foo::bar::<T>::baz -> foo::bar::baz
  const stripped = name.replace(/<[^>]*>/g, "");
  // Take last two segments: some::long::path::func -> path::func
  const parts = stripped.split("::");
  if (parts.length <= 2) return stripped;
  return parts.slice(-2).join("::");
}

/** Compact badge for inline use in the inspector KV table. */
export function BacktraceBadge({
  backtrace,
  onClick,
}: {
  backtrace: ResolvedSnapshotBacktrace;
  onClick: () => void;
}) {
  const topFrame = useMemo(() => {
    return backtrace.frames.find((f) => isResolved(f) && !isSystemFrame(f))
      ?? backtrace.frames.find((f) => isResolved(f))
      ?? backtrace.frames[0];
  }, [backtrace.frames]);

  const location = topFrame && isResolved(topFrame)
    ? `${shortFileName(topFrame.resolved.source_file)}${topFrame.resolved.line != null ? `:${topFrame.resolved.line}` : ""}`
    : null;

  return (
    <button type="button" className="bt-badge" onClick={onClick}>
      <Stack size={12} weight="bold" className="bt-badge-icon" />
      {location ? (
        <span className="bt-badge-location">{location}</span>
      ) : (
        <span className="bt-badge-location bt-badge-location--pending">pending</span>
      )}
      <span className="bt-badge-count">{backtrace.frames.length}f</span>
    </button>
  );
}

/** Full backtrace panel, opened from the badge. */
export function BacktracePanel({
  backtrace,
  onClose,
}: {
  backtrace: ResolvedSnapshotBacktrace;
  onClose: () => void;
}) {
  const [showSystem, setShowSystem] = useState(false);

  const userFrames = useMemo(
    () => backtrace.frames.filter((f) => isResolved(f) && !isSystemFrame(f)),
    [backtrace.frames],
  );
  const systemFrames = useMemo(
    () => backtrace.frames.filter((f) => isResolved(f) && isSystemFrame(f)),
    [backtrace.frames],
  );
  const unresolvedFrames = useMemo(
    () => backtrace.frames.filter((f) => !isResolved(f)),
    [backtrace.frames],
  );

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  return (
    <div className="bt-overlay" role="dialog" aria-modal="true" onClick={onClose}>
      <div className="bt-dialog" onClick={(event) => event.stopPropagation()}>
        <div className="bt-dialog-header">
          <span className="bt-dialog-title">
            Backtrace <span className="bt-dialog-id">#{backtrace.backtrace_id}</span>
          </span>
          <span className="bt-dialog-meta">
            {backtrace.frames.length} frames
            {unresolvedFrames.length > 0 && (
              <> · {unresolvedFrames.length} unresolved</>
            )}
          </span>
          <button type="button" className="bt-dialog-close" onClick={onClose}>
            Esc
          </button>
        </div>

        {userFrames.length > 0 && (
          <div className="bt-section">
            <div className="bt-section-label">User frames ({userFrames.length})</div>
            <div className="bt-frame-list">
              {userFrames.map((frame, index) => (
                // eslint-disable-next-line react/no-array-index-key -- index disambiguates recursive frames
                <FrameRow key={frameKey(frame, index)} frame={frame} />
              ))}
            </div>
          </div>
        )}

        <div className="bt-section-controls">
          <button
            type="button"
            className="bt-section-toggle"
            onClick={() => setShowSystem((v) => !v)}
          >
            {showSystem ? "Hide" : "Show"} system frames ({systemFrames.length})
          </button>
        </div>

        {showSystem && systemFrames.length > 0 && (
          <div className="bt-section bt-section--system">
            <div className="bt-section-label">System frames ({systemFrames.length})</div>
            <div className="bt-frame-list">
              {systemFrames.map((frame, index) => (
                // eslint-disable-next-line react/no-array-index-key -- index disambiguates recursive frames
                <FrameRow key={frameKey(frame, index)} frame={frame} />
              ))}
            </div>
          </div>
        )}

        {unresolvedFrames.length > 0 && (
          <div className="bt-section bt-section--unresolved">
            <div className="bt-section-label">Unresolved ({unresolvedFrames.length})</div>
            <div className="bt-frame-list">
              {unresolvedFrames.map((frame, index) => {
                if (!("unresolved" in frame)) return null;
                return (
                  // eslint-disable-next-line react/no-array-index-key -- index disambiguates recursive frames
                  <div className="bt-frame-row" key={frameKey(frame, index)}>
                    <span className="bt-fn bt-fn--unresolved">
                      {frame.unresolved.module_path}+0x{frame.unresolved.rel_pc.toString(16)}
                    </span>
                    <span className="bt-reason">{frame.unresolved.reason}</span>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function FrameRow({ frame }: { frame: SnapshotBacktraceFrame }) {
  if (!isResolved(frame)) return null;
  const { function_name, source_file, line } = frame.resolved;
  const hasSource = source_file.length > 0;
  const fileName = hasSource ? shortFileName(source_file) : null;
  const location = fileName != null ? (line != null ? `${fileName}:${line}` : fileName) : null;
  const href = hasSource ? zedUrl(source_file, line) : null;

  return (
    <div className="bt-frame-row">
      <span className="bt-fn" title={function_name}>{shortFunctionName(function_name)}</span>
      {location && href ? (
        <a className="bt-src" href={href} title={`Open ${source_file}${line != null ? `:${line}` : ""} in Zed`}>
          {location}
          <ArrowSquareOut size={10} weight="bold" className="bt-src-icon" />
        </a>
      ) : (
        <span className="bt-src bt-src--none">no source</span>
      )}
    </div>
  );
}

/**
 * Inline backtrace widget for inspector panels.
 * Renders a compact badge; clicking opens a full backtrace dialog.
 */
export function BacktraceRenderer({
  backtrace,
}: {
  backtrace: ResolvedSnapshotBacktrace;
}) {
  const [open, setOpen] = useState(false);

  return (
    <>
      <BacktraceBadge backtrace={backtrace} onClick={() => setOpen(true)} />
      {open && (
        <BacktracePanel backtrace={backtrace} onClose={() => setOpen(false)} />
      )}
    </>
  );
}
