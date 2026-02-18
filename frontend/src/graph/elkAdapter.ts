import ELK from "elkjs/lib/elk-api.js";
import elkWorkerUrl from "elkjs/lib/elk-worker.min.js?url";
import type { EntityDef, EdgeDef } from "../snapshot";
import type { GraphGeometry, GeometryNode, GeometryGroup, GeometryEdge, Point } from "./geometry";

// ── ELK layout ────────────────────────────────────────────────

const elk = new ELK({ workerUrl: elkWorkerUrl });

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "24",
  "elk.layered.spacing.nodeNodeBetweenLayers": "48",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
  "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
};

const subgraphElkPadding = "[top=30,left=12,bottom=12,right=12]";

// ── Edge styling ──────────────────────────────────────────────

export type EdgeStyle = {
  stroke?: string;
  strokeWidth?: number;
  strokeDasharray?: string;
};

export function edgeStyle(edge: EdgeDef): EdgeStyle {
  const isPendingOp = edge.kind === "needs" && edge.state === "pending";
  if (isPendingOp) {
    return { stroke: "light-dark(#d7263d, #ff6b81)", strokeWidth: 2.8 };
  }
  const kind = edge.kind;
  switch (kind) {
    case "touches":
      return { stroke: "light-dark(#4f8f8f, #78b8b8)", strokeWidth: 1.2, strokeDasharray: "4 4" };
    case "needs":
      return { stroke: "light-dark(#d7263d, #ff6b81)", strokeWidth: 2.4 };
    case "holds":
      return { stroke: "light-dark(#2f6fed, #7aa2ff)", strokeWidth: 2 };
    case "polls":
      return { stroke: "light-dark(#8e7cc3, #b4a7d6)", strokeWidth: 1.2, strokeDasharray: "2 3" };
    case "closed_by":
      return { stroke: "light-dark(#e08614, #f0a840)", strokeWidth: 1.5 };
    case "channel_link":
      return { stroke: "light-dark(#888, #666)", strokeWidth: 1, strokeDasharray: "6 3" };
    case "rpc_link":
      return { stroke: "light-dark(#888, #666)", strokeWidth: 1, strokeDasharray: "6 3" };
  }
}

export function edgeTooltip(edge: EdgeDef, sourceName: string, targetName: string): string {
  const kind = edge.kind;
  if (kind === "needs" && edge.opKind) {
    const op = edge.opKind.replaceAll("_", " ");
    if (edge.state === "pending") {
      return `${sourceName} is blocked on ${op} for ${targetName}`;
    }
    return `${sourceName} is performing ${op} on ${targetName}`;
  }
  switch (kind) {
    case "touches":
      return `${sourceName} has touched ${targetName}`;
    case "needs":
      return `${sourceName} is blocked waiting for ${targetName}`;
    case "holds":
      return `${sourceName} currently grants permits to ${targetName}`;
    case "polls":
      return `${sourceName} polls ${targetName} (non-blocking)`;
    case "closed_by":
      return `${sourceName} was closed by ${targetName}`;
    case "channel_link":
      return `Channel endpoint: ${sourceName} → ${targetName}`;
    case "rpc_link":
      return `RPC pair: ${sourceName} → ${targetName}`;
  }
}

export function edgeMarkerSize(edge: EdgeDef): number {
  return edge.kind === "needs" ? (edge.state === "pending" ? 14 : 10) : 8;
}

export function edgeLabel(edge: EdgeDef): string | undefined {
  if (edge.kind !== "needs" || !edge.opKind) return undefined;
  return edge.opKind.replaceAll("_", " ");
}

// ── Types ─────────────────────────────────────────────────────

export type SubgraphScopeMode = "none" | "process" | "crate";

// ── Layout ────────────────────────────────────────────────────

export async function layoutGraph(
  entityDefs: EntityDef[],
  edgeDefs: EdgeDef[],
  nodeSizes: Map<string, { width: number; height: number }>,
  subgraphScopeMode: SubgraphScopeMode = "none",
): Promise<GraphGeometry> {
  const nodeIds = new Set(entityDefs.map((n) => n.id));
  const validEdges = edgeDefs.filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target));

  const entityById = new Map(entityDefs.map((entity) => [entity.id, entity]));
  const groupKeyFor = (entity: EntityDef): string | null => {
    if (subgraphScopeMode === "process") return entity.processId;
    if (subgraphScopeMode === "crate") return entity.krate ?? "~no-crate";
    return null;
  };

  const hasSubgraphs = subgraphScopeMode !== "none";
  const groupedChildren = (() => {
    if (!hasSubgraphs) {
      return entityDefs.map((entity) => {
        const size = nodeSizes.get(entity.id);
        return {
          id: entity.id,
          width: size?.width || 150,
          height: size?.height || 36,
        };
      });
    }

    const grouped = new Map<string, EntityDef[]>();
    for (const entity of entityDefs) {
      const key = groupKeyFor(entity) ?? "~unknown";
      if (!grouped.has(key)) grouped.set(key, []);
      grouped.get(key)!.push(entity);
    }

    return Array.from(grouped.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([key, members]) => ({
        id: `scope-group:${subgraphScopeMode}:${key}`,
        layoutOptions: {
          "elk.padding": subgraphElkPadding,
        },
        children: members.map((entity) => {
          const size = nodeSizes.get(entity.id);
          return {
            id: entity.id,
            width: size?.width || 150,
            height: size?.height || 36,
          };
        }),
      }));
  })();

  const result = await elk.layout({
    id: "root",
    layoutOptions: {
      ...elkOptions,
      ...(hasSubgraphs ? { "elk.hierarchyHandling": "INCLUDE_CHILDREN" } : {}),
    },
    children: groupedChildren,
    edges: validEdges.map((e) => ({
      id: e.id,
      sources: [e.source],
      targets: [e.target],
    })),
  });

  type ElkSectionLike = {
    id?: string;
    startPoint?: Point;
    endPoint?: Point;
    bendPoints?: Point[];
    incomingSections?: string[];
    outgoingSections?: string[];
  };
  type CollectedElkEdge = {
    graphOffset: { x: number; y: number };
    depth: number;
    sources: string[];
    targets: string[];
    sections: ElkSectionLike[];
  };
  const edgeLayoutsById = new Map<string, CollectedElkEdge[]>();

  const collectEdgeLayouts = (
    graph: any,
    depth: number,
    graphOffset: { x: number; y: number },
  ) => {
    for (const edge of graph.edges ?? []) {
      const collected: CollectedElkEdge = {
        graphOffset: { x: graphOffset.x, y: graphOffset.y },
        depth,
        sources: (edge.sources ?? []).map(String),
        targets: (edge.targets ?? []).map(String),
        sections: (edge.sections ?? []) as ElkSectionLike[],
      };
      if (!edgeLayoutsById.has(edge.id)) edgeLayoutsById.set(edge.id, []);
      edgeLayoutsById.get(edge.id)!.push(collected);
    }

    for (const child of graph.children ?? []) {
      collectEdgeLayouts(child, depth + 1, {
        x: graphOffset.x + (child.x ?? 0),
        y: graphOffset.y + (child.y ?? 0),
      });
    }
  };

  collectEdgeLayouts(result, 0, { x: 0, y: 0 });

  const orderSections = (sections: ElkSectionLike[]): ElkSectionLike[] => {
    if (sections.length <= 1) return sections;
    const byId = new Map<string, ElkSectionLike>();
    for (const section of sections) {
      if (section.id) byId.set(section.id, section);
    }
    if (byId.size === 0) return sections;

    const roots = sections.filter((section) => {
      const incoming = section.incomingSections ?? [];
      if (incoming.length === 0) return true;
      return !incoming.some((id) => byId.has(id));
    });

    const ordered: ElkSectionLike[] = [];
    const visited = new Set<string>();
    const visit = (section: ElkSectionLike) => {
      if (!section.id || visited.has(section.id)) return;
      visited.add(section.id);
      ordered.push(section);
      for (const outId of section.outgoingSections ?? []) {
        const next = byId.get(outId);
        if (next) visit(next);
      }
    };

    for (const root of roots) {
      if (!root.id) continue;
      visit(root);
    }
    for (const section of sections) {
      if (!section.id || !visited.has(section.id)) ordered.push(section);
    }
    return ordered.length === 0 ? sections : ordered;
  };

  const geoNodes: GeometryNode[] = [];
  const geoGroups: GeometryGroup[] = [];

  // Track absolute positions for each entity node (needed for group member listing)
  const absoluteNodePos = new Map<string, { x: number; y: number }>();

  const makeNodeData = (def: EntityDef): { kind: string; data: any } => {
    if (def.channelPair) {
      return {
        kind: "channelPairNode",
        data: {
          tx: def.channelPair.tx,
          rx: def.channelPair.rx,
          channelName: def.name,
          selected: false,
          statTone: def.statTone,
        },
      };
    }
    if (def.rpcPair) {
      return {
        kind: "rpcPairNode",
        data: {
          req: def.rpcPair.req,
          resp: def.rpcPair.resp,
          rpcName: def.name,
          selected: false,
        },
      };
    }
    return {
      kind: "mockNode",
      data: {
        kind: def.kind,
        label: def.name,
        inCycle: def.inCycle,
        selected: false,
        status: def.status,
        ageMs: def.ageMs,
        stat: def.stat,
        statTone: def.statTone,
      },
    };
  };

  const walkChildren = (
    children: any[] | undefined,
    parentOffset: { x: number; y: number } = { x: 0, y: 0 },
  ) => {
    for (const child of children ?? []) {
      const isGroupNode = typeof child.id === "string" && child.id.startsWith("scope-group:");
      const localX = child.x ?? 0;
      const localY = child.y ?? 0;
      const absX = parentOffset.x + localX;
      const absY = parentOffset.y + localY;

      if (isGroupNode) {
        const groupId = String(child.id);
        const firstColon = groupId.indexOf(":");
        const secondColon = groupId.indexOf(":", firstColon + 1);
        const scopeKind = secondColon > -1 ? groupId.slice(firstColon + 1, secondColon) : "scope";
        const rawScopeKey = secondColon > -1 ? groupId.slice(secondColon + 1) : groupId;
        const memberIds = (child.children ?? [])
          .filter((c: any) => !String(c.id).startsWith("scope-group:"))
          .map((c: any) => String(c.id));

        geoGroups.push({
          id: groupId,
          scopeKind,
          label: rawScopeKey === "~no-crate" ? "(no crate)" : rawScopeKey,
          worldRect: {
            x: absX,
            y: absY,
            width: child.width ?? 260,
            height: child.height ?? 180,
          },
          members: memberIds,
          data: {
            scopeKind,
            scopeKey: rawScopeKey,
            count: memberIds.length,
          },
        });

        walkChildren(child.children, { x: absX, y: absY });
        continue;
      }

      const entity = entityById.get(child.id);
      if (!entity) continue;

      absoluteNodePos.set(entity.id, { x: absX, y: absY });

      const size = nodeSizes.get(entity.id) ?? { width: 150, height: 36 };
      const { kind, data } = makeNodeData(entity);
      geoNodes.push({
        id: entity.id,
        kind,
        worldRect: { x: absX, y: absY, width: size.width, height: size.height },
        data,
      });
    }
  };

  walkChildren(result.children, { x: 0, y: 0 });

  type SectionFragment = {
    sectionId: string | null;
    incoming: string[];
    outgoing: string[];
    points: Point[];
    depth: number;
  };

  const distance = (a: Point, b: Point): number => Math.hypot(a.x - b.x, a.y - b.y);

  const pathLength = (points: Point[]): number => {
    let total = 0;
    for (let i = 1; i < points.length; i++) {
      total += distance(points[i - 1], points[i]);
    }
    return total;
  };

  const orientationScore = (points: Point[], source: Point | null, target: Point | null): number => {
    if (points.length < 2) return Number.POSITIVE_INFINITY;
    if (!source || !target) return 0;
    return distance(points[0], source) + distance(points[points.length - 1], target);
  };

  const orientForAnchors = (
    points: Point[],
    source: Point | null,
    target: Point | null,
  ): { points: Point[]; score: number } => {
    if (points.length < 2) return { points, score: Number.POSITIVE_INFINITY };
    const forwardScore = orientationScore(points, source, target);
    const reversed = [...points].reverse();
    const reverseScore = orientationScore(reversed, source, target);
    if (reverseScore < forwardScore) return { points: reversed, score: reverseScore };
    return { points, score: forwardScore };
  };

  const appendSection = (polyline: Point[], sectionPoints: Point[]): Point[] => {
    if (sectionPoints.length === 0) return polyline;
    if (polyline.length === 0) return [...sectionPoints];
    const last = polyline[polyline.length - 1];
    const startDist = distance(last, sectionPoints[0]);
    const endDist = distance(last, sectionPoints[sectionPoints.length - 1]);
    const oriented = endDist < startDist ? [...sectionPoints].reverse() : sectionPoints;
    if (oriented.length === 0) return polyline;
    const shouldSkipFirst = distance(last, oriented[0]) < 0.0001;
    return shouldSkipFirst ? [...polyline, ...oriented.slice(1)] : [...polyline, ...oriented];
  };

  const pointsKey = (points: Point[]): string =>
    points.map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`).join("|");

  const entityNameMap = new Map(entityDefs.map((e) => [e.id, e.name]));
  const geoEdges: GeometryEdge[] = validEdges.map((def) => {
    const sourcePos = absoluteNodePos.get(def.source);
    const targetPos = absoluteNodePos.get(def.target);
    const sourceSize = nodeSizes.get(def.source) ?? { width: 150, height: 36 };
    const targetSize = nodeSizes.get(def.target) ?? { width: 150, height: 36 };
    const sourceAnchor = sourcePos
      ? { x: sourcePos.x + sourceSize.width / 2, y: sourcePos.y + sourceSize.height / 2 }
      : null;
    const targetAnchor = targetPos
      ? { x: targetPos.x + targetSize.width / 2, y: targetPos.y + targetSize.height / 2 }
      : null;

    const records = edgeLayoutsById.get(def.id) ?? [];
    const exactRecords = records.filter(
      (record) =>
        record.sources.includes(def.source) && record.targets.includes(def.target),
    );
    const edgeRecords = exactRecords.length > 0 ? exactRecords : records;

    const fragmentsRaw: SectionFragment[] = [];
    for (const record of edgeRecords) {
      const orderedSections = orderSections(record.sections);
      for (const section of orderedSections) {
        const sectionPoints: Point[] = [];
        if (section.startPoint) {
          sectionPoints.push({
            x: section.startPoint.x + record.graphOffset.x,
            y: section.startPoint.y + record.graphOffset.y,
          });
        }
        if (section.bendPoints) {
          sectionPoints.push(
            ...section.bendPoints.map((p) => ({
              x: p.x + record.graphOffset.x,
              y: p.y + record.graphOffset.y,
            })),
          );
        }
        if (section.endPoint) {
          sectionPoints.push({
            x: section.endPoint.x + record.graphOffset.x,
            y: section.endPoint.y + record.graphOffset.y,
          });
        }
        if (sectionPoints.length < 2) continue;
        fragmentsRaw.push({
          sectionId: section.id ?? null,
          incoming: [...(section.incomingSections ?? [])],
          outgoing: [...(section.outgoingSections ?? [])],
          points: sectionPoints,
          depth: record.depth,
        });
      }
    }

    const fragmentsById = new Map<string, SectionFragment>();
    const fragmentsByShape = new Map<string, SectionFragment>();

    for (const fragment of fragmentsRaw) {
      if (fragment.sectionId) {
        const existing = fragmentsById.get(fragment.sectionId);
        if (!existing) {
          fragmentsById.set(fragment.sectionId, fragment);
        } else {
          const existingScore = orientForAnchors(existing.points, sourceAnchor, targetAnchor).score;
          const nextScore = orientForAnchors(fragment.points, sourceAnchor, targetAnchor).score;
          if (nextScore < existingScore || (nextScore === existingScore && fragment.depth < existing.depth)) {
            fragmentsById.set(fragment.sectionId, fragment);
          }
        }
        continue;
      }
      const key = pointsKey(fragment.points);
      if (!fragmentsByShape.has(key)) fragmentsByShape.set(key, fragment);
    }

    const fragments = [...fragmentsById.values(), ...fragmentsByShape.values()];

    const linked = new Map<string, SectionFragment>();
    for (const fragment of fragments) {
      if (fragment.sectionId) linked.set(fragment.sectionId, fragment);
    }

    const polylineCandidates: Point[][] = [];

    if (linked.size > 0) {
      const roots = Array.from(linked.values()).filter((fragment) => {
        const linkedIncoming = fragment.incoming.filter((id) => linked.has(id));
        return linkedIncoming.length === 0;
      });
      const startIds = (roots.length > 0 ? roots : Array.from(linked.values()))
        .map((fragment) => fragment.sectionId!)
        .sort();

      const pushChain = (ids: string[]) => {
        let polyline: Point[] = [];
        for (const id of ids) {
          const fragment = linked.get(id);
          if (!fragment) continue;
          polyline = appendSection(polyline, fragment.points);
        }
        if (polyline.length >= 2) polylineCandidates.push(polyline);
      };

      const walk = (currentId: string, path: string[], visiting: Set<string>) => {
        if (visiting.has(currentId)) {
          pushChain(path);
          return;
        }
        const current = linked.get(currentId);
        if (!current) {
          pushChain(path);
          return;
        }

        const nextIds = current.outgoing.filter((id) => linked.has(id));
        if (nextIds.length === 0) {
          pushChain(path);
          return;
        }

        const nextVisiting = new Set(visiting);
        nextVisiting.add(currentId);
        for (const nextId of nextIds.sort()) {
          walk(nextId, [...path, nextId], nextVisiting);
        }
      };

      for (const startId of startIds) {
        walk(startId, [startId], new Set());
      }
    }

    // Fragments without section linkage still provide fallback candidates.
    for (const fragment of fragments) {
      if (fragment.sectionId && linked.has(fragment.sectionId)) continue;
      if (fragment.points.length >= 2) polylineCandidates.push(fragment.points);
    }

    let points: Point[] = [];
    if (polylineCandidates.length > 0) {
      const ranked = polylineCandidates
        .map((candidate) => {
          const oriented = orientForAnchors(candidate, sourceAnchor, targetAnchor);
          return {
            points: oriented.points,
            score: oriented.score,
            length: pathLength(oriented.points),
          };
        })
        .sort((a, b) => {
          if (a.score !== b.score) return a.score - b.score;
          return b.length - a.length;
        });
      points = ranked[0].points;
    } else if (sourceAnchor && targetAnchor) {
      points = [sourceAnchor, targetAnchor];
    }

    const srcName = entityNameMap.get(def.source) ?? def.source;
    const dstName = entityNameMap.get(def.target) ?? def.target;
    const markerSize = edgeMarkerSize(def);

    const edge: GeometryEdge = {
      id: def.id,
      sourceId: def.source,
      targetId: def.target,
      polyline: points,
      kind: def.kind,
      data: {
        style: edgeStyle(def),
        tooltip: edgeTooltip(def, srcName, dstName),
        edgeLabel: edgeLabel(def),
        edgePending: def.state === "pending",
        markerSize,
      },
    };
    return edge;
  });

  // Compute bounding box of all geometry
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;

  for (const node of geoNodes) {
    minX = Math.min(minX, node.worldRect.x);
    minY = Math.min(minY, node.worldRect.y);
    maxX = Math.max(maxX, node.worldRect.x + node.worldRect.width);
    maxY = Math.max(maxY, node.worldRect.y + node.worldRect.height);
  }
  for (const group of geoGroups) {
    minX = Math.min(minX, group.worldRect.x);
    minY = Math.min(minY, group.worldRect.y);
    maxX = Math.max(maxX, group.worldRect.x + group.worldRect.width);
    maxY = Math.max(maxY, group.worldRect.y + group.worldRect.height);
  }

  const bounds =
    minX === Infinity
      ? { x: 0, y: 0, width: 0, height: 0 }
      : { x: minX, y: minY, width: maxX - minX, height: maxY - minY };

  return {
    nodes: geoNodes,
    groups: geoGroups,
    edges: geoEdges,
    bounds,
  };
}
