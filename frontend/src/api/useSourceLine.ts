import { useEffect, useState } from "react";
import { cachedFetchSourcePreview, getSourceLineSync } from "./sourceCache";

export function useSourceLine(frameId: number | undefined): string | undefined {
  const [html, setHtml] = useState<string | undefined>(
    () => frameId != null ? getSourceLineSync(frameId) : undefined,
  );

  useEffect(() => {
    if (frameId == null) {
      setHtml(undefined);
      return;
    }
    const synced = getSourceLineSync(frameId);
    if (synced !== undefined) {
      setHtml(synced);
      return;
    }
    let cancelled = false;
    cachedFetchSourcePreview(frameId).then(() => {
      if (!cancelled) setHtml(getSourceLineSync(frameId));
    }).catch(() => {
      if (!cancelled) setHtml(undefined);
    });
    return () => { cancelled = true; };
  }, [frameId]);

  return html;
}
