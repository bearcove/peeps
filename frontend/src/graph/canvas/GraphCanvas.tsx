import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
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
  getCamera: () => Camera;
  getViewportSize: () => { width: number; height: number };
  getAnimationDestination: () => Camera | null;
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
    getCamera,
    getViewportSize,
    getAnimationDestination,
    handlers,
  } = useCameraController(surfaceRef, geometry?.bounds ?? null, onBackgroundClick);

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

  const transform = cameraTransform(camera, viewportSize.width, viewportSize.height);
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
      getCamera,
      getViewportSize,
      getAnimationDestination,
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
      getCamera,
      getViewportSize,
      getAnimationDestination,
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
      >
        <svg className="graph-canvas__background" aria-hidden="true">
          <rect width="100%" height="100%" fill="var(--bg-base)" data-background="true" />
        </svg>
        <div className="graph-canvas__grain" aria-hidden="true" />
        <div className="graph-canvas__world" style={{ transform }}>
          {children}
        </div>
      </div>
    </CameraContext.Provider>
  );
}
