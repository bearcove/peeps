import React, { useEffect, useState } from "react";
import { createPortal } from "react-dom";
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
  return SYSTEM_PREFIXES.some((prefix) => frame.resolved.module_path.startsWith(prefix));
}

// Index is intentional: a backtrace can contain the same frame multiple times (recursion).
function frameKey(frame: SnapshotBacktraceFrame, index: number): string {
  if (isResolved(frame)) {
    return `r:${index}:${frame.resolved.module_path}:${frame.resolved.function_name}:${frame.resolved.source_file}:${frame.resolved.line ?? ""}`;
  }
  return `u:${index}:${frame.unresolved.module_path}:${frame.unresolved.rel_pc}`;
}

/** Split "some::module::func_name" into { prefix: "some::module::", fn: "func_name" },
 *  stripping generics but preserving the full path. */
function splitFunctionName(name: string): { prefix: string; fn: string } {
  const stripped = name.replace(/<[^>]*>/g, "");
  const lastSep = stripped.lastIndexOf("::");
  if (lastSep === -1) return { prefix: "", fn: stripped };
  return { prefix: stripped.slice(0, lastSep + 2), fn: stripped.slice(lastSep + 2) };
}

/** Show last 3 path components, e.g. "moire-trace-capture/src/lib.rs". */
function relativePath(path: string): string {
  const parts = path.split("/");
  return parts.slice(-3).join("/");
}

function zedUrl(path: string, line?: number): string {
  return line != null ? `zed://file${path}:${line}` : `zed://file${path}`;
}

/** Compact badge for inline use in the inspector KV table. */
export function BacktraceBadge({
  backtrace,
  onClick,
}: {
  backtrace: ResolvedSnapshotBacktrace;
  onClick: () => void;
}) {
  const topFrame = backtrace.frames.find((f) => isResolved(f) && !isSystemFrame(f))
    ?? backtrace.frames.find((f) => isResolved(f))
    ?? backtrace.frames[0];

  const location = topFrame && isResolved(topFrame)
    ? `${topFrame.resolved.source_file.split("/").pop() ?? topFrame.resolved.source_file}${topFrame.resolved.line != null ? `:${topFrame.resolved.line}` : ""}`
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

  const systemCount = backtrace.frames.filter((f) => isResolved(f) && isSystemFrame(f)).length;
  const unresolvedCount = backtrace.frames.filter((f) => !isResolved(f)).length;

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
          <span className="bt-dialog-title">Backtrace</span>
          <span className="bt-dialog-meta">
            {backtrace.frames.length} frames
            {unresolvedCount > 0 && <> · {unresolvedCount} unresolved</>}
          </span>
          <button
            type="button"
            className="bt-section-toggle"
            onClick={() => setShowSystem((v) => !v)}
          >
            {showSystem ? "Hide" : "Show"} system ({systemCount})
          </button>
          <button type="button" className="bt-dialog-close" onClick={onClose}>
            Esc
          </button>
        </div>

        <div className="bt-frame-list">
          {/* eslint-disable-next-line react/no-array-index-key -- index disambiguates recursive frames */}
          {backtrace.frames.map((frame, index) => {
            if (!isResolved(frame)) {
              return (
                // eslint-disable-next-line react/no-array-index-key
                <div className="bt-frame-row bt-frame-row--unresolved" key={frameKey(frame, index)}>
                  <span className="bt-fn bt-fn--unresolved">
                    {frame.unresolved.module_path}+0x{frame.unresolved.rel_pc.toString(16)}
                  </span>
                  <span className="bt-reason">{frame.unresolved.reason}</span>
                </div>
              );
            }
            if (isSystemFrame(frame)) {
              if (!showSystem) return null;
              // eslint-disable-next-line react/no-array-index-key
              return <FrameRow key={frameKey(frame, index)} frame={frame} isSystem />;
            }
            // eslint-disable-next-line react/no-array-index-key
            return <FrameRow key={frameKey(frame, index)} frame={frame} />;
          })}
        </div>
      </div>
    </div>
  );
}

function FrameRow({ frame, isSystem = false }: { frame: SnapshotBacktraceFrame; isSystem?: boolean }) {
  if (!isResolved(frame)) return null;
  const { function_name, source_file, line } = frame.resolved;
  const { prefix, fn } = splitFunctionName(function_name);
  const hasSource = source_file.length > 0;
  const relPath = hasSource ? relativePath(source_file) : null;
  const location = relPath != null ? (line != null ? `${relPath}:${line}` : relPath) : null;
  const href = hasSource ? zedUrl(source_file, line) : null;

  return (
    <div className={`bt-frame-row${isSystem ? " bt-frame-row--system" : ""}`}>
      <span className="bt-fn" title={function_name}>
        {prefix && <span className="bt-fn-prefix">{prefix}</span>}
        {fn}
      </span>
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
      {open && createPortal(
        <BacktracePanel backtrace={backtrace} onClose={() => setOpen(false)} />,
        document.body,
      )}
    </>
  );
}
