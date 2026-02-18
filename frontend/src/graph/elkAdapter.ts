import ELK from "elkjs/lib/elk-api.js";
import elkWorkerUrl from "elkjs/lib/elk-worker.min.js?url";
import type { EntityDef, EdgeDef } from "../snapshot";
import type { GraphGeometry, GeometryNode, GeometryGroup, GeometryEdge, Point } from "./geometry";

// ── ELK layout ────────────────────────────────────────────────

const elk = new ELK({ workerUrl: elkWorkerUrl });

const elkOptions = {
  "elk.algorithm": "layered",
  "elk.direction": "DOWN",
  "elk.spacing.nodeNode": "36",
  "elk.spacing.edgeNode": "20",
  "elk.layered.spacing.nodeNodeBetweenLayers": "56",
  "elk.padding": "[top=24,left=24,bottom=24,right=24]",
  "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
};

const subgraphPaddingBase = {
  top: 30,
  left: 12,
  bottom: 12,
  right: 12,
};

// ── Edge styling ──────────────────────────────────────────────

export type EdgeStyle = {
  stroke?: string;
  strokeWidth?: number;
  strokeDasharray?: string;
};

export function edgeStyle(edge: EdgeDef): EdgeStyle {
  const stroke = "var(--edge-stroke-default)";
  const kind = edge.kind;
  switch (kind) {
    case "touches":
      return { stroke, strokeWidth: 1.1, strokeDasharray: "4 4" };
    case "needs":
      return { stroke, strokeWidth: 1.4 };
    case "holds":
      return { stroke, strokeWidth: 1.2 };
    case "polls":
      return { stroke, strokeWidth: 1.1, strokeDasharray: "2 3" };
    case "closed_by":
      return { stroke, strokeWidth: 1.2 };
    case "channel_link":
      return { stroke, strokeWidth: 1.0, strokeDasharray: "6 3" };
    case "rpc_link":
      return { stroke, strokeWidth: 1.0, strokeDasharray: "6 3" };
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

// ── Types ─────────────────────────────────────────────────────

export type SubgraphScopeMode = "none" | "process" | "crate";

export type LayoutGraphOptions = {
  subgraphHeaderHeight?: number;
};

// ── Layout ────────────────────────────────────────────────────

export async function layoutGraph(
  entityDefs: EntityDef[],
  edgeDefs: EdgeDef[],
  nodeSizes: Map<string, { width: number; height: number }>,
  subgraphScopeMode: SubgraphScopeMode = "none",
  options: LayoutGraphOptions = {},
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
  const measuredHeaderHeight = Math.max(0, Math.ceil(options.subgraphHeaderHeight ?? 0));
  const subgraphElkPadding = `[top=${measuredHeaderHeight + subgraphPaddingBase.left},left=${subgraphPaddingBase.left},bottom=${subgraphPaddingBase.bottom},right=${subgraphPaddingBase.right}]`;

  const defaultInPortId = (entityId: string) => `${entityId}:in`;
  const defaultOutPortId = (entityId: string) => `${entityId}:out`;
  const edgeSourceRef = (edge: EdgeDef) => edge.sourcePort ?? defaultOutPortId(edge.source);
  const edgeTargetRef = (edge: EdgeDef) => edge.targetPort ?? defaultInPortId(edge.target);

  const portsForEntity = (entity: EntityDef): Array<{ id: string; layoutOptions: Record<string, string> }> => {
    if (entity.channelPair) {
      const mergedId = entity.id;
      return [
        { id: `${mergedId}:tx`, layoutOptions: { "elk.port.side": "SOUTH" } },
        { id: `${mergedId}:rx`, layoutOptions: { "elk.port.side": "NORTH" } },
      ];
    }
    if (entity.rpcPair) {
      const mergedId = entity.id;
      return [
        { id: `${mergedId}:req`, layoutOptions: { "elk.port.side": "SOUTH" } },
        { id: `${mergedId}:resp`, layoutOptions: { "elk.port.side": "NORTH" } },
      ];
    }
    return [
      { id: defaultInPortId(entity.id), layoutOptions: { "elk.port.side": "NORTH" } },
      { id: defaultOutPortId(entity.id), layoutOptions: { "elk.port.side": "SOUTH" } },
    ];
  };

  const nodeLayoutOptions = {
    "elk.portConstraints": "FIXED_SIDE",
  };

  const elkNodeForEntity = (entity: EntityDef) => {
    const size = nodeSizes.get(entity.id);
    return {
      id: entity.id,
      width: size?.width || 150,
      height: size?.height || 36,
      layoutOptions: nodeLayoutOptions,
      ports: portsForEntity(entity),
    };
  };

  const groupedChildren = (() => {
    if (!hasSubgraphs) {
      return entityDefs.map(elkNodeForEntity);
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
        children: members.map(elkNodeForEntity),
      }));
  })();

  const result = await elk.layout({
    id: "root",
    layoutOptions: {
      ...elkOptions,
      ...(hasSubgraphs ? { "elk.hierarchyHandling": "INCLUDE_CHILDREN" } : {}),
      "org.eclipse.elk.json.shapeCoords": "ROOT",
      "org.eclipse.elk.json.edgeCoords": "ROOT",
    },
    children: groupedChildren,
    edges: validEdges.map((e) => ({
      id: e.id,
      sources: [edgeSourceRef(e)],
      targets: [edgeTargetRef(e)],
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
    sources: string[];
    targets: string[];
    sections: ElkSectionLike[];
  };
  const edgeLayoutsById = new Map<string, CollectedElkEdge[]>();

  const collectEdgeLayouts = (graph: any) => {
    for (const edge of graph.edges ?? []) {
      const collected: CollectedElkEdge = {
        sources: (edge.sources ?? []).map(String),
        targets: (edge.targets ?? []).map(String),
        sections: (edge.sections ?? []) as ElkSectionLike[],
      };
      if (!edgeLayoutsById.has(edge.id)) edgeLayoutsById.set(edge.id, []);
      edgeLayoutsById.get(edge.id)!.push(collected);
    }

    for (const child of graph.children ?? []) {
      collectEdgeLayouts(child);
    }
  };

  collectEdgeLayouts(result);

  const geoNodes: GeometryNode[] = [];
  const geoGroups: GeometryGroup[] = [];

  // Track absolute positions for each entity node (needed for group member listing)
  const absoluteNodePos = new Map<string, { x: number; y: number }>();

  const makeNodeData = (def: EntityDef): { kind: string; data: any } => {
    if (def.channelPair) {
      return {
        kind: "channelPairNode",
        data: {
          nodeId: def.id,
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
          nodeId: def.id,
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

  const walkChildren = (children: any[] | undefined) => {
    for (const child of children ?? []) {
      const isGroupNode = typeof child.id === "string" && child.id.startsWith("scope-group:");
      const absX = child.x ?? 0;
      const absY = child.y ?? 0;

      if (isGroupNode) {
        const groupId = String(child.id);
        const firstColon = groupId.indexOf(":");
        const secondColon = groupId.indexOf(":", firstColon + 1);
        const scopeKind = secondColon > -1 ? groupId.slice(firstColon + 1, secondColon) : "scope";
        const rawScopeKey = secondColon > -1 ? groupId.slice(secondColon + 1) : groupId;
        const memberIds = (child.children ?? [])
          .filter((c: any) => !String(c.id).startsWith("scope-group:"))
          .map((c: any) => String(c.id));
        const memberEntities = memberIds
          .map((id: string) => entityById.get(id))
          .filter((entity: EntityDef | undefined): entity is EntityDef => !!entity);

        let canonicalLabel = rawScopeKey === "~no-crate" ? "(no crate)" : rawScopeKey;
        if (scopeKind === "process" && memberEntities.length > 0) {
          const anchor = memberEntities[0];
          const pidSuffix = anchor.processPid != null ? String(anchor.processPid) : anchor.processId;
          canonicalLabel = `${anchor.processName}(${pidSuffix})`;
        } else if (scopeKind === "connection" && memberEntities.length > 0) {
          const named = memberEntities.find((entity: EntityDef) => entity.kind === "connection") ?? memberEntities[0];
          canonicalLabel = named.name;
        }

        geoGroups.push({
          id: groupId,
          scopeKind,
          label: canonicalLabel,
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

        walkChildren(child.children);
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

  walkChildren(result.children);

  type SectionFragment = {
    sectionId: string | null;
    incoming: string[];
    outgoing: string[];
    points: Point[];
  };

  const distance = (a: Point, b: Point): number => Math.hypot(a.x - b.x, a.y - b.y);
  const isNear = (a: Point, b: Point): boolean => distance(a, b) < 1;

  const appendSectionStrict = (
    edgeId: string,
    polyline: Point[],
    sectionPoints: Point[],
  ): Point[] => {
    if (sectionPoints.length === 0) return polyline;
    if (polyline.length === 0) return [...sectionPoints];
    const last = polyline[polyline.length - 1];
    const start = sectionPoints[0];
    const end = sectionPoints[sectionPoints.length - 1];
    let oriented = sectionPoints;
    if (isNear(last, start)) {
      oriented = sectionPoints;
    } else if (isNear(last, end)) {
      oriented = [...sectionPoints].reverse();
    } else {
      throw new Error(
        `[elk] edge ${edgeId}: non-contiguous section chain (${last.x},${last.y}) -> (${start.x},${start.y})`,
      );
    }
    if (oriented.length === 0) return polyline;
    const shouldSkipFirst = isNear(last, oriented[0]);
    return shouldSkipFirst ? [...polyline, ...oriented.slice(1)] : [...polyline, ...oriented];
  };

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

    const records = edgeLayoutsById.get(def.id);
    if (!records || records.length === 0) {
      throw new Error(`[elk] edge ${def.id}: no routed sections returned by ELK`);
    }
    const expectedSourceRef = edgeSourceRef(def);
    const expectedTargetRef = edgeTargetRef(def);
    const edgeRecords = records.filter((record) => {
      const sourceMatch =
        record.sources.includes(expectedSourceRef) || record.sources.includes(def.source);
      const targetMatch =
        record.targets.includes(expectedTargetRef) || record.targets.includes(def.target);
      return sourceMatch && targetMatch;
    });
    if (edgeRecords.length === 0) {
      throw new Error(`[elk] edge ${def.id}: no matching ELK records for ${def.source} -> ${def.target}`);
    }

    const fragmentsRaw: SectionFragment[] = [];
    for (const record of edgeRecords) {
      for (const section of record.sections) {
        const sectionPoints: Point[] = [];
        if (section.startPoint) {
          sectionPoints.push({
            x: section.startPoint.x,
            y: section.startPoint.y,
          });
        }
        if (section.bendPoints) {
          sectionPoints.push(
            ...section.bendPoints.map((p: Point) => ({
              x: p.x,
              y: p.y,
            })),
          );
        }
        if (section.endPoint) {
          sectionPoints.push({
            x: section.endPoint.x,
            y: section.endPoint.y,
          });
        }
        if (sectionPoints.length < 2) continue;
        fragmentsRaw.push({
          sectionId: section.id ?? null,
          incoming: [...(section.incomingSections ?? [])],
          outgoing: [...(section.outgoingSections ?? [])],
          points: sectionPoints,
        });
      }
    }

    const byId = new Map<string, SectionFragment>();
    const unnamed: SectionFragment[] = [];
    for (const fragment of fragmentsRaw) {
      if (!fragment.sectionId) {
        unnamed.push(fragment);
        continue;
      }
      const existing = byId.get(fragment.sectionId);
      if (!existing) {
        byId.set(fragment.sectionId, fragment);
        continue;
      }
      const samePoints =
        existing.points.length === fragment.points.length &&
        existing.points.every((p, i) => isNear(p, fragment.points[i]));
      const sameIncoming =
        [...existing.incoming].sort().join("|") === [...fragment.incoming].sort().join("|");
      const sameOutgoing =
        [...existing.outgoing].sort().join("|") === [...fragment.outgoing].sort().join("|");
      if (!samePoints || !sameIncoming || !sameOutgoing) {
        throw new Error(`[elk] edge ${def.id}: conflicting records for section ${fragment.sectionId}`);
      }
    }

    let points: Point[] = [];
    if (byId.size === 0) {
      if (unnamed.length !== 1) {
        throw new Error(`[elk] edge ${def.id}: expected exactly one unnamed section, got ${unnamed.length}`);
      }
      points = [...unnamed[0].points];
    } else {
      if (unnamed.length > 0) {
        throw new Error(`[elk] edge ${def.id}: mixed named and unnamed sections`);
      }
      const incomingCount = new Map<string, number>();
      for (const id of byId.keys()) incomingCount.set(id, 0);
      for (const fragment of byId.values()) {
        for (const outId of fragment.outgoing) {
          if (!byId.has(outId)) continue;
          incomingCount.set(outId, (incomingCount.get(outId) ?? 0) + 1);
        }
      }

      const roots = Array.from(byId.keys()).filter((id) => (incomingCount.get(id) ?? 0) === 0);
      if (roots.length !== 1) {
        throw new Error(`[elk] edge ${def.id}: expected one root section, got ${roots.length}`);
      }

      const orderedIds: string[] = [];
      const visited = new Set<string>();
      let currentId: string | null = roots[0];
      while (currentId) {
        if (visited.has(currentId)) {
          throw new Error(`[elk] edge ${def.id}: cycle in section chain at ${currentId}`);
        }
        visited.add(currentId);
        orderedIds.push(currentId);
        const current = byId.get(currentId);
        if (!current) break;
        const next = current.outgoing.filter((id) => byId.has(id)).sort();
        if (next.length > 1) {
          throw new Error(`[elk] edge ${def.id}: branching section chain at ${currentId}`);
        }
        currentId = next.length === 1 ? next[0] : null;
      }

      if (visited.size !== byId.size) {
        throw new Error(`[elk] edge ${def.id}: disconnected section graph (${visited.size}/${byId.size})`);
      }

      for (const id of orderedIds) {
        const section = byId.get(id);
        if (!section) continue;
        points = appendSectionStrict(def.id, points, section.points);
      }
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
        sourcePortRef: expectedSourceRef,
        targetPortRef: expectedTargetRef,
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
