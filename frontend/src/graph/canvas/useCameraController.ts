import { useCallback, useRef, useState } from "react";
import type { RefObject } from "react";
import type { Rect } from "../geometry";
import { type Camera, MIN_ZOOM, MAX_ZOOM, fitBounds, screenToWorld } from "./camera";

const WHEEL_PIXEL_SENSITIVITY = 0.0042;
const WHEEL_ZOOM_DAMPING_AT_MAX = 0.55;
const DRAG_THRESHOLD_PX = 4;

function normalizeWheelDeltaY(e: WheelEvent): number {
  if (e.deltaMode === WheelEvent.DOM_DELTA_LINE) return e.deltaY * 16;
  if (e.deltaMode === WheelEvent.DOM_DELTA_PAGE) return e.deltaY * 800;
  return e.deltaY;
}

const PAN_DURATION_MS = 300;

export function useCameraController(
  surfaceRef: RefObject<HTMLElement | null>,
  bounds: Rect | null,
  onBackgroundClick?: () => void,
): {
  camera: Camera;
  setCamera: (c: Camera) => void;
  fitView: () => void;
  panTo: (worldX: number, worldY: number, durationMs?: number) => void;
  animateCameraTo: (target: Camera, durationMs?: number) => void;
  getManualInteractionVersion: () => number;
  /** Returns the camera at the current moment, bypassing React render batching. */
  getCamera: () => Camera;
  /** Returns the destination of the in-progress animation, or null if the camera is settled. */
  getAnimationDestination: () => Camera | null;
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
  const manualInteractionVersionRef = useRef(0);
  const onBackgroundClickRef = useRef(onBackgroundClick);
  onBackgroundClickRef.current = onBackgroundClick;
  // Destination of the current animation; null when the camera is settled.
  const animationDestinationRef = useRef<Camera | null>(null);

  const panState = useRef<{
    active: boolean;
    startClientX: number;
    startClientY: number;
    startCamera: Camera;
  }>({ active: false, startClientX: 0, startClientY: 0, startCamera: { x: 0, y: 0, zoom: 1 } });
  // True once the pointer moves past the drag threshold during a pan.
  const didDragRef = useRef(false);

  const getViewportSize = useCallback(() => {
    const surface = surfaceRef.current;
    if (!surface) return { width: 800, height: 600 };
    const rect = surface.getBoundingClientRect();
    return { width: rect.width, height: rect.height };
  }, [surfaceRef]);

  const fitView = useCallback(() => {
    if (!bounds) return;
    const { width, height } = getViewportSize();
    setCamera(fitBounds(bounds, width, height, 40));
  }, [bounds, getViewportSize]);

  const animateCameraTo = useCallback((target: Camera, durationMs: number = PAN_DURATION_MS) => {
    cancelAnimationFrame(animFrameRef.current);
    const start = cameraRef.current;
    const dx = target.x - start.x;
    const dy = target.y - start.y;
    const dz = target.zoom - start.zoom;
    if (Math.abs(dx) < 1 && Math.abs(dy) < 1 && Math.abs(dz) < 0.001) return;
    animationDestinationRef.current = target;
    const startTime = performance.now();
    const tick = () => {
      const elapsed = performance.now() - startTime;
      const t = durationMs <= 0 ? 1 : Math.min(1, elapsed / durationMs);
      // ease-out cubic
      const ease = 1 - (1 - t) ** 3;
      setCamera({
        x: start.x + dx * ease,
        y: start.y + dy * ease,
        zoom: start.zoom + dz * ease,
      });
      if (t < 1) {
        animFrameRef.current = requestAnimationFrame(tick);
      } else {
        animationDestinationRef.current = null;
      }
    };
    animFrameRef.current = requestAnimationFrame(tick);
  }, []);

  const getManualInteractionVersion = useCallback(() => manualInteractionVersionRef.current, []);
  const getCamera = useCallback(() => cameraRef.current, []);
  const getAnimationDestination = useCallback(() => animationDestinationRef.current, []);

  const onWheel = useCallback(
    (e: WheelEvent) => {
      // If the cursor is over a scrollable block (e.g., an expanded graph node),
      // let it capture all scroll events — never zoom through it.
      const target = e.target as Element;
      if (target.closest('[data-scroll-block="true"]')) return;

      e.preventDefault();
      cancelAnimationFrame(animFrameRef.current);
      animationDestinationRef.current = null;
      manualInteractionVersionRef.current += 1;
      const surface = surfaceRef.current;
      if (!surface) return;
      const surfaceRect = surface.getBoundingClientRect();
      const { width, height } = getViewportSize();

      // Cursor position relative to canvas top-left
      const cursorX = e.clientX - surfaceRect.left;
      const cursorY = e.clientY - surfaceRect.top;

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
    [getViewportSize, surfaceRef],
  );

  const onPointerDown = useCallback(
    (e: PointerEvent) => {
      if (e.button !== 0) return;
      const target = e.target as Element;
      const surface = surfaceRef.current;
      if (!surface) return;
      if (target.closest('[data-pan-block="true"]')) return;
      e.preventDefault();
      cancelAnimationFrame(animFrameRef.current);
      animationDestinationRef.current = null;
      if (surface.setPointerCapture) {
        surface.setPointerCapture(e.pointerId);
      }
      panState.current = {
        active: true,
        startClientX: e.clientX,
        startClientY: e.clientY,
        startCamera: camera,
      };
      didDragRef.current = false;
    },
    [camera, surfaceRef],
  );

  const onPointerMove = useCallback((e: PointerEvent) => {
    const state = panState.current;
    if (!state.active) return;
    animationDestinationRef.current = null;
    manualInteractionVersionRef.current += 1;
    const dx = e.clientX - state.startClientX;
    const dy = e.clientY - state.startClientY;
    if (!didDragRef.current && (Math.abs(dx) > DRAG_THRESHOLD_PX || Math.abs(dy) > DRAG_THRESHOLD_PX)) {
      didDragRef.current = true;
    }
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
      const surface = surfaceRef.current;
      if (surface && surface.hasPointerCapture?.(e.pointerId)) {
        surface.releasePointerCapture(e.pointerId);
      }
      if (!didDragRef.current) {
        onBackgroundClickRef.current?.();
      }
      didDragRef.current = false;
    },
    [surfaceRef],
  );

  const onPointerCancel = useCallback(
    (e: PointerEvent) => {
      if (!panState.current.active) return;
      panState.current.active = false;
      didDragRef.current = false;
      const surface = surfaceRef.current;
      if (surface && surface.hasPointerCapture?.(e.pointerId)) {
        surface.releasePointerCapture(e.pointerId);
      }
    },
    [surfaceRef],
  );

  const onLostPointerCapture = useCallback(() => {
    panState.current.active = false;
    didDragRef.current = false;
  }, []);

  const panTo = useCallback(
    (worldX: number, worldY: number, durationMs?: number) => {
      animateCameraTo(
        {
          ...cameraRef.current,
          x: worldX,
          y: worldY,
        },
        durationMs,
      );
    },
    [animateCameraTo],
  );

  return {
    camera,
    setCamera,
    fitView,
    panTo,
    animateCameraTo,
    getManualInteractionVersion,
    getCamera,
    getAnimationDestination,
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
