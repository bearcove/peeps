import type { SourcePreviewResponse } from "./types.generated";
import { apiClient } from "./index";
import { splitHighlightedHtml } from "../utils/highlightedHtml";

/** In-flight / resolved promise cache: frameId → promise. Survives unmount/remount. */
const sourcePreviewCache = new Map<number, Promise<SourcePreviewResponse>>();

/** Resolved preview cache: frameId → response, populated when the promise settles. */
const resolvedPreviewCache = new Map<number, SourcePreviewResponse>();

/** Resolved single-line cache: frameId → extracted highlighted HTML line. */
const resolvedLineCache = new Map<number, string>();

function extractLineFromPreview(res: SourcePreviewResponse): string | undefined {
  // Prefer context_line: the whole target statement collapsed to one line
  if (res.context_line) return res.context_line;
  // Fallback to full file target line
  const lines = splitHighlightedHtml(res.html);
  const targetIdx = res.target_line - 1;
  return targetIdx >= 0 && targetIdx < lines.length ? lines[targetIdx]?.trim() : undefined;
}

export function cachedFetchSourcePreview(frameId: number): Promise<SourcePreviewResponse> {
  let cached = sourcePreviewCache.get(frameId);
  if (!cached) {
    cached = apiClient.fetchSourcePreview(frameId).then((res) => {
      resolvedPreviewCache.set(frameId, res);
      const line = extractLineFromPreview(res);
      if (line) resolvedLineCache.set(frameId, line);
      return res;
    });
    cached.catch(() => sourcePreviewCache.delete(frameId));
    sourcePreviewCache.set(frameId, cached);
  }
  return cached;
}

export function getSourcePreviewSync(frameId: number): SourcePreviewResponse | undefined {
  return resolvedPreviewCache.get(frameId);
}

export function getSourceLineSync(frameId: number): string | undefined {
  return resolvedLineCache.get(frameId);
}
