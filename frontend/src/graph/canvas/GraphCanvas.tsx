import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import type { GraphGeometry } from "../geometry";
import { type Camera, cameraTransform, screenToWorld } from "./camera";
import { useCameraController } from "./useCameraController";
import "./GraphCanvas.css";

interface CameraContextValue {
  camera: Camera;
  setCamera: (c: Camera) => void;
  fitView: () => void;
  panTo: (worldX: number, worldY: number, durationMs?: number) => void;
  animateCameraTo: (target: Camera, durationMs?: number) => void;
  getManualInteractionVersion: () => number;
  clientToGraph: (clientX: number, clientY: number) => { x: number; y: number } | null;
  viewportWidth: number;
  viewportHeight: number;
}

export const CameraContext = createContext<CameraContextValue | null>(null);

export function useCameraContext(): CameraContextValue {
  const ctx = useContext(CameraContext);
  if (!ctx) throw new Error("useCameraContext must be used inside GraphCanvas");
  return ctx;
}

interface GraphCanvasProps {
  geometry: GraphGeometry | null;
  children?: React.ReactNode;
  className?: string;
  onBackgroundClick?: () => void;
}

export function GraphCanvas({
  geometry,
  children,
  className,
  onBackgroundClick,
}: GraphCanvasProps) {
  const instanceId = useId();
  const dotPatternId = `graph-canvas-dots-${instanceId}`;
  const surfaceRef = useRef<HTMLDivElement>(null);
  const [viewportSize, setViewportSize] = useState({ width: 800, height: 600 });

  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) return;
    const rect = surface.getBoundingClientRect();
    if (rect.width > 0 && rect.height > 0) {
      setViewportSize({ width: rect.width, height: rect.height });
    }
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        setViewportSize({ width, height });
      }
    });
    observer.observe(surface);
    return () => observer.disconnect();
  }, []);

  const {
    camera,
    setCamera,
    fitView,
    panTo,
    animateCameraTo,
    getManualInteractionVersion,
    handlers,
  } = useCameraController(surfaceRef, geometry?.bounds ?? null);

  // Attach wheel listener as non-passive to allow preventDefault
  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) return;
    surface.addEventListener("wheel", handlers.onWheel, { passive: false });
    return () => surface.removeEventListener("wheel", handlers.onWheel);
  }, [handlers.onWheel]);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      handlers.onPointerDown(e.nativeEvent);
    },
    [handlers],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      handlers.onPointerMove(e.nativeEvent);
    },
    [handlers],
  );

  const handlePointerUp = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      handlers.onPointerUp(e.nativeEvent);
    },
    [handlers],
  );

  const handlePointerCancel = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      handlers.onPointerCancel(e.nativeEvent);
    },
    [handlers],
  );

  const handleLostPointerCapture = useCallback(() => {
    handlers.onLostPointerCapture();
  }, [handlers]);

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const target = e.target as Element;
      if (target.closest('[data-pan-block="true"]')) return;
      onBackgroundClick?.();
    },
    [onBackgroundClick],
  );

  const transform = cameraTransform(camera, viewportSize.width, viewportSize.height);
  const dotPatternTransform = `translate(${viewportSize.width / 2 - camera.x * camera.zoom}, ${viewportSize.height / 2 - camera.y * camera.zoom}) scale(${camera.zoom})`;
  const clientToGraph = useCallback(
    (clientX: number, clientY: number) => {
      const surface = surfaceRef.current;
      if (!surface) return null;
      const rect = surface.getBoundingClientRect();
      return screenToWorld(camera, viewportSize.width, viewportSize.height, {
        x: clientX - rect.left,
        y: clientY - rect.top,
      });
    },
    [camera, viewportSize.width, viewportSize.height],
  );

  const cameraContextValue = useMemo(
    () => ({
      camera,
      setCamera,
      fitView,
      panTo,
      animateCameraTo,
      getManualInteractionVersion,
      clientToGraph,
      viewportWidth: viewportSize.width,
      viewportHeight: viewportSize.height,
    }),
    [
      camera,
      setCamera,
      fitView,
      panTo,
      animateCameraTo,
      getManualInteractionVersion,
      clientToGraph,
      viewportSize.width,
      viewportSize.height,
    ],
  );

  return (
    <CameraContext.Provider value={cameraContextValue}>
      <div
        ref={surfaceRef}
        className={`graph-canvas${className ? ` ${className}` : ""}`}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerCancel}
        onLostPointerCapture={handleLostPointerCapture}
        onClick={handleClick}
      >
        <svg className="graph-canvas__background" aria-hidden="true">
          <defs>
            <pattern
              id={dotPatternId}
              width="16"
              height="16"
              patternUnits="userSpaceOnUse"
              patternTransform={dotPatternTransform}
            >
              <circle cx="1" cy="1" r="0.8" className="graph-canvas__dot" />
            </pattern>
          </defs>
          <rect width="100%" height="100%" fill="var(--bg-base)" data-background="true" />
          <rect width="100%" height="100%" fill={`url(#${dotPatternId})`} data-background="true" />
        </svg>
        <div className="graph-canvas__world" style={{ transform }}>
          {children}
        </div>
      </div>
    </CameraContext.Provider>
  );
}
