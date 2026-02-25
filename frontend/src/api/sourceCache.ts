import type { SourcePreviewResponse } from "./types.generated";
import { apiClient } from "./index";
import { splitHighlightedHtml } from "../utils/highlightedHtml";

/** In-flight / resolved promise cache: frameId → promise. Survives unmount/remount. */
const sourcePreviewCache = new Map<number, Promise<SourcePreviewResponse>>();

/** Resolved preview cache: frameId → response, populated when the promise settles. */
const resolvedPreviewCache = new Map<number, SourcePreviewResponse>();

/** Resolved single-line cache: frameId → extracted highlighted HTML line. */
const resolvedLineCache = new Map<number, string>();

function seedPreviewCache(frameId: number, res: SourcePreviewResponse): void {
  resolvedPreviewCache.set(frameId, res);
  const line = extractLineFromPreview(res);
  if (line) resolvedLineCache.set(frameId, line);
}

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
    const batchPromise = apiClient
      .fetchSourcePreviews([frameId])
      .then((batch) => {
        const preview = batch.previews.find((entry) => entry.frame_id === frameId);
        if (!preview) {
          const unavailable = new Set(batch.unavailable_frame_ids);
          if (unavailable.has(frameId)) {
            throw new Error(`source preview unavailable for frame_id ${frameId}`);
          }
          throw new Error(
            `source preview batch response missing frame_id ${frameId} in both previews and unavailable_frame_ids`,
          );
        }
        seedPreviewCache(frameId, preview);
        return preview;
      })
      .catch(async () => {
        // Keep compatibility with older servers that only expose /api/source/preview.
        const fallback = await apiClient.fetchSourcePreview(frameId);
        seedPreviewCache(frameId, fallback);
        return fallback;
      });
    cached = batchPromise;
    cached.catch(() => sourcePreviewCache.delete(frameId));
    sourcePreviewCache.set(frameId, cached);
  }
  return cached;
}

/**
 * Preload many source previews via one backend call when possible.
 * Never throws: caller can proceed with fallbacks for frames that are unavailable.
 */
export async function cachedFetchSourcePreviews(frameIds: number[]): Promise<void> {
  const unique = [...new Set(frameIds)].filter((frameId) => Number.isFinite(frameId));
  if (unique.length === 0) return;
  const missing = unique.filter(
    (frameId) => !resolvedPreviewCache.has(frameId) && !sourcePreviewCache.has(frameId),
  );
  if (missing.length > 0) {
    try {
      const batch = await apiClient.fetchSourcePreviews(missing);
      const previewById = new Map(batch.previews.map((entry) => [entry.frame_id, entry]));
      const unavailableSet = new Set(batch.unavailable_frame_ids);
      for (const frameId of missing) {
        const preview = previewById.get(frameId);
        if (preview) {
          seedPreviewCache(frameId, preview);
          sourcePreviewCache.set(frameId, Promise.resolve(preview));
          continue;
        }
        if (unavailableSet.has(frameId)) {
          sourcePreviewCache.delete(frameId);
          continue;
        }
        throw new Error(
          `source preview batch response missing frame_id ${frameId} in both previews and unavailable_frame_ids`,
        );
      }
    } catch {
      await Promise.allSettled(missing.map((frameId) => cachedFetchSourcePreview(frameId)));
    }
  }

  await Promise.allSettled(unique.map((frameId) => sourcePreviewCache.get(frameId)));
}

export function getSourcePreviewSync(frameId: number): SourcePreviewResponse | undefined {
  return resolvedPreviewCache.get(frameId);
}

export function getSourceLineSync(frameId: number): string | undefined {
  return resolvedLineCache.get(frameId);
}
