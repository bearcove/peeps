import { useState } from "preact/hooks";

interface ExpandableProps {
  label?: string;
  content: string | null;
}

const FILE_LOC_RE = /(\/[^\s:][^:\n]*):(\d+):(\d+)/;

function zedFileUrl(path: string, line: string, col: string): string {
  return `zed://file${encodeURI(path)}:${line}:${col}`;
}

function renderTraceLine(line: string, idx: number) {
  const match = line.match(FILE_LOC_RE);
  const frameMatch = line.match(/^\s*(\d+):\s*/);
  const startsWithAt = /^\s*at\s+/.test(line);

  const framePrefix = frameMatch ? (
    <span class="trace-frame-idx">{frameMatch[1]}:</span>
  ) : startsWithAt ? (
    <span class="trace-at">at</span>
  ) : null;

  const withoutPrefix = frameMatch
    ? line.slice(frameMatch[0].length)
    : startsWithAt
      ? line.replace(/^\s*at\s+/, "")
      : line;

  if (!match) {
    return (
      <div class="trace-line" key={idx}>
        {framePrefix}
        {framePrefix ? " " : ""}
        <span class="trace-text">{withoutPrefix || "\u00a0"}</span>
      </div>
    );
  }

  const [full, path, lineNo, colNo] = match;
  const start = withoutPrefix.indexOf(full);
  const before = start >= 0 ? withoutPrefix.slice(0, start) : withoutPrefix;
  const after = start >= 0 ? withoutPrefix.slice(start + full.length) : "";

  return (
    <div class="trace-line" key={idx}>
      {framePrefix}
      {framePrefix ? " " : ""}
      {before ? <span class="trace-text">{before}</span> : null}
      <a
        class="trace-file-link"
        href={zedFileUrl(path, lineNo, colNo)}
        title={`Open in Zed: ${path}:${lineNo}:${colNo}`}
      >
        <span class="trace-file-path">{path}</span>
        <span class="trace-file-pos">:{lineNo}:{colNo}</span>
      </a>
      {after ? <span class="trace-text">{after}</span> : null}
    </div>
  );
}

export function Expandable({ label = "trace", content }: ExpandableProps) {
  const [open, setOpen] = useState(false);

  if (!content) return <span class="muted">{"\u2014"}</span>;

  return (
    <>
      <span class="expand-trigger" onClick={() => setOpen(!open)}>
        {label}
      </span>
      {open && (
        <div class="expandable-content open">
          {content.split("\n").map((line, idx) => renderTraceLine(line, idx))}
        </div>
      )}
    </>
  );
}
