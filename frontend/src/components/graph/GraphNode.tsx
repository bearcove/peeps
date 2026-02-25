import React from "react";
import { DurationDisplay } from "../../ui/primitives/DurationDisplay";
import { kindIcon } from "../../nodeKindSpec";
import { useSourceLine } from "../../api/useSourceLine";
import { useSourcePreview } from "../../api/useSourcePreview";
import { splitHighlightedHtml, collapseContextLines } from "../../utils/highlightedHtml";
import { langIcon } from "./langIcon";
import { canonicalNodeKind } from "../../nodeKindSpec";
import type { GraphFrameData, GraphNodeData } from "./graphNodeData";
import { FrameList } from "./FrameList";
import "./GraphNode.css";

/** Futures: no header, 2 source frames. Everything else: header, 0 frames. */
const FRAMELESS_HEADER_KINDS = new Set(["future"]);

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
  // Strip crate:: prefix and keep the last 2 segments
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

export function FrameLineCollapsed({
  frame,
  showSource,
}: {
  frame: GraphFrameData;
  showSource?: boolean;
}) {
  const sourceHtml = useSourceLine(showSource ? frame.frame_id : undefined);
  const location = formatFileLocation(frame);

  const content = (() => {
    if (sourceHtml) {
      return (
        <pre
          className="graph-node-frame arborium-hl"
          dangerouslySetInnerHTML={{ __html: sourceHtml }}
        />
      );
    }
    if (showSource) {
      return <div className="graph-node-frame-skeleton" />;
    }
    return (
      <pre className="graph-node-frame graph-node-frame--text">
        <span className="graph-node-frame-fn">{shortFnName(frame.function_name)}</span>
        <span className="graph-node-frame-dot">&middot;</span>
        <a
          className="graph-node-frame-loc"
          href={zedHref(frame.source_file, frame.line)}
          onClick={stopPropagation}
        >
          {location}
        </a>
      </pre>
    );
  })();

  return (
    <div className="graph-node-frame-row graph-node-frame-row__collapsed">
      {content}
    </div>
  );
}

export function FrameLineExpanded({ frame, showSource }: { frame: GraphFrameData; showSource?: boolean }) {
  const preview = useSourcePreview(showSource ? frame.frame_id : undefined);
  const location = formatFileLocation(frame);

  const codeBlock = (() => {
    if (!preview) return null;
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
              <span className="graph-node-frame-block__gutter">{entry.lineNum}</span>
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
      <div className="graph-node-frame-sep">
        {langIcon(frame.source_file, 10, "graph-node-frame-sep__icon")}
        <span className="graph-node-frame-sep__name">{shortFnName(frame.function_name)}</span>
        <a
          className="graph-node-frame-sep__loc"
          href={zedHref(frame.source_file, frame.line)}
          onClick={stopPropagation}
        >
          {location}
        </a>
      </div>
      {codeBlock}
    </div>
  );
}

export function GraphNode({ data, expanded = false, pinned = false }: { data: GraphNodeData; expanded?: boolean; pinned?: boolean }) {
  const showScopeColor =
    data.scopeRgbLight !== undefined && data.scopeRgbDark !== undefined && !data.inCycle;

  const canonical = canonicalNodeKind(data.kind);
  const isFuture = FRAMELESS_HEADER_KINDS.has(canonical);
  // Futures always show source; other kinds only when explicitly toggled
  const collapsedShowSource = data.showSource || isFuture;
  const showHeader = !isFuture;

  const effectiveFrames =
    data.skipEntryFrames > 0 ? data.frames.slice(data.skipEntryFrames) : data.frames;
  const visibleFrames = expanded
    ? effectiveFrames
    : pickCollapsedFrames(data.kind, effectiveFrames);

  return (
    <div
      className={[
        "graph-card",
        "graph-node",
        expanded && "graph-node--expanded",
        pinned && "graph-node--pinned",
        data.inCycle && "graph-node--cycle",
        data.statTone === "crit" && "graph-card--stat-crit",
        data.statTone === "warn" && "graph-card--stat-warn",
        showScopeColor && "graph-card--scope",
        data.ghost && "graph-card--ghost",
      ]
        .filter(Boolean)
        .join(" ")}
      data-scroll-block={expanded ? "true" : undefined}
      style={
        showScopeColor
          ? ({
              "--scope-rgb-light": data.scopeRgbLight,
              "--scope-rgb-dark": data.scopeRgbDark,
            } as React.CSSProperties)
          : undefined
      }
    >
      {showHeader && (
        <>
          {/* Header row: icon + main info + file:line badge */}
          <div className="graph-node-header">
            <span className="graph-node-icon">{kindIcon(data.kind, 30)}</span>
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
          {data.sublabel && <div className="graph-node-sublabel">{data.sublabel}</div>}
        </>
      )}
      <FrameList
        data={data}
        expanded={expanded}
        isFuture={isFuture}
        collapsedShowSource={collapsedShowSource}
        collapsedFrames={visibleFrames}
      />
    </div>
  );
}
