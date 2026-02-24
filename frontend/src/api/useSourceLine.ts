import { useEffect, useState } from "react";
import { cachedFetchSourcePreview } from "./sourceCache";
import { splitHighlightedHtml } from "../utils/highlightedHtml";

/**
 * Fetch and cache a single syntax-highlighted source line for a frame.
 *
 * Returns the trimmed HTML string for the target line, or `undefined` while
 * loading (or if the frame has no source). The underlying fetch is deduplicated
 * and cached by `cachedFetchSourcePreview`.
 */
export function useSourceLine(frameId: number | undefined): string | undefined {
  const [html, setHtml] = useState<string | undefined>(() => {
    if (frameId == null) return undefined;
    return resolvedLineCache.get(frameId);
  });

  useEffect(() => {
    if (frameId == null) {
      setHtml(undefined);
      return;
    }

    const cached = resolvedLineCache.get(frameId);
    if (cached !== undefined) {
      setHtml(cached);
      return;
    }

    let cancelled = false;
    cachedFetchSourcePreview(frameId).then((res) => {
      const lines = splitHighlightedHtml(res.html);
      const targetIdx = res.target_line - 1;
      const line = targetIdx >= 0 && targetIdx < lines.length ? lines[targetIdx]?.trim() : undefined;
      if (line) resolvedLineCache.set(frameId, line);
      if (!cancelled) setHtml(line);
    }).catch(() => {
      if (!cancelled) setHtml(undefined);
    });

    return () => { cancelled = true; };
  }, [frameId]);

  return html;
}

/** Module-level cache: frameId → resolved highlighted HTML line. */
const resolvedLineCache = new Map<number, string>();
