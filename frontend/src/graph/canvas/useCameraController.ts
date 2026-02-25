import { useCallback, useRef, useState } from "react";
import type { RefObject } from "react";
import type { Rect } from "../geometry";
import { type Camera, MIN_ZOOM, MAX_ZOOM, fitBounds, screenToWorld } from "./camera";

const WHEEL_PIXEL_SENSITIVITY = 0.0042;
const WHEEL_ZOOM_DAMPING_AT_MAX = 0.55;

function normalizeWheelDeltaY(e: WheelEvent): number {
  if (e.deltaMode === WheelEvent.DOM_DELTA_LINE) return e.deltaY * 16;
  if (e.deltaMode === WheelEvent.DOM_DELTA_PAGE) return e.deltaY * 800;
  return e.deltaY;
}

const PAN_DURATION_MS = 300;

export function useCameraController(
  svgRef: RefObject<SVGSVGElement | null>,
  bounds: Rect | null,
): {
  camera: Camera;
  setCamera: (c: Camera) => void;
  fitView: () => void;
  panTo: (worldX: number, worldY: number) => void;
  handlers: {
    onWheel: (e: WheelEvent) => void;
    onPointerDown: (e: PointerEvent) => void;
    onPointerMove: (e: PointerEvent) => void;
    onPointerUp: (e: PointerEvent) => void;
    onPointerCancel: (e: PointerEvent) => void;
    onLostPointerCapture: () => void;
  };
} {
  const [camera, setCamera] = useState<Camera>({ x: 0, y: 0, zoom: 1 });
  const cameraRef = useRef<Camera>(camera);
  cameraRef.current = camera;
  const animFrameRef = useRef<number>(0);

  const panState = useRef<{
    active: boolean;
    startClientX: number;
    startClientY: number;
    startCamera: Camera;
  }>({ active: false, startClientX: 0, startClientY: 0, startCamera: { x: 0, y: 0, zoom: 1 } });

  const getViewportSize = useCallback(() => {
    const svg = svgRef.current;
    if (!svg) return { width: 800, height: 600 };
    const rect = svg.getBoundingClientRect();
    return { width: rect.width, height: rect.height };
  }, [svgRef]);

  const fitView = useCallback(() => {
    if (!bounds) return;
    const { width, height } = getViewportSize();
    setCamera(fitBounds(bounds, width, height, 40));
  }, [bounds, getViewportSize]);

  const onWheel = useCallback(
    (e: WheelEvent) => {
      // If the cursor is over a scrollable block (e.g., an expanded graph node),
      // let it capture all scroll events — never zoom through it.
      const target = e.target as Element;
      if (target.closest('[data-scroll-block="true"]')) return;

      e.preventDefault();
      const svg = svgRef.current;
      if (!svg) return;
      const svgRect = svg.getBoundingClientRect();
      const { width, height } = getViewportSize();

      // Cursor position relative to SVG top-left
      const cursorX = e.clientX - svgRect.left;
      const cursorY = e.clientY - svgRect.top;

      setCamera((prev) => {
        const worldAtCursor = screenToWorld(prev, width, height, {
          x: cursorX,
          y: cursorY,
        });

        const deltaY = normalizeWheelDeltaY(e);
        const zoomProgress = (prev.zoom - MIN_ZOOM) / (MAX_ZOOM - MIN_ZOOM);
        const zoomDamping = 1 - Math.max(0, Math.min(1, zoomProgress)) * WHEEL_ZOOM_DAMPING_AT_MAX;
        const factor = Math.exp(-deltaY * WHEEL_PIXEL_SENSITIVITY * zoomDamping);
        const newZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, prev.zoom * factor));

        return {
          zoom: newZoom,
          x: worldAtCursor.x - (cursorX - width / 2) / newZoom,
          y: worldAtCursor.y - (cursorY - height / 2) / newZoom,
        };
      });
    },
    [getViewportSize, svgRef],
  );

  const onPointerDown = useCallback(
    (e: PointerEvent) => {
      if (e.button !== 0) return;
      const target = e.target as Element;
      const svg = svgRef.current;
      if (!svg) return;
      if (target.closest('[data-pan-block="true"]')) return;
      e.preventDefault();
      if (svg.setPointerCapture) {
        svg.setPointerCapture(e.pointerId);
      }
      panState.current = {
        active: true,
        startClientX: e.clientX,
        startClientY: e.clientY,
        startCamera: camera,
      };
    },
    [camera, svgRef],
  );

  const onPointerMove = useCallback((e: PointerEvent) => {
    const state = panState.current;
    if (!state.active) return;
    const dx = e.clientX - state.startClientX;
    const dy = e.clientY - state.startClientY;
    setCamera({
      ...state.startCamera,
      x: state.startCamera.x - dx / state.startCamera.zoom,
      y: state.startCamera.y - dy / state.startCamera.zoom,
    });
  }, []);

  const onPointerUp = useCallback(
    (e: PointerEvent) => {
      if (!panState.current.active) return;
      panState.current.active = false;
      const svg = svgRef.current;
      if (svg && svg.hasPointerCapture?.(e.pointerId)) {
        svg.releasePointerCapture(e.pointerId);
      }
    },
    [svgRef],
  );

  const onPointerCancel = useCallback(
    (e: PointerEvent) => {
      if (!panState.current.active) return;
      panState.current.active = false;
      const svg = svgRef.current;
      if (svg && svg.hasPointerCapture?.(e.pointerId)) {
        svg.releasePointerCapture(e.pointerId);
      }
    },
    [svgRef],
  );

  const onLostPointerCapture = useCallback(() => {
    panState.current.active = false;
  }, []);

  const panTo = useCallback(
    (worldX: number, worldY: number) => {
      cancelAnimationFrame(animFrameRef.current);
      const startX = cameraRef.current.x;
      const startY = cameraRef.current.y;
      const dx = worldX - startX;
      const dy = worldY - startY;
      if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return;
      const startTime = performance.now();
      const tick = () => {
        const elapsed = performance.now() - startTime;
        const t = Math.min(1, elapsed / PAN_DURATION_MS);
        // ease-out cubic
        const ease = 1 - (1 - t) ** 3;
        setCamera((prev) => ({
          ...prev,
          x: startX + dx * ease,
          y: startY + dy * ease,
        }));
        if (t < 1) {
          animFrameRef.current = requestAnimationFrame(tick);
        }
      };
      animFrameRef.current = requestAnimationFrame(tick);
    },
    [], // stable — reads from cameraRef
  );

  return {
    camera,
    setCamera,
    fitView,
    panTo,
    handlers: {
      onWheel,
      onPointerDown,
      onPointerMove,
      onPointerUp,
      onPointerCancel,
      onLostPointerCapture,
    },
  };
}
