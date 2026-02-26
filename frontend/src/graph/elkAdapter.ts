import ELK from "elkjs/lib/elk-api.js";
import elkWorkerUrl from "elkjs/lib/elk-worker.min.js?url";
import type { EntityDef, EdgeDef } from "../snapshot";
import {
  graphNodeDataFromEntity,
  graphNodeDataFromEdgeEvent,
} from "../components/graph/graphNodeData";
import { formatProcessLabel } from "../processLabel";
import type { GraphGeometry, GeometryNode, GeometryGroup, GeometryEdge, Point } from "./geometry";

// ── ELK layout ────────────────────────────────────────────────

const nodeSpacingBetweenLayers = 6;
const edgeNodeSpacing = 6;
const edgeSegmentSpacing = 12;

const builtinElkLayoutAlgorithms = ["box", "fixed", "random"] as const;
const registeredElkLayoutAlgorithms = [
  "layered",
  "stress",
  "mrtree",
  "radial",
  "force",
  "disco",
  "sporeOverlap",
  "sporeCompaction",
  "rectpacking",
  "vertiflex",
] as const;
const elk = new ELK({
  workerUrl: elkWorkerUrl,
  algorithms: [...registeredElkLayoutAlgorithms],
});
const knownElkLayoutAlgorithmIds = new Set<string>([
  ...builtinElkLayoutAlgorithms,
  ...registeredElkLayoutAlgorithms,
]);

const elkLayoutAlgorithmLabelById = new Map<string, string>([
  ["box", "ELK Box"],
  ["disco", "ELK DisCo"],
  ["fixed", "ELK Fixed"],
  ["force", "ELK Force"],
  ["layered", "ELK Layered"],
  ["mrtree", "ELK Mr. Tree"],
  ["radial", "ELK Radial"],
  ["random", "ELK Randomizer"],
  ["rectpacking", "ELK Rectangle Packing"],
  ["sporeCompaction", "ELK SPOrE Compaction"],
  ["sporeOverlap", "ELK SPOrE Overlap Removal"],
  ["stress", "ELK Stress"],
  ["vertiflex", "ELK VertiFlex"],
]);

export type ElkLayoutAlgorithm = string;
export type ElkLayoutAlgorithmOption = {
  id: ElkLayoutAlgorithm;
  label: string;
};

export const defaultElkLayoutAlgorithm: ElkLayoutAlgorithm = "layered";
export const fallbackElkLayoutAlgorithmOptions: ElkLayoutAlgorithmOption[] = [
  ...registeredElkLayoutAlgorithms,
  ...builtinElkLayoutAlgorithms,
].map((id) => ({
  id,
  label: elkLayoutAlgorithmLabelById.get(id) ?? id,
}));

let cachedKnownElkLayoutAlgorithmsPromise: Promise<ElkLayoutAlgorithmOption[]> | null = null;

function layoutAlgorithmIdFromMeta(raw: unknown): string | null {
  if (!raw || typeof raw !== "object") return null;
  const candidate = (raw as { id?: unknown }).id;
  if (typeof candidate !== "string" || candidate.length === 0) return null;
  const suffix = candidate.split(".").at(-1);
  return suffix && suffix.length > 0 ? suffix : candidate;
}

function layoutAlgorithmLabelFromMeta(raw: unknown, fallbackId: string): string {
  if (raw && typeof raw === "object") {
    const name = (raw as { name?: unknown }).name;
    if (typeof name === "string" && name.length > 0) return name;
  }
  return elkLayoutAlgorithmLabelById.get(fallbackId) ?? fallbackId;
}

export function knownElkLayoutAlgorithms(): Promise<ElkLayoutAlgorithmOption[]> {
  if (!cachedKnownElkLayoutAlgorithmsPromise) {
    cachedKnownElkLayoutAlgorithmsPromise = elk
      .knownLayoutAlgorithms()
      .then((known) => {
        if (!Array.isArray(known)) {
          throw new Error("[elk] knownLayoutAlgorithms() returned a non-array result");
        }
        const options: ElkLayoutAlgorithmOption[] = [];
        const seen = new Set<string>();
        for (const item of known) {
          const id = layoutAlgorithmIdFromMeta(item);
          if (!id || seen.has(id)) continue;
          seen.add(id);
          options.push({
            id,
            label: layoutAlgorithmLabelFromMeta(item, id),
          });
          knownElkLayoutAlgorithmIds.add(id);
        }
        if (options.length === 0) {
          throw new Error("[elk] knownLayoutAlgorithms() returned no algorithms");
        }
        return options.sort((a, b) => a.label.localeCompare(b.label));
      })
      .catch((error) => {
        console.error("[elk] failed to load known layout algorithms, using fallback list", error);
        return fallbackElkLayoutAlgorithmOptions;
      });
  }
  return cachedKnownElkLayoutAlgorithmsPromise;
}

function parseElkLayoutAlgorithm(raw: string | undefined): ElkLayoutAlgorithm {
  if (raw == null || raw.length === 0) return defaultElkLayoutAlgorithm;
  if (!knownElkLayoutAlgorithmIds.has(raw)) {
    const knownList = Array.from(knownElkLayoutAlgorithmIds).sort().join(", ");
    throw new Error(`[elk] unsupported layout algorithm '${raw}'. Known algorithms: ${knownList}`);
  }
  return raw;
}

function rootElkOptionsFor(algorithm: ElkLayoutAlgorithm): Record<string, string> {
  const common = {
    "elk.algorithm": algorithm,
    "elk.direction": "DOWN",
    "elk.spacing.nodeNode": "28",
    "elk.spacing.edgeNode": String(edgeNodeSpacing),
    "elk.spacing.edgeEdge": String(edgeSegmentSpacing),
    "elk.padding": "[top=24,left=24,bottom=24,right=24]",
  };
  if (algorithm === "layered") {
    return {
      ...common,
      "elk.edgeRouting": "ORTHOGONAL",
      "elk.layered.spacing.nodeNodeBetweenLayers": String(nodeSpacingBetweenLayers),
      "elk.layered.spacing.edgeNodeBetweenLayers": String(edgeNodeSpacing),
      "elk.layered.spacing.edgeEdgeBetweenLayers": String(edgeSegmentSpacing),
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
    };
  }
  return common;
}

function scopeGroupElkOptionsFor(algorithm: ElkLayoutAlgorithm): Record<string, string> {
  if (algorithm === "layered") {
    return {
      "elk.algorithm": "layered",
      "elk.direction": "DOWN",
      "elk.edgeRouting": "ORTHOGONAL",
      "elk.layered.nodePlacement.strategy": "NETWORK_SIMPLEX",
      "elk.layered.spacing.edgeEdgeBetweenLayers": String(edgeSegmentSpacing),
    };
  }
  return {
    "elk.algorithm": algorithm,
    "elk.direction": "DOWN",
  };
}

const subgraphPaddingBase = {
  top: 0,
  left: 25,
  bottom: 25,
  right: 25,
};

// Extra vertical keep-out above grouped content so edge routing is less likely
// to cross the visual scope header strip.
const subgraphHeaderKeepoutTop = 10;
const scopeGroupLabelMinWidth = 72;
const scopeGroupLabelPadX = 10;
const scopeGroupLabelIconWidth = 12;
const scopeGroupLabelIconGap = 6;
const scopeGroupLabelCharWidth = 8.5;
const edgeEventNodePrefix = "edge-event:";
const edgeEventNodeMinWidth = 72;
const edgeEventNodePadX = 8;
const edgeEventNodeCharWidth = 7.4;
const edgeEventNodeHeight = 24;
const edgeEventCenterPortSuffix = ":center";
const distance = (a: Point, b: Point): number => Math.hypot(a.x - b.x, a.y - b.y);
const isNear = (a: Point, b: Point): boolean => distance(a, b) < 1;

// ── Edge styling ──────────────────────────────────────────────

export type EdgeStyle = {
  stroke?: string;
  strokeWidth?: number;
  strokeDasharray?: string;
};

export function edgeStyle(edge: EdgeDef): EdgeStyle {
  const stroke = "var(--edge-stroke-default)";
  const kind = edge.kind;

  let strokeWidth = 2;
  switch (kind) {
    case "polls":
      return { stroke, strokeWidth, strokeDasharray: "2 3" };
    case "waiting_on":
      return { stroke, strokeWidth };
    case "held_by":
      return { stroke, strokeWidth, strokeDasharray: "4 4" };
    case "paired_with":
      return { stroke, strokeWidth, strokeDasharray: "6 3" };
  }
}

export function edgeTooltip(edge: EdgeDef, sourceName: string, targetName: string): string {
  const kind = edge.kind;
  switch (kind) {
    case "polls":
      return `${sourceName} polls ${targetName} (non-blocking)`;
    case "waiting_on":
      return `${sourceName} is blocked waiting for ${targetName}`;
    case "held_by":
      return `${sourceName} currently grants permits to ${targetName}`;
    case "paired_with":
      return `Paired: ${sourceName} ↔ ${targetName}`;
  }
}

export function edgeEventNodeLabel(kind: EdgeDef["kind"]): string | null {
  switch (kind) {
    case "waiting_on":
      return "waits on";
    case "polls":
      return "poll";
    case "held_by":
      return "held by";
    case "paired_with":
      return null;
  }
}

export function edgeEventNodeId(edgeId: string): string {
  return `${edgeEventNodePrefix}${edgeId}`;
}

function edgeEventCenterPortId(nodeId: string): string {
  return `${nodeId}${edgeEventCenterPortSuffix}`;
}

function edgeEventAnchorRef(nodeId: string, algorithm: ElkLayoutAlgorithm): string {
  return algorithm === "layered" ? edgeEventCenterPortId(nodeId) : nodeId;
}

export function edgeMarkerSize(_edge: EdgeDef): number {
  return 10;
}

// ── Types ─────────────────────────────────────────────────────

export type SubgraphScopeMode = "none" | "process" | "crate" | "task" | "cycle";

export type LayoutGraphOptions = {
  subgraphHeaderHeight?: number;
  layoutAlgorithm?: ElkLayoutAlgorithm;
};

type EdgeEventNodeMeta = {
  id: string;
  label: string;
  width: number;
  height: number;
  sourceEdge: EdgeDef;
  groupKey: string | null;
};

type RoutedEdgeDef = {
  id: string;
  sourceId: string;
  targetId: string;
  sourceRef: string;
  targetRef: string;
  sourceEdge: EdgeDef;
  markerSize: number;
};

function canonicalScopeGroupLabel(
  scopeKind: string,
  scopeKey: string,
  members: EntityDef[],
): string {
  if (scopeKind === "process" && members.length > 0) {
    const anchor = members[0];
    return formatProcessLabel(anchor.processName, anchor.processPid);
  }
  if (scopeKind === "task" && members.length > 0) {
    const named = members.find((entity: EntityDef) => !!entity.taskScopeName);
    return named?.taskScopeName ?? "(no task scope)";
  }
  if (scopeKind === "connection" && members.length > 0) {
    const named = members.find((entity: EntityDef) => entity.kind === "connection") ?? members[0];
    return named.name;
  }
  if (scopeKind === "cycle") return `Deadlock #${scopeKey}`;
  return scopeKey === "~no-crate" ? "(no crate)" : scopeKey;
}

function estimateScopeGroupLabelWidth(label: string): number {
  return Math.max(
    scopeGroupLabelMinWidth,
    Math.ceil(
      scopeGroupLabelPadX * 2 +
        scopeGroupLabelIconWidth +
        scopeGroupLabelIconGap +
        label.length * scopeGroupLabelCharWidth,
    ),
  );
}

function estimateEdgeEventNodeWidth(label: string): number {
  return Math.max(
    edgeEventNodeMinWidth,
    Math.ceil(edgeEventNodePadX * 2 + Math.max(0, label.length) * edgeEventNodeCharWidth),
  );
}

function resolveGroupLabelRect(
  groupId: string,
  groupRect: { x: number; y: number; width: number; height: number },
  labelRectRaw: { x: number; y: number; width: number; height: number },
): { x: number; y: number; width: number; height: number } {
  const { x: groupX, y: groupY, width: groupWidth, height: groupHeight } = groupRect;
  const { x: rawX, y: rawY, width: labelWidth, height: labelHeight } = labelRectRaw;

  const absoluteCandidate = { x: rawX, y: rawY, width: labelWidth, height: labelHeight };
  const relativeCandidate = {
    x: groupX + rawX,
    y: groupY + rawY,
    width: labelWidth,
    height: labelHeight,
  };

  const fitsRelativeBounds =
    rawX >= -labelWidth * 1.5 &&
    rawX <= groupWidth + labelWidth * 0.5 &&
    rawY >= -labelHeight * 3 &&
    rawY <= groupHeight + labelHeight;

  const fitsAbsoluteBounds =
    rawX >= groupX - labelWidth * 1.5 &&
    rawX <= groupX + groupWidth + labelWidth * 0.5 &&
    rawY >= groupY - labelHeight * 3 &&
    rawY <= groupY + groupHeight + labelHeight;

  if (fitsRelativeBounds && !fitsAbsoluteBounds) return relativeCandidate;
  if (fitsAbsoluteBounds && !fitsRelativeBounds) return absoluteCandidate;

  const expectedLeft = groupX + 10;
  const expectedBottom = groupY;
  const score = (rect: { x: number; y: number; width: number; height: number }) =>
    Math.abs(rect.x - expectedLeft) + Math.abs(rect.y + rect.height - expectedBottom);

  const absScore = score(absoluteCandidate);
  const relScore = score(relativeCandidate);
  if (!Number.isFinite(absScore) || !Number.isFinite(relScore)) {
    throw new Error(
      `[elk] group ${groupId}: non-finite label mode score abs=${absScore} rel=${relScore}`,
    );
  }
  return relScore < absScore ? relativeCandidate : absoluteCandidate;
}

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
    if (subgraphScopeMode === "crate") return entity.topFrame?.crate_name ?? "~no-crate";
    if (subgraphScopeMode === "task") return entity.taskScopeKey ?? `${entity.processId}:~no-task`;
    if (subgraphScopeMode === "cycle")
      return entity.sccIndex != null ? String(entity.sccIndex) : null;
    return null;
  };

  const hasSubgraphs = subgraphScopeMode !== "none";
  const elkAlgorithm = parseElkLayoutAlgorithm(
    options.layoutAlgorithm ??
      (typeof window === "undefined"
        ? undefined
        : (new URLSearchParams(window.location.search).get("elkLayout") ?? undefined)),
  );
  const elkOptions = rootElkOptionsFor(elkAlgorithm);
  const measuredHeaderHeight = Math.max(0, Math.ceil(options.subgraphHeaderHeight ?? 0));
  const elkScopeLabelHeight = Math.max(measuredHeaderHeight, 16);
  const subgraphContentInsetTop =
    measuredHeaderHeight + subgraphPaddingBase.top + subgraphHeaderKeepoutTop;
  const subgraphContentInsetBottom = subgraphPaddingBase.bottom;
  const subgraphElkPadding = `[top=${subgraphContentInsetTop},left=${subgraphPaddingBase.left},bottom=${subgraphPaddingBase.bottom},right=${subgraphPaddingBase.right}]`;

  const edgeEventNodeById = new Map<string, EdgeEventNodeMeta>();
  const routedEdgeDefs: RoutedEdgeDef[] = [];
  for (const edge of validEdges) {
    const eventLabel = edgeEventNodeLabel(edge.kind);
    if (!eventLabel) {
      routedEdgeDefs.push({
        id: edge.id,
        sourceId: edge.source,
        targetId: edge.target,
        sourceRef: edge.source,
        targetRef: edge.target,
        sourceEdge: edge,
        markerSize: edgeMarkerSize(edge),
      });
      continue;
    }

    const eventNodeId = edgeEventNodeId(edge.id);
    const sourceEntity = entityById.get(edge.source);
    const targetEntity = entityById.get(edge.target);
    const sourceGroupKey = sourceEntity ? groupKeyFor(sourceEntity) : null;
    const targetGroupKey = targetEntity ? groupKeyFor(targetEntity) : null;
    const eventGroupKey =
      sourceGroupKey != null && sourceGroupKey === targetGroupKey ? sourceGroupKey : null;
    if (!edgeEventNodeById.has(eventNodeId)) {
      edgeEventNodeById.set(eventNodeId, {
        id: eventNodeId,
        label: eventLabel,
        width: estimateEdgeEventNodeWidth(eventLabel),
        height: edgeEventNodeHeight,
        sourceEdge: edge,
        groupKey: eventGroupKey,
      });
    } else {
      const existing = edgeEventNodeById.get(eventNodeId)!;
      if (existing.groupKey !== eventGroupKey) {
        throw new Error(
          `[elk] event node ${eventNodeId}: inconsistent inferred group assignment (${existing.groupKey ?? "none"} vs ${eventGroupKey ?? "none"})`,
        );
      }
    }

    routedEdgeDefs.push(
      {
        id: `${edge.id}:a`,
        sourceId: edge.source,
        targetId: eventNodeId,
        sourceRef: edge.source,
        targetRef: edgeEventAnchorRef(eventNodeId, elkAlgorithm),
        sourceEdge: edge,
        markerSize: 0,
      },
      {
        id: `${edge.id}:b`,
        sourceId: eventNodeId,
        targetId: edge.target,
        sourceRef: edgeEventAnchorRef(eventNodeId, elkAlgorithm),
        targetRef: edge.target,
        sourceEdge: edge,
        markerSize: edgeMarkerSize(edge),
      },
    );
  }

  const requireNodeSize = (entityId: string): { width: number; height: number } => {
    const size = nodeSizes.get(entityId);
    if (size && size.width > 0 && size.height > 0) return size;
    const message = size
      ? `[elk] invalid measured node size for entity ${entityId}: ${size.width}x${size.height}`
      : `[elk] missing measured node size for entity ${entityId}`;
    if (typeof window !== "undefined" && typeof window.alert === "function") {
      window.alert(message);
    }
    throw new Error(message);
  };

  const elkNodeForEntity = (entity: EntityDef) => {
    const size = requireNodeSize(entity.id);
    return {
      id: entity.id,
      width: size.width,
      height: size.height,
    };
  };

  const elkNodeForEdgeEvent = (node: EdgeEventNodeMeta) => {
    const measured = nodeSizes.get(node.id);
    const width = measured && measured.width > 0 ? measured.width : node.width;
    const height = measured && measured.height > 0 ? measured.height : node.height;
    if (elkAlgorithm !== "layered") {
      return {
        id: node.id,
        width,
        height,
      };
    }
    return {
      id: node.id,
      width,
      height,
      layoutOptions: {
        "elk.portConstraints": "FIXED_POS",
      },
      ports: [
        {
          id: edgeEventCenterPortId(node.id),
          x: width / 2,
          y: height / 2,
        },
      ],
    };
  };

  type ScopeGroupMeta = {
    id: string;
    scopeKind: string;
    scopeKey: string;
    label: string;
  };
  const scopeGroupById = new Map<string, ScopeGroupMeta>();

  const groupedChildren = (() => {
    const eventNodeMetas = Array.from(edgeEventNodeById.values()).sort((a, b) =>
      a.id.localeCompare(b.id),
    );

    if (!hasSubgraphs) {
      return [...entityDefs.map(elkNodeForEntity), ...eventNodeMetas.map(elkNodeForEdgeEvent)];
    }

    const grouped = new Map<string, EntityDef[]>();
    const ungrouped: EntityDef[] = [];
    for (const entity of entityDefs) {
      const key = groupKeyFor(entity);
      if (key == null) {
        ungrouped.push(entity);
      } else {
        if (!grouped.has(key)) grouped.set(key, []);
        grouped.get(key)!.push(entity);
      }
    }
    const groupedEventNodes = new Map<string, EdgeEventNodeMeta[]>();
    const ungroupedEventNodes: EdgeEventNodeMeta[] = [];
    for (const eventNode of eventNodeMetas) {
      if (eventNode.groupKey == null) {
        ungroupedEventNodes.push(eventNode);
        continue;
      }
      if (!groupedEventNodes.has(eventNode.groupKey)) groupedEventNodes.set(eventNode.groupKey, []);
      groupedEventNodes.get(eventNode.groupKey)!.push(eventNode);
    }

    const groupNodes = Array.from(grouped.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([scopeKey, members]) => {
        const id = `scope-group:${subgraphScopeMode}:${scopeKey}`;
        const label = canonicalScopeGroupLabel(subgraphScopeMode, scopeKey, members);
        scopeGroupById.set(id, {
          id,
          scopeKind: subgraphScopeMode,
          scopeKey,
          label,
        });
        return {
          id,
          layoutOptions: {
            ...scopeGroupElkOptionsFor(elkAlgorithm),
            "elk.padding": subgraphElkPadding,
            "elk.nodeLabels.placement": "[H_LEFT, V_TOP, OUTSIDE]",
          },
          labels: [
            {
              id: `${id}:label`,
              text: label,
              width: estimateScopeGroupLabelWidth(label),
              height: elkScopeLabelHeight,
            },
          ],
          children: [
            ...members.map(elkNodeForEntity),
            ...(groupedEventNodes.get(scopeKey) ?? []).map(elkNodeForEdgeEvent),
          ],
        };
      });

    return [
      ...groupNodes,
      ...ungrouped.map(elkNodeForEntity),
      ...ungroupedEventNodes.map(elkNodeForEdgeEvent),
    ];
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
    edges: routedEdgeDefs.map((e) => ({
      id: e.id,
      sources: [e.sourceRef],
      targets: [e.targetRef],
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
    sourcePoint?: Point;
    targetPoint?: Point;
    bendPoints?: Point[];
  };
  const edgeLayoutsById = new Map<string, CollectedElkEdge[]>();

  const collectEdgeLayouts = (graph: any) => {
    for (const edge of graph.edges ?? []) {
      const collected: CollectedElkEdge = {
        sources: (edge.sources ?? []).map(String),
        targets: (edge.targets ?? []).map(String),
        sections: (edge.sections ?? []) as ElkSectionLike[],
        sourcePoint:
          edge.sourcePoint &&
          Number.isFinite(edge.sourcePoint.x) &&
          Number.isFinite(edge.sourcePoint.y)
            ? { x: edge.sourcePoint.x, y: edge.sourcePoint.y }
            : undefined,
        targetPoint:
          edge.targetPoint &&
          Number.isFinite(edge.targetPoint.x) &&
          Number.isFinite(edge.targetPoint.y)
            ? { x: edge.targetPoint.x, y: edge.targetPoint.y }
            : undefined,
        bendPoints: Array.isArray(edge.bendPoints)
          ? edge.bendPoints
              .filter((p: any) => Number.isFinite(p?.x) && Number.isFinite(p?.y))
              .map((p: any) => ({ x: p.x, y: p.y }))
          : undefined,
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
  const portAnchorMap = new Map<string, Point>();

  const makeNodeData = (def: EntityDef): { kind: string; data: any } => ({
    kind: "graphNode",
    data: graphNodeDataFromEntity(def),
  });

  const eventNodeDataById = new Map(
    Array.from(edgeEventNodeById.values()).map((meta) => [
      meta.id,
      graphNodeDataFromEdgeEvent(meta.sourceEdge, meta.label),
    ]),
  );

  const walkChildren = (children: any[] | undefined) => {
    for (const child of children ?? []) {
      const isGroupNode = typeof child.id === "string" && child.id.startsWith("scope-group:");
      const absX = child.x ?? 0;
      const absY = child.y ?? 0;

      if (isGroupNode) {
        const groupId = String(child.id);
        const scopeMeta = scopeGroupById.get(groupId);
        if (!scopeMeta) {
          throw new Error(`[elk] group ${groupId}: missing scope metadata`);
        }
        const memberIds = (child.children ?? [])
          .filter((c: any) => !String(c.id).startsWith("scope-group:"))
          .map((c: any) => String(c.id));
        const groupLabel = child.labels?.[0];
        if (!groupLabel) {
          throw new Error(`[elk] group ${groupId}: missing ELK label geometry`);
        }
        const labelX = groupLabel.x;
        const labelY = groupLabel.y;
        const labelWidth = groupLabel.width;
        const labelHeight = groupLabel.height;
        if (
          !Number.isFinite(labelX) ||
          !Number.isFinite(labelY) ||
          !Number.isFinite(labelWidth) ||
          !Number.isFinite(labelHeight)
        ) {
          throw new Error(
            `[elk] group ${groupId}: invalid ELK label rect (${labelX},${labelY},${labelWidth},${labelHeight})`,
          );
        }
        const resolvedLabelRect = resolveGroupLabelRect(
          groupId,
          {
            x: absX,
            y: absY,
            width: child.width ?? 260,
            height: child.height ?? 180,
          },
          {
            x: labelX,
            y: labelY,
            width: labelWidth,
            height: labelHeight,
          },
        );

        geoGroups.push({
          id: groupId,
          scopeKind: scopeMeta.scopeKind,
          label: scopeMeta.label,
          worldRect: {
            x: absX,
            y: absY,
            width: child.width ?? 260,
            height: child.height ?? 180,
          },
          labelRect: {
            x: resolvedLabelRect.x,
            y: resolvedLabelRect.y,
            width: resolvedLabelRect.width,
            height: resolvedLabelRect.height,
          },
          members: memberIds,
          data: {
            scopeKind: scopeMeta.scopeKind,
            scopeKey: scopeMeta.scopeKey,
            count: memberIds.length,
          },
        });

        walkChildren(child.children);
        continue;
      }

      const entity = entityById.get(child.id);
      if (!entity) {
        const eventNodeId = String(child.id);
        const eventNodeData = eventNodeDataById.get(eventNodeId);
        if (!eventNodeData) continue;
        const nodeWidth = child.width ?? edgeEventNodeMinWidth;
        const nodeHeight = child.height ?? edgeEventNodeHeight;
        geoNodes.push({
          id: eventNodeId,
          kind: "graphNode",
          worldRect: { x: absX, y: absY, width: nodeWidth, height: nodeHeight },
          data: eventNodeData,
        });
        if (elkAlgorithm === "layered") {
          const centerPortId = edgeEventCenterPortId(eventNodeId);
          const centerPort = (child.ports ?? []).find(
            (port: any) => String(port.id) === centerPortId,
          );
          if (!centerPort) {
            throw new Error(`[elk] event node ${eventNodeId}: missing center port ${centerPortId}`);
          }
          const rawX = centerPort.x;
          const rawY = centerPort.y;
          if (!Number.isFinite(rawX) || !Number.isFinite(rawY)) {
            throw new Error(
              `[elk] event node ${eventNodeId}: invalid center port coordinates (${rawX},${rawY})`,
            );
          }

          const expectedCenter = { x: absX + nodeWidth / 2, y: absY + nodeHeight / 2 };
          const absoluteCandidate = { x: rawX, y: rawY };
          const relativeCandidate = { x: absX + rawX, y: absY + rawY };
          const anchor =
            distance(absoluteCandidate, expectedCenter) <=
            distance(relativeCandidate, expectedCenter)
              ? absoluteCandidate
              : relativeCandidate;
          const centerDistance = distance(anchor, expectedCenter);
          if (centerDistance > 1.5) {
            throw new Error(
              `[elk] event node ${eventNodeId}: center port not centered (distance=${centerDistance.toFixed(
                3,
              )}, anchor=${anchor.x.toFixed(3)},${anchor.y.toFixed(3)}, expected=${expectedCenter.x.toFixed(3)},${expectedCenter.y.toFixed(3)})`,
            );
          }
          portAnchorMap.set(centerPortId, anchor);
        }
        continue;
      }

      const size = requireNodeSize(entity.id);
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

  // ELK hierarchy can stretch parent group heights based on cross-group edge routing,
  // which makes nodes look arbitrarily "too high/low" inside group boxes. Keep the
  // visual group containers tight around member node content in Y.
  if (hasSubgraphs && geoGroups.length > 0) {
    const nodeRectById = new Map(geoNodes.map((node) => [node.id, node.worldRect]));
    for (const group of geoGroups) {
      const memberRects = group.members
        .map((memberId) => nodeRectById.get(memberId))
        .filter((rect): rect is { x: number; y: number; width: number; height: number } => !!rect);
      if (memberRects.length === 0) continue;

      const minMemberY = Math.min(...memberRects.map((rect) => rect.y));
      const maxMemberY = Math.max(...memberRects.map((rect) => rect.y + rect.height));
      const tightY = minMemberY - subgraphContentInsetTop;
      const tightHeight =
        maxMemberY - minMemberY + subgraphContentInsetTop + subgraphContentInsetBottom;
      const oldY = group.worldRect.y;
      const deltaY = tightY - oldY;
      group.worldRect = {
        ...group.worldRect,
        y: tightY,
        height: tightHeight,
      };
      if (group.labelRect) {
        group.labelRect = {
          ...group.labelRect,
          y: group.labelRect.y + deltaY,
        };
      }
    }
  }

  type SectionFragment = {
    sectionId: string | null;
    incoming: string[];
    outgoing: string[];
    points: Point[];
  };

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
  const nodeCenterById = new Map(
    geoNodes.map((node) => [
      node.id,
      {
        x: node.worldRect.x + node.worldRect.width / 2,
        y: node.worldRect.y + node.worldRect.height / 2,
      },
    ]),
  );

  const polylineForRoutedEdge = (edge: RoutedEdgeDef): Point[] => {
    const records = edgeLayoutsById.get(edge.id);
    if (!records || records.length === 0) {
      throw new Error(`[elk] edge ${edge.id}: no routed sections returned by ELK`);
    }
    const edgeRecords = records.filter((record) => {
      const sourceMatch =
        record.sources.includes(edge.sourceRef) || record.sources.includes(edge.sourceId);
      const targetMatch =
        record.targets.includes(edge.targetRef) || record.targets.includes(edge.targetId);
      return sourceMatch && targetMatch;
    });
    if (edgeRecords.length === 0) {
      throw new Error(
        `[elk] edge ${edge.id}: no matching ELK records for ${edge.sourceId} -> ${edge.targetId}`,
      );
    }

    const collectFragments = (recordsToCollect: CollectedElkEdge[]): SectionFragment[] => {
      const fragments: SectionFragment[] = [];
      for (const record of recordsToCollect) {
        let sectionAdded = false;
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
          sectionAdded = true;
          fragments.push({
            sectionId: section.id ?? null,
            incoming: [...(section.incomingSections ?? [])],
            outgoing: [...(section.outgoingSections ?? [])],
            points: sectionPoints,
          });
        }

        // Some ELK algorithms expose edge points directly on the edge record
        // instead of inside `sections`.
        if (!sectionAdded) {
          const directPoints: Point[] = [];
          if (record.sourcePoint)
            directPoints.push({ x: record.sourcePoint.x, y: record.sourcePoint.y });
          if (record.bendPoints && record.bendPoints.length > 0) {
            directPoints.push(...record.bendPoints.map((p) => ({ x: p.x, y: p.y })));
          }
          if (record.targetPoint)
            directPoints.push({ x: record.targetPoint.x, y: record.targetPoint.y });
          if (directPoints.length >= 2) {
            fragments.push({
              sectionId: null,
              incoming: [],
              outgoing: [],
              points: directPoints,
            });
          }
        }
      }
      return fragments;
    };

    let fragmentsRaw: SectionFragment[] = collectFragments(edgeRecords);
    if (fragmentsRaw.length === 0) {
      const recordsWithGeometry = records.filter(
        (record) =>
          record.sections.length > 0 ||
          (record.sourcePoint != null && record.targetPoint != null) ||
          (record.bendPoints?.length ?? 0) >= 2,
      );
      fragmentsRaw = collectFragments(recordsWithGeometry);
    }

    if (fragmentsRaw.length === 0) {
      const sourceAnchor = nodeCenterById.get(edge.sourceId);
      const targetAnchor = nodeCenterById.get(edge.targetId);
      if (!sourceAnchor || !targetAnchor) {
        throw new Error(
          `[elk] edge ${edge.id}: no section geometry and missing node centers (${edge.sourceId} -> ${edge.targetId})`,
        );
      }
      if (isNear(sourceAnchor, targetAnchor)) {
        throw new Error(
          `[elk] edge ${edge.id}: no section geometry and degenerate anchors (${sourceAnchor.x},${sourceAnchor.y})`,
        );
      }
      return [sourceAnchor, targetAnchor];
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
        throw new Error(
          `[elk] edge ${edge.id}: conflicting records for section ${fragment.sectionId}`,
        );
      }
    }

    let points: Point[] = [];
    if (byId.size === 0) {
      if (unnamed.length === 0) {
        throw new Error(`[elk] edge ${edge.id}: expected at least one unnamed section, got 0`);
      }
      if (unnamed.length === 1) {
        points = [...unnamed[0].points];
      } else {
        const unique: SectionFragment[] = [];
        for (const fragment of unnamed) {
          const existing = unique.find(
            (candidate) =>
              candidate.points.length === fragment.points.length &&
              candidate.points.every((p, i) => isNear(p, fragment.points[i])),
          );
          if (!existing) unique.push(fragment);
        }
        if (unique.length !== 1) {
          throw new Error(
            `[elk] edge ${edge.id}: expected one unique unnamed section, got ${unique.length}`,
          );
        }
        points = [...unique[0].points];
      }
    } else {
      if (unnamed.length > 0) {
        throw new Error(`[elk] edge ${edge.id}: mixed named and unnamed sections`);
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
        throw new Error(`[elk] edge ${edge.id}: expected one root section, got ${roots.length}`);
      }

      const orderedIds: string[] = [];
      const visited = new Set<string>();
      let currentId: string | null = roots[0];
      while (currentId) {
        if (visited.has(currentId)) {
          throw new Error(`[elk] edge ${edge.id}: cycle in section chain at ${currentId}`);
        }
        visited.add(currentId);
        orderedIds.push(currentId);
        const current = byId.get(currentId);
        if (!current) break;
        const next = current.outgoing.filter((id) => byId.has(id)).sort();
        if (next.length > 1) {
          throw new Error(`[elk] edge ${edge.id}: branching section chain at ${currentId}`);
        }
        currentId = next.length === 1 ? next[0] : null;
      }

      if (visited.size !== byId.size) {
        throw new Error(
          `[elk] edge ${edge.id}: disconnected section graph (${visited.size}/${byId.size})`,
        );
      }

      for (const id of orderedIds) {
        const section = byId.get(id);
        if (!section) continue;
        points = appendSectionStrict(edge.id, points, section.points);
      }
    }

    return points;
  };

  const geoEdges: GeometryEdge[] = routedEdgeDefs.map((routed) => {
    const points = polylineForRoutedEdge(routed);
    const sourcePortRef = portAnchorMap.has(routed.sourceRef) ? routed.sourceRef : undefined;
    const targetPortRef = portAnchorMap.has(routed.targetRef) ? routed.targetRef : undefined;
    const sourceEdge = routed.sourceEdge;
    const srcName = entityNameMap.get(sourceEdge.source) ?? sourceEdge.source;
    const dstName = entityNameMap.get(sourceEdge.target) ?? sourceEdge.target;

    const edge: GeometryEdge = {
      id: routed.id,
      sourceId: routed.sourceId,
      targetId: routed.targetId,
      polyline: points,
      kind: sourceEdge.kind,
      data: {
        style: edgeStyle(sourceEdge),
        tooltip: edgeTooltip(sourceEdge, srcName, dstName),
        sourcePortRef,
        targetPortRef,
        markerSize: routed.markerSize,
        backtraceId: sourceEdge.backtraceId,
        sourceFrame: sourceEdge.sourceFrame,
        topFrame: sourceEdge.topFrame,
        frames: sourceEdge.frames,
        allFrames: sourceEdge.allFrames,
        framesLoading: sourceEdge.framesLoading,
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
    portAnchors: portAnchorMap,
  };
}
