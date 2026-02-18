import type { Node, Edge } from "@xyflow/react";
import type { FrameSummary } from "../api/types";
import type { ApiClient } from "../api/client";
import {
  convertSnapshot,
  getConnectedSubgraph,
  type EntityDef,
  type EdgeDef,
} from "../snapshot";
import {
  measureNodeDefs,
  layoutGraph,
  type LayoutResult,
  type RenderNodeForMeasure,
} from "../layout";

// ── Types ─────────────────────────────────────────────────────

export interface UnionLayout {
  /** Full ELK layout result (nodes with positions, edges with waypoints). */
  nodes: Node[];
  edges: Edge[];
  /** Per-frame converted data: frameIndex → { entities, edges }. */
  frameCache: Map<number, { entities: EntityDef[]; edges: EdgeDef[] }>;
  /** Which node IDs exist at each frame index. */
  nodePresence: Map<string, Set<number>>;
  /** Which edge IDs exist at each frame index. */
  edgePresence: Map<string, Set<number>>;
}

// ── Build ─────────────────────────────────────────────────────

const BATCH_SIZE = 20;

export async function buildUnionLayout(
  frames: FrameSummary[],
  apiClient: ApiClient,
  renderNode: RenderNodeForMeasure,
  onProgress?: (loaded: number, total: number) => void,
): Promise<UnionLayout> {
  const total = frames.length;
  const frameCache = new Map<number, { entities: EntityDef[]; edges: EdgeDef[] }>();

  // Fetch all frames in parallel batches.
  for (let batchStart = 0; batchStart < total; batchStart += BATCH_SIZE) {
    const batchEnd = Math.min(batchStart + BATCH_SIZE, total);
    const promises: Promise<void>[] = [];
    for (let i = batchStart; i < batchEnd; i++) {
      const frameIndex = frames[i].frame_index;
      promises.push(
        apiClient.fetchRecordingFrame(frameIndex).then((snapshot) => {
          const converted = convertSnapshot(snapshot);
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
  const sizes = await measureNodeDefs(unionEntities, renderNode);
  const layout = await layoutGraph(unionEntities, unionEdges, sizes);

  return {
    nodes: layout.nodes,
    edges: layout.edges,
    frameCache,
    nodePresence,
    edgePresence,
  };
}

// ── Per-frame rendering ───────────────────────────────────────

export function renderFrameFromUnion(
  frameIndex: number,
  unionLayout: UnionLayout,
  hiddenKrates: ReadonlySet<string>,
  hiddenProcesses: ReadonlySet<string>,
  focusedEntityId: string | null,
): LayoutResult {
  const frameData = unionLayout.frameCache.get(frameIndex);
  if (!frameData) return { nodes: [], edges: [] };

  // Apply krate/process filters.
  let filteredEntities = frameData.entities.filter(
    (e) =>
      (hiddenKrates.size === 0 || !hiddenKrates.has(e.krate ?? "~no-crate")) &&
      (hiddenProcesses.size === 0 || !hiddenProcesses.has(e.processId)),
  );
  let filteredEdges = frameData.edges;

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

  // Build a lookup from frame entity/edge data for updating node data.
  const frameEntityById = new Map(filteredEntities.map((e) => [e.id, e]));

  // Filter union layout nodes/edges to only those visible in this frame,
  // and update each visible node's data from the frame's EntityDef.
  const nodes: Node[] = [];
  for (const unionNode of unionLayout.nodes) {
    if (!visibleNodeIds.has(unionNode.id)) continue;
    const frameDef = frameEntityById.get(unionNode.id);
    if (!frameDef) continue;

    // Rebuild node data from the frame's entity (body/status may change per frame)
    // but keep the position from the union layout.
    let data: Record<string, unknown>;
    if (frameDef.channelPair) {
      data = {
        tx: frameDef.channelPair.tx,
        rx: frameDef.channelPair.rx,
        channelName: frameDef.name,
        selected: false,
        statTone: frameDef.statTone,
      };
    } else if (frameDef.rpcPair) {
      data = {
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

    nodes.push({
      ...unionNode,
      data,
    });
  }

  const edges: Edge[] = unionLayout.edges.filter((e) => visibleEdgeIds.has(e.id));

  return { nodes, edges };
}
