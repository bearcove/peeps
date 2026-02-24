import React from "react";
import type { SourcePreviewResponse } from "../../api/types.generated";
import { splitHighlightedHtml } from "../../utils/highlightedHtml";
import "./SourcePreview.css";

const CONTEXT_LINES = 4;

export function SourcePreview({ preview }: { preview: SourcePreviewResponse }) {
  const lines = splitHighlightedHtml(preview.html);
  const { target_line } = preview;

  // target_line is 1-based
  const start = Math.max(0, target_line - 1 - CONTEXT_LINES);
  const end = Math.min(lines.length, target_line + CONTEXT_LINES);
  const slice = lines.slice(start, end);
  const firstLineNum = start + 1;

  return (
    <div className="ui-source-preview arborium-hl">
      <pre className="ui-source-preview__code">
        {slice.map((html, i) => {
          const lineNum = firstLineNum + i;
          const isTarget = lineNum === target_line;
          return (
            <div
              key={lineNum}
              className={`ui-source-preview__line${isTarget ? " ui-source-preview__line--target" : ""}`}
            >
              <span className="ui-source-preview__gutter">
                <span className="ui-source-preview__ln">{lineNum}</span>
                <span className="ui-source-preview__ribbon" />
              </span>
              {/* eslint-disable-next-line react/no-danger */}
              <span className="ui-source-preview__text" dangerouslySetInnerHTML={{ __html: html }} />
            </div>
          );
        })}
      </pre>
    </div>
  );
}
