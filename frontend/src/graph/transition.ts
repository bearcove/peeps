import type {
  GraphGeometry,
  GeometryEdge,
  GeometryGroup,
  GeometryNode,
  Point,
  Rect,
} from "./geometry";

export type InterpolatedGraph = {
  geometry: GraphGeometry;
  nodes: GeometryNode[];
  groups: GeometryGroup[];
  nodeOpacityById: Map<string, number>;
  groupOpacityById: Map<string, number>;
  edgeOpacityById: Map<string, number>;
};

const EDGE_SAMPLE_POINTS = 24;

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}

export function easeOutCubic(t: number): number {
  return 1 - (1 - t) ** 3;
}

function lerpPoint(a: Point, b: Point, t: number): Point {
  return { x: lerp(a.x, b.x, t), y: lerp(a.y, b.y, t) };
}

function lerpRect(a: Rect, b: Rect, t: number): Rect {
  return {
    x: lerp(a.x, b.x, t),
    y: lerp(a.y, b.y, t),
    width: lerp(a.width, b.width, t),
    height: lerp(a.height, b.height, t),
  };
}

function assertValidRect(rect: Rect, context: string): void {
  const { x, y, width, height } = rect;
  if (
    !Number.isFinite(x) ||
    !Number.isFinite(y) ||
    !Number.isFinite(width) ||
    !Number.isFinite(height)
  ) {
    throw new Error(
      `[graph-transition] ${context} has non-finite rect ${JSON.stringify({ x, y, width, height })}`,
    );
  }
  if (width <= 0 || height <= 0) {
    throw new Error(
      `[graph-transition] ${context} has non-positive rect ${JSON.stringify({ x, y, width, height })}`,
    );
  }
}

function dist(a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  return Math.hypot(dx, dy);
}

function polylineLength(points: Point[]): number {
  if (points.length < 2) return 0;
  let total = 0;
  for (let i = 1; i < points.length; i++) total += dist(points[i - 1], points[i]);
  return total;
}

function pointAtDistance(points: Point[], distanceAlong: number): Point {
  if (points.length === 0) return { x: 0, y: 0 };
  if (points.length === 1) return points[0];
  let remaining = Math.max(0, distanceAlong);
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1];
    const b = points[i];
    const segLen = dist(a, b);
    if (segLen <= 0) continue;
    if (remaining <= segLen) {
      const t = remaining / segLen;
      return lerpPoint(a, b, t);
    }
    remaining -= segLen;
  }
  return points[points.length - 1];
}

function resamplePolyline(points: Point[], sampleCount: number): Point[] {
  if (sampleCount < 2)
    throw new Error(`[graph-transition] sampleCount must be >= 2, got ${sampleCount}`);
  if (points.length < 2) return points;
  const length = polylineLength(points);
  if (length <= 0) return points;
  const out: Point[] = [];
  for (let i = 0; i < sampleCount; i++) {
    const d = (length * i) / (sampleCount - 1);
    out.push(pointAtDistance(points, d));
  }
  return out;
}

function interpolatePolyline(from: Point[], to: Point[], t: number): Point[] {
  if (from.length < 2 || to.length < 2) return to;
  // Same topology: interpolate waypoint-to-waypoint directly.
  // This preserves the orthogonal structure so polylineToPath can round corners correctly.
  // Resampling both paths to a common point count breaks structural correspondence and
  // produces phantom kinks where sample #N lands on different legs of each path.
  if (from.length === to.length) {
    return from.map((p, i) => lerpPoint(p, to[i], t));
  }
  // Different topology (waypoint count changed): fall back to arc-length resampling.
  const fromResampled = resamplePolyline(from, EDGE_SAMPLE_POINTS);
  const toResampled = resamplePolyline(to, EDGE_SAMPLE_POINTS);
  if (fromResampled.length !== toResampled.length) {
    throw new Error(
      `[graph-transition] mismatched resampled polyline lengths: ${fromResampled.length} vs ${toResampled.length}`,
    );
  }
  const out: Point[] = [];
  for (let i = 0; i < fromResampled.length; i++) {
    out.push(lerpPoint(fromResampled[i], toResampled[i], t));
  }
  return out;
}

function orderedUnionIds(primaryIds: string[], secondaryIds: string[]): string[] {
  const seen = new Set<string>();
  const ordered: string[] = [];
  for (const id of primaryIds) {
    if (seen.has(id)) continue;
    seen.add(id);
    ordered.push(id);
  }
  for (const id of secondaryIds) {
    if (seen.has(id)) continue;
    seen.add(id);
    ordered.push(id);
  }
  return ordered;
}

function boundsForNodesAndGroups(nodes: GeometryNode[], groups: GeometryGroup[]): Rect {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const node of nodes) {
    minX = Math.min(minX, node.worldRect.x);
    minY = Math.min(minY, node.worldRect.y);
    maxX = Math.max(maxX, node.worldRect.x + node.worldRect.width);
    maxY = Math.max(maxY, node.worldRect.y + node.worldRect.height);
  }
  for (const group of groups) {
    minX = Math.min(minX, group.worldRect.x);
    minY = Math.min(minY, group.worldRect.y);
    maxX = Math.max(maxX, group.worldRect.x + group.worldRect.width);
    maxY = Math.max(maxY, group.worldRect.y + group.worldRect.height);
  }

  if (!Number.isFinite(minX)) return { x: 0, y: 0, width: 0, height: 0 };
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}

export function interpolateGraph(
  fromGeometry: GraphGeometry,
  toGeometry: GraphGeometry,
  fromNodes: GeometryNode[],
  toNodes: GeometryNode[],
  fromGroups: GeometryGroup[],
  toGroups: GeometryGroup[],
  rawT: number,
): InterpolatedGraph {
  const t = easeOutCubic(clamp01(rawT));

  const fromNodeById = new Map(fromNodes.map((n) => [n.id, n]));
  const toNodeById = new Map(toNodes.map((n) => [n.id, n]));
  const nodeIds = orderedUnionIds(
    toNodes.map((n) => n.id),
    fromNodes.map((n) => n.id),
  );
  const nodeOpacityById = new Map<string, number>();
  const nodes: GeometryNode[] = [];
  for (const id of nodeIds) {
    const fromNode = fromNodeById.get(id);
    const toNode = toNodeById.get(id);
    if (fromNode && toNode) {
      assertValidRect(fromNode.worldRect, `from node ${id}`);
      assertValidRect(toNode.worldRect, `to node ${id}`);
      nodes.push({ ...toNode, worldRect: lerpRect(fromNode.worldRect, toNode.worldRect, t) });
      nodeOpacityById.set(id, 1);
    } else if (toNode) {
      assertValidRect(toNode.worldRect, `to node ${id}`);
      nodes.push(toNode);
      nodeOpacityById.set(id, t);
    } else if (fromNode) {
      assertValidRect(fromNode.worldRect, `from node ${id}`);
      nodes.push(fromNode);
      nodeOpacityById.set(id, 1 - t);
    }
  }

  const fromGroupById = new Map(fromGroups.map((g) => [g.id, g]));
  const toGroupById = new Map(toGroups.map((g) => [g.id, g]));
  const groupIds = orderedUnionIds(
    toGroups.map((g) => g.id),
    fromGroups.map((g) => g.id),
  );
  const groupOpacityById = new Map<string, number>();
  const groups: GeometryGroup[] = [];
  for (const id of groupIds) {
    const fromGroup = fromGroupById.get(id);
    const toGroup = toGroupById.get(id);
    if (fromGroup && toGroup) {
      assertValidRect(fromGroup.worldRect, `from group ${id}`);
      assertValidRect(toGroup.worldRect, `to group ${id}`);
      const fromLabelRect = fromGroup.labelRect;
      const toLabelRect = toGroup.labelRect;
      const labelRect =
        fromLabelRect && toLabelRect
          ? lerpRect(fromLabelRect, toLabelRect, t)
          : (toLabelRect ?? fromLabelRect);
      groups.push({
        ...toGroup,
        worldRect: lerpRect(fromGroup.worldRect, toGroup.worldRect, t),
        labelRect,
      });
      groupOpacityById.set(id, 1);
    } else if (toGroup) {
      assertValidRect(toGroup.worldRect, `to group ${id}`);
      groups.push(toGroup);
      groupOpacityById.set(id, t);
    } else if (fromGroup) {
      assertValidRect(fromGroup.worldRect, `from group ${id}`);
      groups.push(fromGroup);
      groupOpacityById.set(id, 1 - t);
    }
  }

  const fromEdgeById = new Map(fromGeometry.edges.map((e) => [e.id, e]));
  const toEdgeById = new Map(toGeometry.edges.map((e) => [e.id, e]));
  const edgeIds = orderedUnionIds(
    toGeometry.edges.map((e) => e.id),
    fromGeometry.edges.map((e) => e.id),
  );
  const edgeOpacityById = new Map<string, number>();
  const edges: GeometryEdge[] = [];
  for (const id of edgeIds) {
    const fromEdge = fromEdgeById.get(id);
    const toEdge = toEdgeById.get(id);
    if (fromEdge && toEdge) {
      edges.push({
        ...toEdge,
        polyline: interpolatePolyline(fromEdge.polyline, toEdge.polyline, t),
      });
      edgeOpacityById.set(id, 1);
    } else if (toEdge) {
      edges.push(toEdge);
      edgeOpacityById.set(id, t);
    } else if (fromEdge) {
      edges.push(fromEdge);
      edgeOpacityById.set(id, 1 - t);
    }
  }

  const portAnchors = new Map<string, Point>();
  const portAnchorIds = new Set<string>([
    ...fromGeometry.portAnchors.keys(),
    ...toGeometry.portAnchors.keys(),
  ]);
  for (const id of portAnchorIds) {
    const fromAnchor = fromGeometry.portAnchors.get(id);
    const toAnchor = toGeometry.portAnchors.get(id);
    if (fromAnchor && toAnchor) {
      portAnchors.set(id, lerpPoint(fromAnchor, toAnchor, t));
    } else if (toAnchor) {
      portAnchors.set(id, toAnchor);
    } else if (fromAnchor) {
      portAnchors.set(id, fromAnchor);
    }
  }

  const geometry: GraphGeometry = {
    nodes,
    groups,
    edges,
    bounds: boundsForNodesAndGroups(nodes, groups),
    portAnchors,
  };

  return { geometry, nodes, groups, nodeOpacityById, groupOpacityById, edgeOpacityById };
}
