import type { Point } from "../geometry";

const ORTHO_CORNER_RADIUS = 3;
const POINT_EPSILON = 0.5;

function distance(a: Point, b: Point): number {
  return Math.hypot(b.x - a.x, b.y - a.y);
}

function unit(from: Point, to: Point): Point {
  const len = distance(from, to);
  if (len === 0) return { x: 0, y: 0 };
  return { x: (to.x - from.x) / len, y: (to.y - from.y) / len };
}

function dedupePoints(points: Point[]): Point[] {
  if (points.length <= 1) return points;
  const out: Point[] = [points[0]];
  for (let i = 1; i < points.length; i++) {
    if (distance(out[out.length - 1], points[i]) > POINT_EPSILON) out.push(points[i]);
  }
  return out;
}

// Generate SVG path string from polyline points using straight segments.
// skipLastCorner: when true, the bend immediately before the final point is
// not rounded — ensures the path arrives at the endpoint on a clean straight
// segment (important when the endpoint has an arrowhead marker).
export function polylineToPath(points: Point[], skipLastCorner = false): string {
  const clean = dedupePoints(points);
  if (clean.length < 2) return "";
  let d = `M ${clean[0].x} ${clean[0].y}`;

  for (let i = 1; i < clean.length; i++) {
    const prev = clean[i - 1];
    const curr = clean[i];
    const next = clean[i + 1];

    if (!next) {
      d += ` L ${curr.x} ${curr.y}`;
      continue;
    }

    const isLastBend = i === clean.length - 2;
    if (isLastBend && skipLastCorner) {
      d += ` L ${curr.x} ${curr.y}`;
      continue;
    }

    const inDir = unit(prev, curr);
    const outDir = unit(curr, next);
    const dot = inDir.x * outDir.x + inDir.y * outDir.y;
    const cross = inDir.x * outDir.y - inDir.y * outDir.x;
    const isNearRightAngle = Math.abs(dot) < 0.35;
    if (!isNearRightAngle || Math.abs(cross) < 0.01) {
      d += ` L ${curr.x} ${curr.y}`;
      continue;
    }

    const inLen = distance(prev, curr);
    const outLen = distance(curr, next);
    const cornerRadius = Math.min(ORTHO_CORNER_RADIUS, inLen / 2, outLen / 2);
    if (cornerRadius <= 0) {
      d += ` L ${curr.x} ${curr.y}`;
      continue;
    }

    const cornerStart = {
      x: curr.x - inDir.x * cornerRadius,
      y: curr.y - inDir.y * cornerRadius,
    };
    const cornerEnd = {
      x: curr.x + outDir.x * cornerRadius,
      y: curr.y + outDir.y * cornerRadius,
    };

    d += ` L ${cornerStart.x} ${cornerStart.y}`;
    d += ` Q ${curr.x} ${curr.y} ${cornerEnd.x} ${cornerEnd.y}`;
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
