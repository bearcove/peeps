import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import { kindIcon } from "../../nodeKindSpec";
import {
  cachedFetchSourcePreviews,
  getSourceLineSync,
  getSourcePreviewSync,
} from "../../api/sourceCache";
import { splitHighlightedHtml, collapseContextLines } from "../../utils/highlightedHtml";
import { langIcon } from "./langIcon";
import { canonicalNodeKind } from "../../nodeKindSpec";
import type { GraphFrameData, GraphNodeData } from "./graphNodeData";
import { FrameList } from "./FrameList";
import "./GraphNode.css";

/** Futures: no header, 2 source frames. Everything else: header, 0 frames. */
const FRAMELESS_HEADER_KINDS: Set<string> = new Set([]);

export function collapsedFrameCount(kind: string): number {
  return FRAMELESS_HEADER_KINDS.has(canonicalNodeKind(kind)) ? 1 : 0;
}

function pickCollapsedFrames(kind: string, frames: GraphFrameData[]): GraphFrameData[] {
  return frames.slice(0, collapsedFrameCount(kind));
}

function formatFileLocation(f: GraphFrameData): string {
  const file = f.source_file.split("/").pop() ?? f.source_file;
  return f.line != null ? `${file}:${f.line}` : file;
}

function shortFnName(fn_name: string): string {
  const parts = fn_name.split("::");
  if (parts.length <= 2) return fn_name;
  return parts.slice(-2).join("::");
}

function zedHref(sourceFile: string, line?: number): string {
  return line != null ? `zed://file${sourceFile}:${line}` : `zed://file${sourceFile}`;
}

function stopPropagation(e: React.MouseEvent) {
  e.stopPropagation();
}

export function FrameSep({ frame }: { frame: GraphFrameData }) {
  const location = formatFileLocation(frame);
  return (
    <div className="graph-node-frame-sep">
      {langIcon(frame.source_file, 12, "graph-node-frame-sep__icon")}
      <span className="graph-node-frame-sep__name">{shortFnName(frame.function_name)}</span>
      <a
        className="graph-node-frame-sep__loc"
        href={zedHref(frame.source_file, frame.line)}
        onClick={stopPropagation}
      >
        {location}
      </a>
    </div>
  );
}

export function FrameLine({
  frame,
  expanded,
  showSource,
}: {
  frame: GraphFrameData;
  expanded: boolean;
  showSource?: boolean;
}) {
  const fallbackCollapsedLine = (
    <pre className="graph-node-frame graph-node-frame--text graph-node-frame--fallback">
      {frame.function_name}
    </pre>
  );
  const fallbackExpandedLine = (
    <pre className="graph-node-frame graph-node-frame--text graph-node-frame--fallback">…</pre>
  );

  const codeBlock = (() => {
    if (!showSource) return null;

    if (!expanded) {
      if (frame.frame_id == null) return fallbackCollapsedLine;
      const preview = getSourcePreviewSync(frame.frame_id);
      if (!preview) return fallbackExpandedLine;
      const enclosingFn = preview.enclosing_fn;
      if (enclosingFn) {
        return (
          <div className="graph-node-enclosing-fn arborium-hl">
            <span className="graph-node-enclosing-fn__in">in </span>
            {/* eslint-disable-next-line react/no-danger */}
            <span
              className="graph-node-enclosing-fn__name"
              dangerouslySetInnerHTML={{ __html: enclosingFn }}
            />
          </div>
        );
      }
      // Fallback for non-Rust: show the single-line code snippet
      const lineHtml = getSourceLineSync(frame.frame_id);
      if (!lineHtml) return null;
      return (
        <pre
          className="graph-node-frame arborium-hl"
          dangerouslySetInnerHTML={{ __html: lineHtml }}
        />
      );
    }

    if (frame.frame_id == null) return fallbackCollapsedLine;
    const preview = getSourcePreviewSync(frame.frame_id);
    if (!preview) return fallbackExpandedLine;
    const useCtx = preview.context_html != null && preview.context_range != null;
    const rawLines = splitHighlightedHtml(useCtx ? preview.context_html! : preview.html);
    const startLineNum = useCtx ? preview.context_range!.start : 1;
    const lines = useCtx
      ? collapseContextLines(rawLines, startLineNum)
      : rawLines.map((html, i) => ({ lineNum: startLineNum + i, html, isSeparator: false }));

    return (
      <pre className="graph-node-frame-block arborium-hl">
        {lines.map((entry) => {
          if (entry.isSeparator) {
            return (
              <div key={`sep-${entry.lineNum}`} className="graph-node-frame-block__sep">
                <span className="graph-node-frame-block__gutter" />
                <span className="graph-node-frame-block__sep-label">⋯</span>
              </div>
            );
          }
          const isTarget = entry.lineNum === preview.target_line;
          return (
            <div
              key={entry.lineNum}
              className={`graph-node-frame-block__line${isTarget ? " graph-node-frame-block__line--target" : ""}`}
            >
              <span
                className="graph-node-frame-block__gutter"
                onClick={(e) => {
                  e.stopPropagation();
                  window.location.href = zedHref(frame.source_file, entry.lineNum);
                }}
              >
                {entry.lineNum}
              </span>
              {/* eslint-disable-next-line react/no-danger */}
              <span
                className="graph-node-frame-block__text"
                dangerouslySetInnerHTML={{ __html: entry.html }}
              />
            </div>
          );
        })}
      </pre>
    );
  })();

  return (
    <div className="graph-node-frame-section">
      <FrameSep frame={frame} />
      {codeBlock}
    </div>
  );
}

export function GraphNode({
  data,
  expanded = false,
  expanding = false,
}: {
  data: GraphNodeData;
  expanded?: boolean;
  expanding?: boolean;
}) {
  const showScopeColor =
    data.scopeRgbLight !== undefined && data.scopeRgbDark !== undefined && !data.inCycle;

  const canonical = canonicalNodeKind(data.kind);
  const isFutureKind = canonical === "future" || canonical === "futures";
  const isFramelessHeaderKind = FRAMELESS_HEADER_KINDS.has(canonical);
  // Futures always show source; other kinds only when explicitly toggled
  const collapsedShowSource = data.showSource || isFramelessHeaderKind;
  const showHeader = !isFramelessHeaderKind;

  const effectiveFrames =
    data.skipEntryFrames > 0 ? data.frames.slice(data.skipEntryFrames) : data.frames;
  const futureTopFrameId = isFutureKind ? effectiveFrames[0]?.frame_id : undefined;
  const futureTopPreview = futureTopFrameId != null ? getSourcePreviewSync(futureTopFrameId) : null;
  const futureTopStatement = futureTopFrameId != null ? getSourceLineSync(futureTopFrameId) : null;
  const collapsedFrameSlotCount = collapsedFrameCount(data.kind);
  const visibleFrames = expanded
    ? effectiveFrames
    : pickCollapsedFrames(data.kind, effectiveFrames);
  const collapsedSourceFrameIds = useMemo(() => {
    if (expanded) return [];
    if (!collapsedShowSource) return [];
    return visibleFrames
      .map((frame) => frame.frame_id)
      .filter((frameId): frameId is number => frameId != null);
  }, [expanded, collapsedShowSource, visibleFrames]);
  const [collapsedSourceLoading, setCollapsedSourceLoading] = useState(false);
  const [futureTopSourceLoading, setFutureTopSourceLoading] = useState(false);
  const framesShellRef = useRef<HTMLDivElement | null>(null);
  const lastFramesShellHeightRef = useRef(0);
  const prevExpandedRef = useRef(expanded);
  const shellTransitionRafRef = useRef<number | null>(null);

  useEffect(() => {
    if (collapsedSourceFrameIds.length === 0) {
      setCollapsedSourceLoading(false);
      return;
    }
    const missingFrameIds = collapsedSourceFrameIds.filter(
      (frameId) => getSourceLineSync(frameId) == null,
    );
    if (missingFrameIds.length === 0) {
      setCollapsedSourceLoading(false);
      return;
    }
    let cancelled = false;
    setCollapsedSourceLoading(true);
    void cachedFetchSourcePreviews(missingFrameIds).then(() => {
      if (cancelled) return;
      setCollapsedSourceLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [collapsedSourceFrameIds]);

  useEffect(() => {
    if (!isFutureKind || futureTopFrameId == null) {
      setFutureTopSourceLoading(false);
      return;
    }
    if (futureTopPreview && futureTopStatement) {
      setFutureTopSourceLoading(false);
      return;
    }
    let cancelled = false;
    setFutureTopSourceLoading(true);
    void cachedFetchSourcePreviews([futureTopFrameId]).then(() => {
      if (cancelled) return;
      setFutureTopSourceLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [futureTopFrameId, futureTopPreview, futureTopStatement, isFutureKind]);

  useLayoutEffect(() => {
    const el = framesShellRef.current;
    if (!el) return;
    const prevExpanded = prevExpandedRef.current;
    if (prevExpanded === expanded) return;
    prevExpandedRef.current = expanded;

    const fromHeight = Math.max(
      0,
      lastFramesShellHeightRef.current || el.getBoundingClientRect().height,
    );
    const toHeight = Math.max(0, el.scrollHeight);
    if (Math.abs(fromHeight - toHeight) < 0.5) return;

    if (shellTransitionRafRef.current != null) {
      cancelAnimationFrame(shellTransitionRafRef.current);
      shellTransitionRafRef.current = null;
    }

    el.style.height = `${fromHeight}px`;
    void el.offsetHeight;
    shellTransitionRafRef.current = window.requestAnimationFrame(() => {
      el.style.height = `${toHeight}px`;
      shellTransitionRafRef.current = null;
    });

    const onTransitionEnd = (event: TransitionEvent) => {
      if (event.propertyName !== "height") return;
      el.style.height = "auto";
      lastFramesShellHeightRef.current = el.getBoundingClientRect().height;
      el.removeEventListener("transitionend", onTransitionEnd);
    };

    el.addEventListener("transitionend", onTransitionEnd);
    return () => {
      el.removeEventListener("transitionend", onTransitionEnd);
    };
  }, [expanded]);

  useLayoutEffect(() => {
    const el = framesShellRef.current;
    if (!el) return;
    lastFramesShellHeightRef.current = el.getBoundingClientRect().height;
  });

  useEffect(() => {
    return () => {
      if (shellTransitionRafRef.current != null) {
        cancelAnimationFrame(shellTransitionRafRef.current);
      }
    };
  }, []);

  const cardStyle = showScopeColor
    ? ({
        "--scope-rgb-light": data.scopeRgbLight,
        "--scope-rgb-dark": data.scopeRgbDark,
      } as React.CSSProperties)
    : undefined;

  const isLoading = expanding || collapsedSourceLoading || futureTopSourceLoading;
  const showFutureSummary =
    isFutureKind && showHeader && (futureTopPreview?.enclosing_fn || futureTopStatement);

  return (
    <div
      className={[
        "graph-card",
        "graph-node",
        expanded && "graph-node--expanded",
        expanding && "graph-node--expanding",
        isLoading && "graph-node--loading",
        data.inCycle && "graph-node--cycle",
        data.statTone === "crit" && "graph-card--stat-crit",
        data.statTone === "warn" && "graph-card--stat-warn",
        showScopeColor && "graph-card--scope",
        data.ghost && "graph-card--ghost",
      ]
        .filter(Boolean)
        .join(" ")}
      data-scroll-block={expanded ? "true" : undefined}
      style={cardStyle}
    >
      {showHeader && (
        <>
          {/* Header row: icon + main info + file:line badge */}
          <div className="graph-node-header">
            <span className="graph-node-icon">{kindIcon(data.kind, 18)}</span>
            <div className="graph-node-main">
              <span className="graph-node-label">{data.label}</span>
            </div>
            <div className="graph-node-main">
              {(data.ageMs ?? 0) > 3000 && (
                <>
                  <DurationDisplay ms={data.ageMs ?? 0} />
                </>
              )}
              {data.stat && (
                <>
                  <span className="graph-node-dot">&middot;</span>
                  <span
                    className={[
                      "graph-node-stat",
                      data.statTone === "crit" && "graph-node-stat--crit",
                      data.statTone === "warn" && "graph-node-stat--warn",
                    ]
                      .filter(Boolean)
                      .join(" ")}
                  >
                    {data.stat}
                  </span>
                </>
              )}
            </div>
          </div>
          {showFutureSummary ? (
            <div className="graph-node-future-summary">
              {futureTopPreview?.enclosing_fn && (
                <div className="graph-node-future-context arborium-hl">
                  <span className="graph-node-future-context__in">in </span>
                  {/* eslint-disable-next-line react/no-danger */}
                  <span
                    className="graph-node-future-context__name"
                    dangerouslySetInnerHTML={{ __html: futureTopPreview.enclosing_fn }}
                  />
                </div>
              )}
              {futureTopStatement && (
                // eslint-disable-next-line react/no-danger
                <div
                  className="graph-node-future-statement arborium-hl"
                  dangerouslySetInnerHTML={{ __html: futureTopStatement }}
                />
              )}
            </div>
          ) : (
            data.sublabel && <div className="graph-node-sublabel">{data.sublabel}</div>
          )}
        </>
      )}
      <div ref={framesShellRef} className="graph-node-frames-shell">
        <FrameList
          data={data}
          expanded={expanded}
          collapsedShowSource={collapsedShowSource}
          collapsedFrameSlotCount={collapsedFrameSlotCount}
          collapsedFrames={visibleFrames}
        />
      </div>
    </div>
  );
}
