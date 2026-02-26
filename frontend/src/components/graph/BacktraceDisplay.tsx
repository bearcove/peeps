import React, { useEffect, useMemo, useState } from "react";
import type { GraphFrameData } from "./graphNodeData";
import { FrameLine } from "./GraphNode";
import { cachedFetchSourcePreviews, getSourcePreviewSync } from "../../api/sourceCache";
import "./BacktraceDisplay.css";

export type BacktraceDisplayProps = {
  frames: GraphFrameData[];
  allFrames: GraphFrameData[];
  framesLoading: boolean;
  showSource?: boolean;
  useCompactContext?: boolean;
  hideLocation?: boolean;
  activeFrameIndex?: number;
};

export function BacktraceDisplay({
  frames,
  allFrames,
  framesLoading,
  showSource,
  useCompactContext,
  hideLocation,
  activeFrameIndex,
}: BacktraceDisplayProps) {
  const SCROLL_ANIMATION_MS = 110;
  const [showSystem, setShowSystem] = useState(false);
  const [sourceVersion, setSourceVersion] = useState(0);
  const framesRootRef = React.useRef<HTMLDivElement | null>(null);
  const scrollAnimationRafRef = React.useRef<number | null>(null);
  const prevActiveFrameIndexRef = React.useRef<number | undefined>(activeFrameIndex);

  const hasSystemFrames = allFrames.length > frames.length;
  const displayFrames = showSystem ? allFrames : frames;
  const normalizedActiveFrameIndex =
    displayFrames.length === 0
      ? 0
      : Math.max(0, Math.min(activeFrameIndex ?? 0, displayFrames.length - 1));

  const frameIds = useMemo(
    () => displayFrames.map((f) => f.frame_id).filter((id): id is number => id != null),
    [displayFrames],
  );
  const frameRenderItems = useMemo(() => {
    const seen = new Map<string, number>();
    return displayFrames.map((frame) => {
      const baseKey =
        frame.frame_id != null
          ? `id:${frame.frame_id}`
          : `sig:${frame.function_name}:${frame.source_file}:${frame.line ?? "none"}`;
      const nextCount = (seen.get(baseKey) ?? 0) + 1;
      seen.set(baseKey, nextCount);
      return {
        frame,
        key: `${baseKey}:occurrence:${nextCount}`,
      };
    });
  }, [displayFrames]);

  useEffect(() => {
    if (!showSource || frameIds.length === 0) return;
    const missing = frameIds.filter((id) => getSourcePreviewSync(id) == null);
    if (missing.length === 0) return;
    let cancelled = false;
    void cachedFetchSourcePreviews(missing).then(() => {
      if (!cancelled) setSourceVersion((v) => v + 1);
    });
    return () => {
      cancelled = true;
    };
  }, [frameIds, showSource]);

  useEffect(() => {
    if (activeFrameIndex == null) return;
    const prevActiveFrameIndex = prevActiveFrameIndexRef.current;
    prevActiveFrameIndexRef.current = activeFrameIndex;

    const root = framesRootRef.current;
    if (!root || displayFrames.length === 0) return;
    const sections = root.querySelectorAll<HTMLElement>(".graph-node-frame-section");
    const activeSection = sections[normalizedActiveFrameIndex];
    if (!activeSection) return;
    const scrollContainer = root.closest<HTMLElement>(".graph-node-frames-scroll");
    const targetLine = activeSection.querySelector<HTMLElement>(
      ".graph-node-frame-block__line--target",
    );
    const focusElement = targetLine ?? activeSection;
    if (!scrollContainer) return;
    const containerRect = scrollContainer.getBoundingClientRect();
    const focusRect = focusElement.getBoundingClientRect();
    const zoomScaleY =
      scrollContainer.clientHeight > 0 ? containerRect.height / scrollContainer.clientHeight : 1;
    const normalizedScaleY = Number.isFinite(zoomScaleY) && zoomScaleY > 0 ? zoomScaleY : 1;
    const focusCenterFromContainerTop =
      (focusRect.top - containerRect.top + focusRect.height / 2) / normalizedScaleY;
    const targetScrollTop =
      scrollContainer.scrollTop + focusCenterFromContainerTop - scrollContainer.clientHeight / 2;
    const maxScrollTop = Math.max(0, scrollContainer.scrollHeight - scrollContainer.clientHeight);
    const clampedTarget = Math.max(0, Math.min(maxScrollTop, targetScrollTop));
    const shouldAnimate =
      prevActiveFrameIndex != null &&
      prevActiveFrameIndex !== activeFrameIndex &&
      import.meta.env.MODE !== "test";

    if (scrollAnimationRafRef.current != null) {
      cancelAnimationFrame(scrollAnimationRafRef.current);
      scrollAnimationRafRef.current = null;
    }

    if (!shouldAnimate) {
      scrollContainer.scrollTop = clampedTarget;
      return;
    }

    const start = scrollContainer.scrollTop;
    const delta = clampedTarget - start;
    if (Math.abs(delta) < 1) {
      scrollContainer.scrollTop = clampedTarget;
      return;
    }

    const startedAt = performance.now();
    const tick = (now: number) => {
      const t = Math.min(1, (now - startedAt) / SCROLL_ANIMATION_MS);
      const ease = 1 - (1 - t) ** 3;
      scrollContainer.scrollTop = start + delta * ease;
      if (t < 1) {
        scrollAnimationRafRef.current = requestAnimationFrame(tick);
      } else {
        scrollAnimationRafRef.current = null;
      }
    };
    scrollAnimationRafRef.current = requestAnimationFrame(tick);
  }, [activeFrameIndex, displayFrames, normalizedActiveFrameIndex, sourceVersion]);

  useEffect(() => {
    return () => {
      if (scrollAnimationRafRef.current != null) {
        cancelAnimationFrame(scrollAnimationRafRef.current);
      }
    };
  }, []);

  if (displayFrames.length === 0) {
    if (!framesLoading) return null;
    return (
      <div className="graph-node-frames">
        <div className="graph-node-frame-section graph-node-frame-section--loading">
          <div className="graph-node-frame-sep graph-node-frame-sep--loading">
            <span className="graph-node-frame-sep__name">symbolicating…</span>
            <span className="graph-node-frame-sep__loc">loading source</span>
          </div>
          <pre className="graph-node-frame graph-node-frame--text graph-node-frame--fallback">
            …
          </pre>
        </div>
      </div>
    );
  }

  return (
    <>
      <div className="graph-node-frames" ref={framesRootRef}>
        {frameRenderItems.map(({ frame, key }, index) => (
          <FrameLine
            key={key}
            frame={frame}
            expanded={true}
            showSource={showSource}
            useCompactContext={useCompactContext}
            hideLocation={hideLocation}
            active={index === normalizedActiveFrameIndex}
          />
        ))}
      </div>
      {hasSystemFrames && (
        <div className="backtrace-toolbar" onClick={(e) => e.stopPropagation()}>
          <label className="backtrace-system-toggle">
            <input
              type="checkbox"
              checked={showSystem}
              onChange={(e) => setShowSystem(e.target.checked)}
            />
            Show system frames
          </label>
        </div>
      )}
    </>
  );
}
