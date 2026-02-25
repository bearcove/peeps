import type { Point, Rect } from "../geometry";

export interface Camera {
  x: number; // world X at center of viewport
  y: number; // world Y at center of viewport
  zoom: number; // scale factor (1 = 100%)
}

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 3.0;
export const MAX_FIT_ZOOM = 1.2;

export function worldToScreen(
  camera: Camera,
  viewportWidth: number,
  viewportHeight: number,
  point: Point,
): Point {
  return {
    x: (point.x - camera.x) * camera.zoom + viewportWidth / 2,
    y: (point.y - camera.y) * camera.zoom + viewportHeight / 2,
  };
}

export function screenToWorld(
  camera: Camera,
  viewportWidth: number,
  viewportHeight: number,
  point: Point,
): Point {
  return {
    x: (point.x - viewportWidth / 2) / camera.zoom + camera.x,
    y: (point.y - viewportHeight / 2) / camera.zoom + camera.y,
  };
}

export function cameraTransform(
  camera: Camera,
  viewportWidth: number,
  viewportHeight: number,
): string {
  const tx = viewportWidth / 2 - camera.x * camera.zoom;
  const ty = viewportHeight / 2 - camera.y * camera.zoom;
  return `translate(${tx}px, ${ty}px) scale(${camera.zoom})`;
}

export function fitBounds(
  bounds: Rect,
  viewportWidth: number,
  viewportHeight: number,
  padding = 40,
  maxZoom = MAX_FIT_ZOOM,
): Camera {
  const availW = viewportWidth - 2 * padding;
  const availH = viewportHeight - 2 * padding;
  const zoom = Math.min(
    Math.max(MIN_ZOOM, Math.min(availW / bounds.width, availH / bounds.height)),
    Math.min(maxZoom, MAX_ZOOM),
  );
  return {
    x: bounds.x + bounds.width / 2,
    y: bounds.y + bounds.height / 2,
    zoom,
  };
}
