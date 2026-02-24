import React, { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import { Stack, FileRs } from "@phosphor-icons/react";
import type { SnapshotBacktraceFrame, SourcePreviewResponse } from "../../api/types.generated";
import { isSystemCrate, type ResolvedSnapshotBacktrace } from "../../snapshot";
import { assignScopeColorRgbByKey, type ScopeColorPair } from "../graph/scopeColors";
import { apiClient } from "../../api";
import { Source } from "./Source";
import { ClosurePill } from "../../ui/primitives/ClosurePill";
import { tokenizeRustName, parseSlim, RustTokens } from "../../ui/primitives/RustName";
import { FrameCard } from "../../ui/primitives/FrameCard";
import { SourcePreview } from "../../ui/primitives/SourcePreview";
import "./BacktraceRenderer.css";

/** Module-level cache: frameId → promise of source preview. Survives unmount/remount. */
const sourcePreviewCache = new Map<number, Promise<SourcePreviewResponse>>();

function cachedFetchSourcePreview(frameId: number): Promise<SourcePreviewResponse> {
  let cached = sourcePreviewCache.get(frameId);
  if (!cached) {
    cached = apiClient.fetchSourcePreview(frameId);
    // Evict on failure so a transient error can be retried next time.
    cached.catch(() => sourcePreviewCache.delete(frameId));
    sourcePreviewCache.set(frameId, cached);
  }
  return cached;
}

function isResolved(frame: SnapshotBacktraceFrame): frame is {
  resolved: { module_path: string; function_name: string; source_file: string; line?: number };
} {
  return "resolved" in frame;
}

function isSystemFrame(frame: SnapshotBacktraceFrame): boolean {
  if (!isResolved(frame)) return false;
  const crate = extractCrate(frame.resolved.function_name);
  return crate !== null && isSystemCrate(crate);
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
      backtrace.frames.find((f) => isResolved(f) && !isSystemFrame(f)) ??
      backtrace.frames.find((f) => isResolved(f)) ??
      backtrace.frames[0],
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
      <button
        type="button"
        className="bt-badge-expand"
        onClick={onExpand}
        title="View full backtrace"
      >
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
  const [previews, setPreviews] = useState<Map<number, SourcePreviewResponse>>(new Map());

  const appCrate = useMemo(() => detectAppCrate(backtrace.frames), [backtrace.frames]);

  const crateColors = useMemo(() => {
    const crates = backtrace.frames
      .filter(isResolved)
      .map((f) => extractCrate(f.resolved.function_name) ?? "")
      .filter(Boolean);
    return assignScopeColorRgbByKey(crates);
  }, [backtrace.frames]);

  // Eagerly fetch source previews for all non-system resolved frames.
  useEffect(() => {
    const targets = backtrace.frame_ids
      .map((frameId, i) => ({ frameId, frame: backtrace.frames[i] }))
      .filter(({ frame }) => isResolved(frame) && !isSystemFrame(frame));

    Promise.allSettled(
      targets.map(({ frameId }) =>
        cachedFetchSourcePreview(frameId).then((res) => [frameId, res] as const),
      ),
    )
      .then((results) => {
        const map = new Map<number, SourcePreviewResponse>();
        for (const r of results) {
          if (r.status === "fulfilled") {
            const [frameId, res] = r.value;
            map.set(frameId, res);
          }
        }
        setPreviews(map);
      })
      .catch(() => {});
  }, [backtrace]);

  const systemCount = useMemo(
    () => backtrace.frames.filter((f) => isResolved(f) && isSystemFrame(f)).length,
    [backtrace.frames],
  );
  const unresolvedCount = useMemo(
    () => backtrace.frames.filter((f) => !isResolved(f)).length,
    [backtrace.frames],
  );

  const annotatedFrames = useMemo(() => {
    return backtrace.frames
      .map((frame, origIndex) => ({ frame, origIndex, frameId: backtrace.frame_ids[origIndex] }))
      .filter(({ frame }) => (isResolved(frame) ? showSystem || !isSystemFrame(frame) : true));
  }, [backtrace, showSystem]);

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
            {appCrate && (
              <>
                {" "}
                · app: <span className="bt-dialog-app-crate">{appCrate}</span>
              </>
            )}
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
          {annotatedFrames.map(({ frame, origIndex, frameId }) => (
            <>
              <FrameRow
                key={frameKey(frame, origIndex)}
                frame={frame}
                crateColors={crateColors}
                appCrate={appCrate}
                preview={previews.get(frameId)}
              />
            </>
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
  preview,
}: {
  frame: SnapshotBacktraceFrame;
  crateColors: Map<string, ScopeColorPair>;
  appCrate: string | null;
  preview: SourcePreviewResponse | undefined;
}) {
  const [nameExpanded, setNameExpanded] = useState(false);

  if (!isResolved(frame)) {
    return (
      <FrameCard>
        <div className="bt-frame-row bt-frame-row--unresolved">
          <span className="bt-fn bt-fn--unresolved">
            {frame.unresolved.module_path}+0x{frame.unresolved.rel_pc.toString(16)}
          </span>
        </div>
      </FrameCard>
    );
  }

  const { function_name, source_file, line } = frame.resolved;
  const crate = extractCrate(function_name);
  const crateColor = crate ? (crateColors.get(crate) ?? null) : null;
  const isApp = appCrate !== null && crate === appCrate;

  // Non-system resolved frames start with source preview open.
  // eslint-disable-next-line react-hooks/rules-of-hooks
  const [previewOpen, _setPreviewOpen] = useState(!isSystemFrame(frame));

  // eslint-disable-next-line react-hooks/rules-of-hooks
  const allTokens = useMemo(() => tokenizeRustName(function_name), [function_name]);
  // eslint-disable-next-line react-hooks/rules-of-hooks
  const { slim, closureCount, wasStripped } = useMemo(() => parseSlim(allTokens), [allTokens]);

  const sourceStr =
    source_file.length > 0 ? (line != null ? `${source_file}:${line}` : source_file) : null;

  const rowClass = ["bt-frame-row", isApp && "bt-frame-row--app"].filter(Boolean).join(" ");

  return (
    <FrameCard color={crateColor ?? undefined}>
      <div className={rowClass}>
        <div className="bt-fn-line">
          <span
            className={`bt-fn${wasStripped ? " bt-fn--expandable" : ""}`}
            title={function_name}
            onClick={wasStripped ? () => setNameExpanded((v) => !v) : undefined}
            role={wasStripped ? "button" : undefined}
          >
            <RustTokens tokens={nameExpanded ? allTokens : slim} />
          </span>
          {closureCount > 0 && <ClosurePill />}
          <span className="bt-filler" />
          {sourceStr && (
            <span className="bt-source-row">
              <Source source={sourceStr} />
            </span>
          )}
        </div>
        {previewOpen && preview && <SourcePreview preview={preview} />}
      </div>
    </FrameCard>
  );
}

// ── Widget ───────────────────────────────────────────────────────────────────

export function BacktraceRenderer({
  backtrace,
  openTrigger,
}: {
  backtrace: ResolvedSnapshotBacktrace;
  openTrigger?: number;
}) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (openTrigger) setOpen(true);
  }, [openTrigger]);

  return (
    <>
      <BacktraceBadge backtrace={backtrace} onExpand={() => setOpen(true)} />
      {open &&
        createPortal(
          <BacktracePanel backtrace={backtrace} onClose={() => setOpen(false)} />,
          document.body,
        )}
    </>
  );
}
