import { useEffect, useState } from "react";
import type { SourcePreviewResponse } from "./types.generated";
import { cachedFetchSourcePreview, getSourcePreviewSync } from "./sourceCache";

export function useSourcePreview(frameId: number | undefined): SourcePreviewResponse | undefined {
  const [preview, setPreview] = useState<SourcePreviewResponse | undefined>(
    () => frameId != null ? getSourcePreviewSync(frameId) : undefined,
  );

  useEffect(() => {
    if (frameId == null) {
      setPreview(undefined);
      return;
    }
    const synced = getSourcePreviewSync(frameId);
    if (synced) {
      setPreview(synced);
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
