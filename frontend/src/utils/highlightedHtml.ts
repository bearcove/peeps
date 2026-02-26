/** Strip all HTML tags to get plain text. */
export function stripHtmlTags(html: string): string {
  return html.replace(/<[^>]*>/g, "");
}

/** Check if a highlighted HTML line is a context cut marker (the "slash-star ... star-slash" placeholder). */
export function isContextCutMarker(htmlLine: string): boolean {
  const plain = stripHtmlTags(htmlLine).trim();
  return plain === "/* ... */";
}

export interface CollapsedLine {
  lineNum: number;
  html: string;
  isSeparator: boolean;
  separatorIndentCols?: number;
}

function leadingIndentCols(text: string): number {
  let cols = 0;
  for (const ch of text) {
    if (ch === " ") {
      cols += 1;
      continue;
    }
    if (ch === "\t") {
      cols += 4;
      continue;
    }
    break;
  }
  return cols;
}

/**
 * Collapse cut marker lines (and their following empty lines) into single separator entries.
 * Input: per-line HTML strings from splitHighlightedHtml applied to context_html.
 * startLineNum: the 1-based line number of the first line in `lines`.
 */
export function collapseContextLines(lines: string[], startLineNum: number): CollapsedLine[] {
  const result: CollapsedLine[] = [];
  let i = 0;
  while (i < lines.length) {
    const lineNum = startLineNum + i;
    if (isContextCutMarker(lines[i])) {
      const plain = stripHtmlTags(lines[i]);
      result.push({
        lineNum,
        html: "",
        isSeparator: true,
        separatorIndentCols: leadingIndentCols(plain),
      });
      i++;
      // Skip following empty lines (part of the same cut region)
      while (i < lines.length && stripHtmlTags(lines[i]).trim() === "") {
        i++;
      }
    } else {
      result.push({ lineNum, html: lines[i], isSeparator: false });
      i++;
    }
  }
  return result;
}

function trimLeadingIndentFromHtmlLine(htmlLine: string, dedentCols: number): string {
  if (dedentCols <= 0) return htmlLine;
  const parser = new DOMParser();
  const doc = parser.parseFromString(`<div>${htmlLine}</div>`, "text/html");
  const container = doc.body.firstElementChild;
  if (!container) return htmlLine;

  let remaining = dedentCols;
  let hitCode = false;

  function visit(node: Node): void {
    if (remaining <= 0 || hitCode) return;
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent ?? "";
      if (text.length === 0) return;

      let cut = 0;
      while (cut < text.length && remaining > 0) {
        const ch = text[cut];
        if (ch === " ") {
          cut += 1;
          remaining -= 1;
          continue;
        }
        if (ch === "\t") {
          if (remaining < 4) {
            hitCode = true;
            return;
          }
          cut += 1;
          remaining -= 4;
          continue;
        }
        hitCode = true;
        return;
      }

      if (cut > 0) {
        node.textContent = text.slice(cut);
      }
      if (remaining <= 0) return;

      const next = node.textContent ?? "";
      if (next.length === 0) return;
      const first = next[0];
      if (first !== " " && first !== "\t") {
        hitCode = true;
      }
      return;
    }
    if (node.nodeType === Node.ELEMENT_NODE) {
      for (const child of node.childNodes) {
        visit(child);
        if (remaining <= 0 || hitCode) return;
      }
    }
  }

  for (const child of container.childNodes) {
    visit(child);
    if (remaining <= 0 || hitCode) break;
  }

  return container.innerHTML;
}

export function dedentHighlightedHtmlLines(lines: string[]): string[] {
  let minIndent = Number.POSITIVE_INFINITY;
  for (const line of lines) {
    const plain = stripHtmlTags(line);
    if (plain.trim() === "") continue;
    minIndent = Math.min(minIndent, leadingIndentCols(plain));
  }
  if (!Number.isFinite(minIndent) || minIndent <= 0) return lines;
  return lines.map((line) => trimLeadingIndentFromHtmlLine(line, minIndent));
}

export function dedentHighlightedHtmlBlock(html: string): string {
  const lines = splitHighlightedHtml(html);
  if (lines.length === 0) return html;
  return dedentHighlightedHtmlLines(lines).join("\n");
}

/**
 * Split arborium-highlighted HTML into per-line strings while preserving
 * tag nesting balance across line boundaries.
 *
 * Arborium produces a flat HTML string where inline elements (`<a-k>`,
 * `<a-f>`, etc.) can span multiple lines. This function splits at `\n`
 * characters and reopens/closes any tags that straddle a line break.
 */
export function splitHighlightedHtml(html: string): string[] {
  const parser = new DOMParser();
  const doc = parser.parseFromString(`<div>${html}</div>`, "text/html");
  const container = doc.body.firstChild;
  const lines: string[] = [];
  let currentLine = "";
  const openTags: { tag: string; attrs: string }[] = [];

  function processNode(node: Node) {
    if (node.nodeType === Node.TEXT_NODE) {
      const text = node.textContent ?? "";
      for (const char of text) {
        if (char === "\n") {
          for (let j = openTags.length - 1; j >= 0; j--) {
            currentLine += `</${openTags[j].tag}>`;
          }
          lines.push(currentLine);
          currentLine = "";
          for (const t of openTags) {
            currentLine += `<${t.tag}${t.attrs}>`;
          }
        } else {
          currentLine +=
            char === "<" ? "&lt;" : char === ">" ? "&gt;" : char === "&" ? "&amp;" : char;
        }
      }
    } else if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node as Element;
      const tag = el.tagName.toLowerCase();
      let attrs = "";
      for (const attr of el.attributes) {
        attrs += ` ${attr.name}="${attr.value.replace(/"/g, "&quot;")}"`;
      }
      currentLine += `<${tag}${attrs}>`;
      openTags.push({ tag, attrs });
      for (const child of el.childNodes) {
        processNode(child);
      }
      openTags.pop();
      currentLine += `</${tag}>`;
    }
  }

  if (container) {
    for (const child of container.childNodes) {
      processNode(child);
    }
  }
  if (currentLine) {
    lines.push(currentLine);
  }
  return lines;
}
