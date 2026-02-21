import React, { createContext, useCallback, useContext, useEffect, useId, useMemo, useRef, useState } from "react";
import type { GraphGeometry } from "../geometry";
import { type Camera, cameraTransform } from "./camera";
import { useCameraController } from "./useCameraController";
import "./GraphCanvas.css";

interface CameraContextValue {
  camera: Camera;
  setCamera: (c: Camera) => void;
  fitView: () => void;
  clientToGraph: (clientX: number, clientY: number) => { x: number; y: number } | null;
  viewportWidth: number;
  viewportHeight: number;
  markerUrl: (size: number) => string;
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
  const arrowhead8Id = `arrowhead-8-${instanceId}`;
  const arrowhead10Id = `arrowhead-10-${instanceId}`;
  const arrowhead14Id = `arrowhead-14-${instanceId}`;
  const svgRef = useRef<SVGSVGElement>(null);
  const worldRef = useRef<SVGGElement>(null);
  const [viewportSize, setViewportSize] = useState({ width: 800, height: 600 });

  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    const rect = svg.getBoundingClientRect();
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
    observer.observe(svg);
    return () => observer.disconnect();
  }, []);

  const { camera, setCamera, fitView, handlers } = useCameraController(
    svgRef,
    geometry?.bounds ?? null,
  );

  // Attach wheel listener as non-passive to allow preventDefault
  useEffect(() => {
    const svg = svgRef.current;
    if (!svg) return;
    svg.addEventListener("wheel", handlers.onWheel, { passive: false });
    return () => svg.removeEventListener("wheel", handlers.onWheel);
  }, [handlers.onWheel]);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent<SVGSVGElement>) => {
      handlers.onPointerDown(e.nativeEvent);
    },
    [handlers],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent<SVGSVGElement>) => {
      handlers.onPointerMove(e.nativeEvent);
    },
    [handlers],
  );

  const handlePointerUp = useCallback(
    (e: React.PointerEvent<SVGSVGElement>) => {
      handlers.onPointerUp(e.nativeEvent);
    },
    [handlers],
  );

  const handlePointerCancel = useCallback(
    (e: React.PointerEvent<SVGSVGElement>) => {
      handlers.onPointerCancel(e.nativeEvent);
    },
    [handlers],
  );

  const handleLostPointerCapture = useCallback(() => {
    handlers.onLostPointerCapture();
  }, [handlers]);

  const handleClick = useCallback(
    (e: React.MouseEvent<SVGSVGElement>) => {
      const target = e.target as Element;
      const svg = svgRef.current;
      if (!svg) return;
      if (target === svg || target.getAttribute("data-background") === "true") {
        onBackgroundClick?.();
      }
    },
    [onBackgroundClick],
  );

  const transform = cameraTransform(camera, viewportSize.width, viewportSize.height);
  const dotPatternTransform = `translate(${viewportSize.width / 2 - camera.x * camera.zoom}, ${viewportSize.height / 2 - camera.y * camera.zoom}) scale(${camera.zoom})`;
  const clientToGraph = useCallback((clientX: number, clientY: number) => {
    const svg = svgRef.current;
    const world = worldRef.current;
    if (!svg || !world) return null;
    const ctm = world.getScreenCTM();
    if (!ctm) return null;
    const point = new DOMPoint(clientX, clientY).matrixTransform(ctm.inverse());
    return { x: point.x, y: point.y };
  }, []);

  const markerUrl = useCallback((size: number) => `url(#arrowhead-${size}-${instanceId})`, [instanceId]);

  const cameraContextValue = useMemo(
    () => ({
      camera,
      setCamera,
      fitView,
      clientToGraph,
      viewportWidth: viewportSize.width,
      viewportHeight: viewportSize.height,
      markerUrl,
    }),
    [camera, setCamera, fitView, clientToGraph, viewportSize.width, viewportSize.height, markerUrl],
  );

  return (
    <CameraContext.Provider value={cameraContextValue}>
      <div className={`graph-canvas${className ? ` ${className}` : ""}`}>
        <svg
          ref={svgRef}
          className="graph-canvas__svg"
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerCancel={handlePointerCancel}
          onLostPointerCapture={handleLostPointerCapture}
          onClick={handleClick}
        >
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
            <marker id={arrowhead8Id} markerWidth="8" markerHeight="8" refX="0" refY="4" orient="auto" markerUnits="userSpaceOnUse">
              <path d="M 0 0 L 8 4 L 0 8 Z" fill="context-stroke" />
            </marker>
            <marker id={arrowhead10Id} markerWidth="10" markerHeight="10" refX="0" refY="5" orient="auto" markerUnits="userSpaceOnUse">
              <path d="M 0 0 L 10 5 L 0 10 Z" fill="context-stroke" />
            </marker>
            <marker id={arrowhead14Id} markerWidth="14" markerHeight="14" refX="0" refY="7" orient="auto" markerUnits="userSpaceOnUse">
              <path d="M 0 0 L 14 7 L 0 14 Z" fill="context-stroke" />
            </marker>
          </defs>
          <rect
            width="100%"
            height="100%"
            fill="var(--bg-base)"
            data-background="true"
          />
          <rect
            width="100%"
            height="100%"
            fill={`url(#${dotPatternId})`}
            data-background="true"
          />
          <g ref={worldRef} transform={transform}>{children}</g>
        </svg>
      </div>
    </CameraContext.Provider>
  );
}
