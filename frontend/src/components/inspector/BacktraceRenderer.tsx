import React, { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { Stack, FileRs } from "@phosphor-icons/react";
import type { SnapshotBacktraceFrame } from "../../api/types.generated";
import type { ResolvedSnapshotBacktrace } from "../../snapshot";
import { assignScopeColorRgbByKey, type ScopeColorPair } from "../graph/scopeColors";
import { Source } from "./Source";
import { CratePill } from "../../ui/primitives/CratePill";
import { ClosurePill } from "../../ui/primitives/ClosurePill";
import { tokenizeRustName, parseSlim, RustTokens } from "../../ui/primitives/RustName";
import { FrameCard } from "../../ui/primitives/FrameCard";
import "./BacktraceRenderer.css";

const SYSTEM_CRATES = new Set([
  "std", "core", "alloc",
  "tokio", "tokio_util",
  "futures", "futures_core", "futures_util",
  "moire", "moire_trace_capture", "moire_runtime", "moire_tokio",
]);

function isResolved(frame: SnapshotBacktraceFrame): frame is { resolved: { module_path: string; function_name: string; source_file: string; line?: number } } {
  return "resolved" in frame;
}

function isSystemFrame(frame: SnapshotBacktraceFrame): boolean {
  if (!isResolved(frame)) return false;
  const crate = extractCrate(frame.resolved.function_name);
  return crate !== null && SYSTEM_CRATES.has(crate);
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

function frameKey(frame: SnapshotBacktraceFrame, index: number): string {
  if (isResolved(frame)) {
    return `r:${index}:${frame.resolved.module_path}:${frame.resolved.function_name}:${frame.resolved.source_file}:${frame.resolved.line ?? ""}`;
  }
  return `u:${index}:${frame.unresolved.module_path}:${frame.unresolved.rel_pc}`;
}

function extractCrate(functionName: string): string | null {
  if (functionName.startsWith("<")) {
    const m = functionName.match(/^<([a-zA-Z_][a-zA-Z0-9_]*)::/);
    return m?.[1] ?? null;
  }
  const sep = functionName.indexOf("::");
  return sep === -1 ? null : functionName.slice(0, sep);
}

function zedUrl(path: string, line?: number): string {
  return line != null ? `zed://file${path}:${line}` : `zed://file${path}`;
}

// ── Badge ────────────────────────────────────────────────────────────────────

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
      </button>
    </span>
  );
}

// ── Panel ────────────────────────────────────────────────────────────────────

export function BacktracePanel({
  backtrace,
  onClose,
}: {
  backtrace: ResolvedSnapshotBacktrace;
  onClose: () => void;
}) {
  const [showSystem, setShowSystem] = useState(false);

  const appCrate = useMemo(() => detectAppCrate(backtrace.frames), [backtrace.frames]);

  const crateColors = useMemo(() => {
    const crates = backtrace.frames
      .filter(isResolved)
      .map(f => extractCrate(f.resolved.function_name) ?? "")
      .filter(Boolean);
    return assignScopeColorRgbByKey(crates);
  }, [backtrace.frames]);

  const systemCount = useMemo(
    () => backtrace.frames.filter((f) => isResolved(f) && isSystemFrame(f)).length,
    [backtrace.frames],
  );
  const unresolvedCount = useMemo(
    () => backtrace.frames.filter((f) => !isResolved(f)).length,
    [backtrace.frames],
  );

  // Annotate each frame with whether its crate repeats the previous visible frame's crate.
  const annotatedFrames = useMemo(() => {
    const visible = backtrace.frames
      .map((frame, origIndex) => ({ frame, origIndex }))
      .filter(({ frame }) => isResolved(frame) ? (showSystem || !isSystemFrame(frame)) : true);

    return visible.map(({ frame, origIndex }) => ({ frame, origIndex }));
  }, [backtrace.frames, showSystem]);

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
          {annotatedFrames.map(({ frame, origIndex }) => (
            <FrameRow
              key={frameKey(frame, origIndex)}
              frame={frame}
              crateColors={crateColors}
              appCrate={appCrate}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

// ── Frame row ────────────────────────────────────────────────────────────────

function FrameRow({
  frame,
  crateColors,
  appCrate,
}: {
  frame: SnapshotBacktraceFrame;
  crateColors: Map<string, ScopeColorPair>;
  appCrate: string | null;
}) {
  const [expanded, setExpanded] = useState(false);

  if (!isResolved(frame)) {
    return (
      <FrameCard>
        <div className="bt-frame-row bt-frame-row--unresolved">
          <span className="bt-fn bt-fn--unresolved">
            {frame.unresolved.module_path}+0x{frame.unresolved.rel_pc.toString(16)}
          </span>
          <span className="bt-reason">—</span>
        </div>
      </FrameCard>
    );
  }

  const { function_name, source_file, line } = frame.resolved;
  const crate = extractCrate(function_name);
  const crateColor = crate ? crateColors.get(crate) ?? null : null;
  const isApp = appCrate != null && crate === appCrate;

  // eslint-disable-next-line react-hooks/rules-of-hooks
  const allTokens = useMemo(() => tokenizeRustName(function_name), [function_name]);
  // eslint-disable-next-line react-hooks/rules-of-hooks
  const { slim, closureCount, wasStripped } = useMemo(() => parseSlim(allTokens), [allTokens]);

  const sourceStr = source_file.length > 0
    ? (line != null ? `${source_file}:${line}` : source_file)
    : null;

  const rowClass = [
    "bt-frame-row",
    isApp && "bt-frame-row--app",
  ].filter(Boolean).join(" ");

  return (
    <FrameCard>
      <div className={rowClass}>
        <div className="bt-fn-line">
          {crate && (
            <CratePill name={crate} color={crateColor ?? undefined} />
          )}
          <span
            className={`bt-fn${wasStripped ? " bt-fn--expandable" : ""}`}
            title={function_name}
            onClick={wasStripped ? () => setExpanded(v => !v) : undefined}
            role={wasStripped ? "button" : undefined}
          >
            <RustTokens tokens={expanded ? allTokens : slim} />
          </span>
          {closureCount > 0 && <ClosurePill />}
        </div>
        {sourceStr && <Source source={sourceStr} />}
      </div>
    </FrameCard>
  );
}

// ── Widget ───────────────────────────────────────────────────────────────────

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
