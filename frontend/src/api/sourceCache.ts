import type { SourcePreviewResponse } from "./types.generated";
import { apiClient } from "./index";

/** Module-level cache: frameId → promise of source preview. Survives unmount/remount. */
const sourcePreviewCache = new Map<number, Promise<SourcePreviewResponse>>();

export function cachedFetchSourcePreview(frameId: number): Promise<SourcePreviewResponse> {
  let cached = sourcePreviewCache.get(frameId);
  if (!cached) {
    cached = apiClient.fetchSourcePreview(frameId);
    // Evict on failure so a transient error can be retried next time.
    cached.catch(() => sourcePreviewCache.delete(frameId));
    sourcePreviewCache.set(frameId, cached);
  }
  return cached;
}
