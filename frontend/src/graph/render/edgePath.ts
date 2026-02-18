import type { Point } from "../geometry";

// Generate SVG path string from polyline points using straight segments.
export function polylineToPath(points: Point[]): string {
  if (points.length < 2) return "";
  const [start, ...rest] = points;
  let d = `M ${start.x} ${start.y}`;
  for (const point of rest) {
    d += ` L ${point.x} ${point.y}`;
  }
  return d;
}

// Same path as polylineToPath — rendered with a wider transparent stroke for hit testing.
export function hitTestPath(points: Point[]): string {
  return polylineToPath(points);
}

// Compute the midpoint along the polyline (by arc length).
export function labelPosition(points: Point[]): Point {
  if (points.length < 2) return points[0] ?? { x: 0, y: 0 };

  let totalLength = 0;
  for (let i = 1; i < points.length; i++) {
    const dx = points[i].x - points[i - 1].x;
    const dy = points[i].y - points[i - 1].y;
    totalLength += Math.hypot(dx, dy);
  }
  if (totalLength <= 0) return points[0];

  const halfway = totalLength / 2;
  let traversed = 0;
  for (let i = 1; i < points.length; i++) {
    const seg0 = points[i - 1];
    const seg1 = points[i];
    const segLength = Math.hypot(seg1.x - seg0.x, seg1.y - seg0.y);
    if (traversed + segLength >= halfway) {
      const remain = halfway - traversed;
      const t = segLength <= 0 ? 0 : remain / segLength;
      return {
        x: seg0.x + (seg1.x - seg0.x) * t,
        y: seg0.y + (seg1.y - seg0.y) * t,
      };
    }
    traversed += segLength;
  }
  return points[Math.floor(points.length / 2)];
}
