import type { FrameSummary } from "../api/types";
import type { ApiClient } from "../api/client";
import { canonicalNodeKind } from "../nodeKindSpec";
import {
  collapseEdgesThroughHiddenNodes,
  convertSnapshot,
  filterLoners,
  getConnectedSubgraph,
  type SnapshotGroupMode,
  type EntityDef,
  type EdgeDef,
} from "../snapshot";
import { measureGraphLayout } from "../graph/render/NodeLayer";
import { layoutGraph } from "../graph/elkAdapter";
import type { GraphGeometry, GeometryNode, GeometryEdge } from "../graph/geometry";

// ── Types ─────────────────────────────────────────────────────

export interface UnionLayout {
  /** Full geometry (nodes with world positions, edges with polylines). */
  geometry: GraphGeometry;
  /** Per-frame converted data: frameIndex → { entities, edges }. */
  frameCache: Map<number, { entities: EntityDef[]; edges: EdgeDef[] }>;
  /** Which node IDs exist at each frame index. */
  nodePresence: Map<string, Set<number>>;
  /** Which edge IDs exist at each frame index. */
  edgePresence: Map<string, Set<number>>;
  /** Sorted list of frame indices that were actually fetched and processed. */
  processedFrameIndices: number[];
}

export interface FrameRenderResult {
  geometry: GraphGeometry;
  ghostNodeIds: Set<string>;
  ghostEdgeIds: Set<string>;
}

// ── Build ─────────────────────────────────────────────────────

const BATCH_SIZE = 20;

function selectFrameIndices(frames: FrameSummary[], interval: number): number[] {
  if (frames.length === 0) return [];
  if (interval <= 1) return frames.map((f) => f.frame_index);
  const indices: number[] = [];
  for (let i = 0; i < frames.length; i++) {
    if (i === 0 || i === frames.length - 1 || i % interval === 0) {
      indices.push(frames[i].frame_index);
    }
  }
  return indices;
}

export async function buildUnionLayout(
  frames: FrameSummary[],
  apiClient: ApiClient,
  onProgress?: (loaded: number, total: number) => void,
  downsampleInterval: number = 1,
  groupMode: SnapshotGroupMode = "none",
): Promise<UnionLayout> {
  const processedFrameIndices = selectFrameIndices(frames, downsampleInterval);
  const total = processedFrameIndices.length;
  const frameCache = new Map<number, { entities: EntityDef[]; edges: EdgeDef[] }>();

  // Fetch processed frames in parallel batches.
  for (let batchStart = 0; batchStart < total; batchStart += BATCH_SIZE) {
    const batchEnd = Math.min(batchStart + BATCH_SIZE, total);
    const promises: Promise<void>[] = [];
    for (let i = batchStart; i < batchEnd; i++) {
      const frameIndex = processedFrameIndices[i];
      promises.push(
        apiClient.fetchRecordingFrame(frameIndex).then((snapshot) => {
          const converted = convertSnapshot(snapshot, groupMode);
          frameCache.set(frameIndex, converted);
        }),
      );
    }
    await Promise.all(promises);
    onProgress?.(batchEnd, total);
  }

  // Build union: collect all unique EntityDefs by ID (latest version wins),
  // all unique EdgeDefs by ID.
  const unionEntitiesById = new Map<string, EntityDef>();
  const unionEdgesById = new Map<string, EdgeDef>();
  const nodePresence = new Map<string, Set<number>>();
  const edgePresence = new Map<string, Set<number>>();

  for (const [frameIndex, { entities, edges }] of frameCache) {
    for (const entity of entities) {
      unionEntitiesById.set(entity.id, entity);
      if (!nodePresence.has(entity.id)) nodePresence.set(entity.id, new Set());
      nodePresence.get(entity.id)!.add(frameIndex);
    }
    for (const edge of edges) {
      unionEdgesById.set(edge.id, edge);
      if (!edgePresence.has(edge.id)) edgePresence.set(edge.id, new Set());
      edgePresence.get(edge.id)!.add(frameIndex);
    }
  }

  const unionEntities = Array.from(unionEntitiesById.values());
  const unionEdges = Array.from(unionEdgesById.values());

  // Measure and layout the full union graph.
  const measurements = await measureGraphLayout(unionEntities, groupMode);
  const geometry = await layoutGraph(unionEntities, unionEdges, measurements.nodeSizes, groupMode, {
    subgraphHeaderHeight: measurements.subgraphHeaderHeight,
  });

  return {
    geometry,
    frameCache,
    nodePresence,
    edgePresence,
    processedFrameIndices,
  };
}

// ── Nearest processed frame ───────────────────────────────────

/** Returns the processed frame index nearest to frameIndex. */
export function nearestProcessedFrame(frameIndex: number, processedFrameIndices: number[]): number {
  if (processedFrameIndices.length === 0) return 0;
  // Binary search: find the first index where processedFrameIndices[i] >= frameIndex.
  let lo = 0;
  let hi = processedFrameIndices.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (processedFrameIndices[mid] < frameIndex) lo = mid + 1;
    else hi = mid;
  }
  if (lo === processedFrameIndices.length) return processedFrameIndices[processedFrameIndices.length - 1];
  if (lo === 0) return processedFrameIndices[0];
  const lower = processedFrameIndices[lo - 1];
  const upper = processedFrameIndices[lo];
  return frameIndex - lower <= upper - frameIndex ? lower : upper;
}

// ── Entity diffs ──────────────────────────────────────────────

export interface EntityDiff {
  appeared: boolean;
  disappeared: boolean;
  statusChanged: { from: string; to: string } | null;
  statChanged: { from: string | undefined; to: string | undefined } | null;
  ageChange: number;
}

export function diffEntityBetweenFrames(
  entityId: string,
  currentFrameIndex: number,
  prevFrameIndex: number,
  unionLayout: UnionLayout,
): EntityDiff | null {
  const currentData = unionLayout.frameCache.get(currentFrameIndex);
  const prevData = unionLayout.frameCache.get(prevFrameIndex);

  const currentEntity = currentData?.entities.find((e) => e.id === entityId) ?? null;
  const prevEntity = prevData?.entities.find((e) => e.id === entityId) ?? null;

  if (!currentEntity && !prevEntity) return null;

  const appeared = !!currentEntity && !prevEntity;
  const disappeared = !currentEntity && !!prevEntity;

  let statusChanged: { from: string; to: string } | null = null;
  let statChanged: { from: string | undefined; to: string | undefined } | null = null;
  let ageChange = 0;

  if (currentEntity && prevEntity) {
    const fromStatus = prevEntity.status?.label ?? "?";
    const toStatus = currentEntity.status?.label ?? "?";
    if (fromStatus !== toStatus) {
      statusChanged = { from: fromStatus, to: toStatus };
    }
    if (currentEntity.stat !== prevEntity.stat) {
      statChanged = { from: prevEntity.stat, to: currentEntity.stat };
    }
    ageChange = currentEntity.ageMs - prevEntity.ageMs;
  }

  return { appeared, disappeared, statusChanged, statChanged, ageChange };
}

// ── Change summaries ──────────────────────────────────────────

export interface FrameChangeSummary {
  nodesAdded: number;
  nodesRemoved: number;
  edgesAdded: number;
  edgesRemoved: number;
}

export function computeFrameChangeSummary(
  frameIndex: number,
  prevFrameIndex: number | undefined,
  unionLayout: UnionLayout,
): FrameChangeSummary {
  let nodesAdded = 0;
  let nodesRemoved = 0;
  let edgesAdded = 0;
  let edgesRemoved = 0;

  for (const [, frames] of unionLayout.nodePresence) {
    const inCurrent = frames.has(frameIndex);
    const inPrev = prevFrameIndex !== undefined && frames.has(prevFrameIndex);
    if (inCurrent && !inPrev) nodesAdded++;
    if (!inCurrent && inPrev) nodesRemoved++;
  }

  for (const [, frames] of unionLayout.edgePresence) {
    const inCurrent = frames.has(frameIndex);
    const inPrev = prevFrameIndex !== undefined && frames.has(prevFrameIndex);
    if (inCurrent && !inPrev) edgesAdded++;
    if (!inCurrent && inPrev) edgesRemoved++;
  }

  return { nodesAdded, nodesRemoved, edgesAdded, edgesRemoved };
}

export function computeChangeSummaries(unionLayout: UnionLayout): Map<number, FrameChangeSummary> {
  const result = new Map<number, FrameChangeSummary>();
  const processed = unionLayout.processedFrameIndices;
  for (let i = 0; i < processed.length; i++) {
    const frameIdx = processed[i];
    const prevIdx = i > 0 ? processed[i - 1] : undefined;
    result.set(frameIdx, computeFrameChangeSummary(frameIdx, prevIdx, unionLayout));
  }
  return result;
}

export function computeChangeFrames(unionLayout: UnionLayout): number[] {
  const processed = unionLayout.processedFrameIndices;
  if (processed.length === 0) return [];
  const result: number[] = [processed[0]];

  for (let i = 1; i < processed.length; i++) {
    const frameIdx = processed[i];
    const prevIdx = processed[i - 1];
    let changed = false;
    for (const [, frames] of unionLayout.nodePresence) {
      if (frames.has(frameIdx) !== frames.has(prevIdx)) {
        changed = true;
        break;
      }
    }
    if (!changed) {
      for (const [, frames] of unionLayout.edgePresence) {
        if (frames.has(frameIdx) !== frames.has(prevIdx)) {
          changed = true;
          break;
        }
      }
    }
    if (changed) result.push(frameIdx);
  }

  return result;
}

// ── Per-frame rendering ───────────────────────────────────────

export function renderFrameFromUnion(
  frameIndex: number,
  unionLayout: UnionLayout,
  hiddenKrates: ReadonlySet<string>,
  hiddenProcesses: ReadonlySet<string>,
  hiddenKinds: ReadonlySet<string>,
  focusedEntityId: string | null,
  ghostMode?: boolean,
  showLoners: boolean = true,
): FrameRenderResult {
  const snappedIndex = nearestProcessedFrame(frameIndex, unionLayout.processedFrameIndices);
  const frameData = unionLayout.frameCache.get(snappedIndex);
  const emptyGeometry: GraphGeometry = { nodes: [], groups: [], edges: [], bounds: { x: 0, y: 0, width: 0, height: 0 } };
  if (!frameData) return { geometry: emptyGeometry, ghostNodeIds: new Set(), ghostEdgeIds: new Set() };

  // Apply krate/process filters.
  let filteredEntities = frameData.entities.filter(
    (e) =>
      (hiddenKrates.size === 0 || !hiddenKrates.has(e.krate ?? "~no-crate")) &&
      (hiddenProcesses.size === 0 || !hiddenProcesses.has(e.processId)) &&
      (hiddenKinds.size === 0 || !hiddenKinds.has(canonicalNodeKind(e.kind))),
  );
  let filteredEdges = frameData.edges;
  const filteredEntityIds = new Set(filteredEntities.map((entity) => entity.id));
  filteredEdges = collapseEdgesThroughHiddenNodes(filteredEdges, filteredEntityIds);

  if (!showLoners) {
    const withoutLoners = filterLoners(filteredEntities, filteredEdges);
    filteredEntities = withoutLoners.entities;
    filteredEdges = withoutLoners.edges;
  }

  // Apply focused entity subgraph filtering.
  if (focusedEntityId) {
    const subgraph = getConnectedSubgraph(focusedEntityId, filteredEntities, filteredEdges);
    filteredEntities = subgraph.entities;
    filteredEdges = subgraph.edges;
  }

  // Build the visible ID sets for this frame.
  const visibleNodeIds = new Set(filteredEntities.map((e) => e.id));
  const visibleEdgeIds = new Set(
    filteredEdges
      .filter((e) => visibleNodeIds.has(e.source) && visibleNodeIds.has(e.target))
      .map((e) => e.id),
  );

  // Build a lookup from frame entity data for updating node data.
  const frameEntityById = new Map(filteredEntities.map((e) => [e.id, e]));

  // Track all rendered node IDs (present + ghost) for edge validity in ghost mode.
  const renderedNodeIds = new Set<string>();
  const ghostNodeIds = new Set<string>();
  const ghostEdgeIds = new Set<string>();

  const nodes: GeometryNode[] = [];
  for (const unionNode of unionLayout.geometry.nodes) {
    const isPresent = visibleNodeIds.has(unionNode.id);

    if (isPresent) {
      const frameDef = frameEntityById.get(unionNode.id);
      if (!frameDef) continue;

      // Rebuild node data from the frame's entity (body/status may change per frame)
      // but keep the position from the union layout.
      let data: Record<string, unknown>;
      if (frameDef.channelPair) {
        data = {
          nodeId: frameDef.id,
          tx: frameDef.channelPair.tx,
          rx: frameDef.channelPair.rx,
          channelName: frameDef.name,
          selected: false,
          statTone: frameDef.statTone,
        };
      } else if (frameDef.rpcPair) {
        data = {
          nodeId: frameDef.id,
          req: frameDef.rpcPair.req,
          resp: frameDef.rpcPair.resp,
          rpcName: frameDef.name,
          selected: false,
        };
      } else {
        data = {
          kind: frameDef.kind,
          label: frameDef.name,
          inCycle: frameDef.inCycle,
          selected: false,
          status: frameDef.status,
          ageMs: frameDef.ageMs,
          stat: frameDef.stat,
          statTone: frameDef.statTone,
        };
      }

      nodes.push({ ...unionNode, data });
      renderedNodeIds.add(unionNode.id);
    } else if (ghostMode) {
      nodes.push({ ...unionNode, data: { ...unionNode.data, ghost: true } });
      ghostNodeIds.add(unionNode.id);
      renderedNodeIds.add(unionNode.id);
    }
  }

  const edges: GeometryEdge[] = [];
  for (const unionEdge of unionLayout.geometry.edges) {
    if (visibleEdgeIds.has(unionEdge.id)) {
      edges.push(unionEdge);
    } else if (
      ghostMode &&
      renderedNodeIds.has(unionEdge.sourceId) &&
      renderedNodeIds.has(unionEdge.targetId)
    ) {
      edges.push({ ...unionEdge, data: { ...unionEdge.data, ghost: true } });
      ghostEdgeIds.add(unionEdge.id);
    }
  }

  // Recompute bounds for the filtered set
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const node of nodes) {
    minX = Math.min(minX, node.worldRect.x);
    minY = Math.min(minY, node.worldRect.y);
    maxX = Math.max(maxX, node.worldRect.x + node.worldRect.width);
    maxY = Math.max(maxY, node.worldRect.y + node.worldRect.height);
  }
  const bounds = minX === Infinity
    ? { x: 0, y: 0, width: 0, height: 0 }
    : { x: minX, y: minY, width: maxX - minX, height: maxY - minY };

  // Keep groups from the union geometry (they always show).
  const geometry: GraphGeometry = {
    nodes,
    groups: unionLayout.geometry.groups,
    edges,
    bounds,
  };

  return { geometry, ghostNodeIds, ghostEdgeIds };
}
