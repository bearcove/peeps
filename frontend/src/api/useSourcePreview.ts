import { useEffect, useState } from "react";
import type { SourcePreviewResponse } from "./types.generated";
import { cachedFetchSourcePreview } from "./sourceCache";

/**
 * Fetch and cache the full source preview for a frame.
 *
 * Returns the `SourcePreviewResponse` or `undefined` while loading / on error.
 */
export function useSourcePreview(frameId: number | undefined): SourcePreviewResponse | undefined {
  const [preview, setPreview] = useState<SourcePreviewResponse | undefined>(undefined);

  useEffect(() => {
    if (frameId == null) {
      setPreview(undefined);
      return;
    }

    let cancelled = false;
    cachedFetchSourcePreview(frameId).then((res) => {
      if (!cancelled) setPreview(res);
    }).catch(() => {
      if (!cancelled) setPreview(undefined);
    });

    return () => { cancelled = true; };
  }, [frameId]);

  return preview;
}
