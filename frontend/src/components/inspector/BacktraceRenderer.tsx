import React, { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { Stack, ArrowSquareOut, FileRs } from "@phosphor-icons/react";
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
  "moire_runtime::",
  "moire_tokio::",
];

function isResolved(frame: SnapshotBacktraceFrame): frame is { resolved: { module_path: string; function_name: string; source_file: string; line?: number } } {
  return "resolved" in frame;
}

function isSystemFrame(frame: SnapshotBacktraceFrame): boolean {
  if (!isResolved(frame)) return false;
  return SYSTEM_PREFIXES.some((prefix) => frame.resolved.function_name.startsWith(prefix));
}

function detectAppCrate(frames: SnapshotBacktraceFrame[]): string | null {
  for (const frame of frames) {
    if (!isResolved(frame)) continue;
    if (/(?:^|::)main$/.test(frame.resolved.function_name)) {
      return frame.resolved.function_name.split("::")[0] ?? null;
    }
  }
  return null;
}

// Index is intentional: a backtrace can contain the same frame multiple times (recursion).
function frameKey(frame: SnapshotBacktraceFrame, index: number): string {
  if (isResolved(frame)) {
    return `r:${index}:${frame.resolved.module_path}:${frame.resolved.function_name}:${frame.resolved.source_file}:${frame.resolved.line ?? ""}`;
  }
  return `u:${index}:${frame.unresolved.module_path}:${frame.unresolved.rel_pc}`;
}

/** Split "some::module::func_name" into { prefix: "some::module::", fn: "func_name" }. */
function splitFunctionName(name: string): { prefix: string; fn: string } {
  const lastSep = name.lastIndexOf("::");
  if (lastSep === -1) return { prefix: "", fn: name };
  return { prefix: name.slice(0, lastSep + 2), fn: name.slice(lastSep + 2) };
}

/** Show last 3 path components, e.g. "moire-trace-capture/src/lib.rs". */
function relativePath(path: string): string {
  const parts = path.split("/");
  return parts.slice(-3).join("/");
}

function zedUrl(path: string, line?: number): string {
  return line != null ? `zed://file${path}:${line}` : `zed://file${path}`;
}

/** Compact inline badge: location as Zed link + expand button for the full panel. */
export function BacktraceBadge({
  backtrace,
  onExpand,
}: {
  backtrace: ResolvedSnapshotBacktrace;
  onExpand: () => void;
}) {
  const topFrame = useMemo(
    () =>
      backtrace.frames.find((f) => isResolved(f) && !isSystemFrame(f))
      ?? backtrace.frames.find((f) => isResolved(f))
      ?? backtrace.frames[0],
    [backtrace.frames],
  );

  const resolvedTop = topFrame && isResolved(topFrame) ? topFrame.resolved : null;
  const location = resolvedTop?.source_file
    ? `${resolvedTop.source_file.split("/").pop() ?? resolvedTop.source_file}${resolvedTop.line != null ? `:${resolvedTop.line}` : ""}`
    : null;
  const href = resolvedTop?.source_file ? zedUrl(resolvedTop.source_file, resolvedTop.line) : null;

  return (
    <span className="bt-badge">
      {location && href ? (
        <a
          className="bt-badge-location"
          href={href}
          title={`Open ${resolvedTop!.source_file}${resolvedTop!.line != null ? `:${resolvedTop!.line}` : ""} in Zed`}
        >
          <FileRs size={12} weight="bold" className="bt-badge-file-icon" />
          {location}
        </a>
      ) : (
        <span className="bt-badge-location bt-badge-location--pending">pending…</span>
      )}
      <button type="button" className="bt-badge-expand" onClick={onExpand} title="View full backtrace">
        <Stack size={11} weight="bold" />
        View backtrace
      </button>
    </span>
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

  const appCrate = useMemo(() => detectAppCrate(backtrace.frames), [backtrace.frames]);
  const systemCount = useMemo(
    () => backtrace.frames.filter((f) => isResolved(f) && isSystemFrame(f)).length,
    [backtrace.frames],
  );
  const unresolvedCount = useMemo(
    () => backtrace.frames.filter((f) => !isResolved(f)).length,
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
          <span className="bt-dialog-title">Backtrace</span>
          <span className="bt-dialog-meta">
            {backtrace.frames.length} frames
            {unresolvedCount > 0 && <> · {unresolvedCount} unresolved</>}
            {appCrate && <> · app: <span className="bt-dialog-app-crate">{appCrate}</span></>}
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
          {backtrace.frames.map((frame, index) => {
            if (!isResolved(frame)) {
              return (
                // eslint-disable-next-line react/no-array-index-key -- index disambiguates recursive frames
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
              // eslint-disable-next-line react/no-array-index-key -- index disambiguates recursive frames
              return <FrameRow key={frameKey(frame, index)} frame={frame} appCrate={null} isSystem />;
            }
            // eslint-disable-next-line react/no-array-index-key -- index disambiguates recursive frames
            return <FrameRow key={frameKey(frame, index)} frame={frame} appCrate={appCrate} />;
          })}
        </div>
      </div>
    </div>
  );
}

function FrameRow({
  frame,
  appCrate,
  isSystem = false,
}: {
  frame: SnapshotBacktraceFrame;
  appCrate: string | null;
  isSystem?: boolean;
}) {
  if (!isResolved(frame)) return null;
  const { function_name, source_file, line } = frame.resolved;
  const { prefix, fn } = splitFunctionName(function_name);
  const isApp = appCrate != null && (
    function_name === appCrate || function_name.startsWith(appCrate + "::")
  );
  const hasSource = source_file.length > 0;
  const relPath = hasSource ? relativePath(source_file) : null;
  const location = relPath != null ? (line != null ? `${relPath}:${line}` : relPath) : null;
  const href = hasSource ? zedUrl(source_file, line) : null;

  const rowClass = [
    "bt-frame-row",
    isSystem && "bt-frame-row--system",
    isApp && "bt-frame-row--app",
  ].filter(Boolean).join(" ");

  return (
    <div className={rowClass}>
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
      <BacktraceBadge backtrace={backtrace} onExpand={() => setOpen(true)} />
      {open && createPortal(
        <BacktracePanel backtrace={backtrace} onClose={() => setOpen(false)} />,
        document.body,
      )}
    </>
  );
}
