import type { EntityBody, SnapshotCutResponse, SnapshotEdgeKind } from "./api/types";
import { canonicalScopeKind } from "./scopeKindSpec";

// ── Body type helpers ──────────────────────────────────────────

// TypeScript's `in` narrowing on complex union types produces `unknown` for
// nested property types. Use `Extract` to safely reference specific variants.
type RequestBody = Extract<EntityBody, { request: unknown }>;
type ResponseBody = Extract<EntityBody, { response: unknown }>;

// ── Display types ──────────────────────────────────────────────

export type Tone = "ok" | "warn" | "crit" | "neutral";

export type MetaValue = string | number | boolean | null | MetaValue[] | { [key: string]: MetaValue };

export type EntityDef = {
  /** Composite identity: "${processId}/${rawEntityId}". Unique across all processes. */
  id: string;
  /** Original entity ID as reported by the process. */
  rawEntityId: string;
  processId: string;
  processName: string;
  processPid: number;
  name: string;
  kind: string;
  body: EntityBody;
  source: string;
  krate?: string;
  /** Process-relative birth time in ms (PTime). Not comparable across processes. */
  birthPtime: number;
  /** Age at capture time: ptime_now_ms - birthPtime (clamped to 0). */
  ageMs: number;
  /** Approximate wall-clock birth: (captured_at_unix_ms - ptime_now_ms) + birthPtime. */
  birthApproxUnixMs: number;
  meta: Record<string, MetaValue>;
  inCycle: boolean;
  status: { label: string; tone: Tone };
  stat?: string;
  statTone?: Tone;
  /** Present when this is a merged TX/RX channel pair node. */
  channelPair?: { tx: EntityDef; rx: EntityDef };
  /** Present when this is a merged request/response RPC pair node. */
  rpcPair?: { req: EntityDef; resp: EntityDef };
};

export type EdgeDef = {
  id: string;
  source: string;
  target: string;
  kind: SnapshotEdgeKind;
  meta: Record<string, MetaValue>;
  opKind?: string;
  state?: string;
  pendingSincePtimeMs?: number;
  /** ELK port ID on the source node, when the source is a merged channel pair. */
  sourcePort?: string;
  /** ELK port ID on the target node, when the target is a merged channel pair. */
  targetPort?: string;
};

export type SnapshotGroupMode = "none" | "process" | "crate";

export type ScopeDef = {
  /** Composite key: `${processId}:${scopeId}` */
  key: string;
  processId: string;
  processName: string;
  processPid: number;
  scopeId: string;
  scopeName: string;
  /** Canonical scope kind: "process" | "thread" | "task" | "connection" | … */
  scopeKind: string;
  source: string;
  krate?: string;
  /** Process-relative birth time in ms. */
  birthPtime: number;
  /** Age at capture time: ptime_now_ms - birthPtime (clamped to 0). */
  ageMs: number;
  /** Composite entity IDs (`${processId}/${entityId}`) that belong to this scope. */
  memberEntityIds: string[];
};

export function extractScopes(snapshot: SnapshotCutResponse): ScopeDef[] {
  const result: ScopeDef[] = [];
  for (const proc of snapshot.processes) {
    const { process_id, process_name, pid, ptime_now_ms, scope_entity_links } = proc;
    const processIdStr = String(process_id);

    const membersByScope = new Map<string, string[]>();
    for (const link of scope_entity_links ?? []) {
      const compositeEntityId = `${processIdStr}/${link.entity_id}`;
      let list = membersByScope.get(link.scope_id);
      if (!list) {
        list = [];
        membersByScope.set(link.scope_id, list);
      }
      list.push(compositeEntityId);
    }

    for (const scope of proc.snapshot.scopes) {
      const memberEntityIds = membersByScope.get(scope.id) ?? [];
      result.push({
        key: `${processIdStr}:${scope.id}`,
        processId: processIdStr,
        processName: process_name,
        processPid: pid,
        scopeId: scope.id,
        scopeName: scope.name,
        scopeKind: canonicalScopeKind(scope.body ?? "unknown"),
        source: scope.source,
        krate: scope.krate,
        birthPtime: scope.birth,
        ageMs: Math.max(0, ptime_now_ms - scope.birth),
        memberEntityIds,
      });
    }
  }
  return result;
}

// ── Snapshot conversion ────────────────────────────────────────

export function bodyToKind(body: EntityBody): string {
  return typeof body === "string" ? body : Object.keys(body)[0];
}

export function deriveStatus(body: EntityBody): { label: string; tone: Tone } {
  if (typeof body === "string") return { label: "polling", tone: "neutral" };
  if ("request" in body) return { label: "in_flight", tone: "warn" };
  if ("response" in body) {
    const s = (body as ResponseBody).response.status;
    if (s === "ok") return { label: "ok", tone: "ok" };
    if (s === "error") return { label: "error", tone: "crit" };
    if (s === "cancelled") return { label: "cancelled", tone: "neutral" };
    return { label: "pending", tone: "warn" };
  }
  if ("lock" in body) return { label: "held", tone: "crit" };
  if ("channel_tx" in body || "channel_rx" in body) {
    const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
    return ep.lifecycle === "open"
      ? { label: "open", tone: "ok" }
      : { label: "closed", tone: "neutral" };
  }
  if ("semaphore" in body) {
    const { max_permits, handed_out_permits } = body.semaphore;
    const available = max_permits - handed_out_permits;
    return {
      label: `${available}/${max_permits} permits`,
      tone: handed_out_permits > 0 ? "warn" : "ok",
    };
  }
  if ("notify" in body) return { label: "waiting", tone: "neutral" };
  if ("once_cell" in body) {
    const s = body.once_cell.state;
    if (s === "initialized") return { label: "initialized", tone: "ok" };
    if (s === "initializing") return { label: "initializing", tone: "warn" };
    return { label: "empty", tone: "neutral" };
  }
  if ("command" in body) return { label: "running", tone: "neutral" };
  if ("file_op" in body) return { label: body.file_op.op, tone: "ok" };
  if ("net_connect" in body || "net_accept" in body || "net_read" in body || "net_write" in body) {
    return { label: "connected", tone: "ok" };
  }
  return { label: "unknown", tone: "neutral" };
}

export function deriveStat(body: EntityBody): string | undefined {
  if (typeof body === "string") return undefined;
  if ("semaphore" in body) {
    const { max_permits, handed_out_permits } = body.semaphore;
    return `${max_permits - handed_out_permits}/${max_permits}`;
  }
  if ("channel_tx" in body || "channel_rx" in body) {
    const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
    if ("mpsc" in ep.details && ep.details.mpsc.buffer) {
      const { occupancy, capacity } = ep.details.mpsc.buffer;
      return `${occupancy}/${capacity ?? "∞"}`;
    }
  }
  if ("notify" in body) {
    return body.notify.waiter_count > 0 ? `${body.notify.waiter_count} waiters` : undefined;
  }
  if ("once_cell" in body) {
    return body.once_cell.waiter_count > 0 ? `${body.once_cell.waiter_count} waiter` : undefined;
  }
  return undefined;
}

export function deriveStatTone(body: EntityBody): Tone | undefined {
  if (typeof body === "string") return undefined;
  if ("channel_tx" in body || "channel_rx" in body) {
    const ep = "channel_tx" in body ? body.channel_tx : body.channel_rx;
    if ("mpsc" in ep.details && ep.details.mpsc.buffer) {
      const { occupancy, capacity } = ep.details.mpsc.buffer;
      if (capacity == null) return undefined;
      if (occupancy >= capacity) return "crit";
      if (occupancy / capacity >= 0.75) return "warn";
    }
  }
  return undefined;
}

export function detectCycleNodes(entities: EntityDef[], edges: EdgeDef[]): Set<string> {
  const adj = new Map<string, string[]>();
  for (const e of edges) {
    if (e.kind !== "needs") continue;
    if (!adj.has(e.source)) adj.set(e.source, []);
    adj.get(e.source)!.push(e.target);
  }

  const inCycle = new Set<string>();
  const color = new Map<string, "gray" | "black">();

  function dfs(id: string, stack: string[]) {
    color.set(id, "gray");
    stack.push(id);
    for (const neighbor of adj.get(id) ?? []) {
      if (color.get(neighbor) === "gray") {
        const start = stack.indexOf(neighbor);
        for (const n of stack.slice(start)) inCycle.add(n);
      } else if (!color.has(neighbor)) {
        dfs(neighbor, stack);
      }
    }
    stack.pop();
    color.set(id, "black");
  }

  for (const entity of entities) {
    if (!color.has(entity.id)) dfs(entity.id, []);
  }
  return inCycle;
}

export function mergeChannelPairs(
  entities: EntityDef[],
  edges: EdgeDef[],
): { entities: EntityDef[]; edges: EdgeDef[] } {
  const channelLinks = edges.filter((e) => e.kind === "channel_link");
  const entityById = new Map(entities.map((e) => [e.id, e]));

  // Maps from original TX/RX entity id → merged id and port id
  const mergedIdFor = new Map<string, string>();
  const portIdFor = new Map<string, string>();
  const removedIds = new Set<string>();
  const mergedEntities: EntityDef[] = [];

  for (const link of channelLinks) {
    const txEntity = entityById.get(link.source);
    const rxEntity = entityById.get(link.target);
    if (!txEntity || !rxEntity) continue;
    // Guard against a TX or RX being part of multiple links (shouldn't happen)
    if (mergedIdFor.has(link.source) || mergedIdFor.has(link.target)) continue;

    const mergedId = `pair:${link.source}:${link.target}`;
    const txPortId = `${mergedId}:tx`;
    const rxPortId = `${mergedId}:rx`;

    mergedIdFor.set(link.source, mergedId);
    mergedIdFor.set(link.target, mergedId);
    portIdFor.set(link.source, txPortId);
    portIdFor.set(link.target, rxPortId);
    removedIds.add(link.source);
    removedIds.add(link.target);

    const channelName = txEntity.name.endsWith(":tx") ? txEntity.name.slice(0, -3) : txEntity.name;

    const mergedStatus =
      txEntity.status.tone === "ok" && rxEntity.status.tone === "ok"
        ? ({ label: "open", tone: "ok" } as const)
        : ({ label: "closed", tone: "neutral" } as const);

    mergedEntities.push({
      ...txEntity,
      id: mergedId,
      name: channelName,
      kind: "channel_pair",
      status: mergedStatus,
      stat: txEntity.stat,
      statTone: txEntity.statTone,
      inCycle: false, // set later by detectCycleNodes
      channelPair: { tx: txEntity, rx: rxEntity },
    });
  }

  const filteredEntities = entities.filter((e) => !removedIds.has(e.id));
  const newEntities = [...filteredEntities, ...mergedEntities];

  // Remove channel_link edges; remap sources/targets that pointed at TX/RX entities
  const newEdges = edges
    .filter((e) => e.kind !== "channel_link")
    .map((e) => {
      const origSource = e.source;
      const origTarget = e.target;
      const newSource = mergedIdFor.get(origSource) ?? origSource;
      const newTarget = mergedIdFor.get(origTarget) ?? origTarget;
      const sourcePort = mergedIdFor.has(origSource) ? portIdFor.get(origSource) : undefined;
      const targetPort = mergedIdFor.has(origTarget) ? portIdFor.get(origTarget) : undefined;
      if (newSource === origSource && newTarget === origTarget) return e;
      return { ...e, source: newSource, target: newTarget, sourcePort, targetPort };
    });

  return { entities: newEntities, edges: newEdges };
}

function groupKeyForEntity(entity: EntityDef, mode: SnapshotGroupMode): string {
  if (mode === "process") return `process:${entity.processId}`;
  if (mode === "crate") return `crate:${entity.krate ?? "~no-crate"}`;
  return "all";
}

function buildGroupKeyByEntity(
  entities: EntityDef[],
  mode: SnapshotGroupMode,
): Map<string, string> {
  const out = new Map<string, string>();
  for (const entity of entities) {
    out.set(entity.id, groupKeyForEntity(entity, mode));
  }
  return out;
}

export function mergeRpcPairs(
  entities: EntityDef[],
  edges: EdgeDef[],
  groupMode: SnapshotGroupMode = "none",
): { entities: EntityDef[]; edges: EdgeDef[] } {
  const rpcLinks = edges.filter((e) => e.kind === "rpc_link");
  const entityById = new Map(entities.map((e) => [e.id, e]));
  const groupKeyByEntity = buildGroupKeyByEntity(entities, groupMode);
  const mergedRpcLinkIds = new Set<string>();

  const mergedIdFor = new Map<string, string>();
  const portIdFor = new Map<string, string>();
  const removedIds = new Set<string>();
  const mergedEntities: EntityDef[] = [];

  for (const link of rpcLinks) {
    const reqEntity = entityById.get(link.source);
    const respEntity = entityById.get(link.target);
    if (!reqEntity || !respEntity) continue;
    if (groupKeyByEntity.get(reqEntity.id) !== groupKeyByEntity.get(respEntity.id)) continue;
    if (mergedIdFor.has(link.source) || mergedIdFor.has(link.target)) continue;

    const mergedId = `rpc_pair:${link.source}:${link.target}`;
    const reqPortId = `${mergedId}:req`;
    const respPortId = `${mergedId}:resp`;

    mergedIdFor.set(link.source, mergedId);
    mergedIdFor.set(link.target, mergedId);
    mergedRpcLinkIds.add(link.id);
    portIdFor.set(link.source, reqPortId);
    portIdFor.set(link.target, respPortId);
    removedIds.add(link.source);
    removedIds.add(link.target);

    const rpcName = reqEntity.name.endsWith(":req") ? reqEntity.name.slice(0, -4) : reqEntity.name;

    const respBody =
      typeof respEntity.body !== "string" && "response" in respEntity.body
        ? respEntity.body.response
        : null;
    const mergedStatus = respBody
      ? deriveStatus(respEntity.body)
      : { label: "in_flight", tone: "warn" as Tone };

    mergedEntities.push({
      ...reqEntity,
      id: mergedId,
      name: rpcName,
      kind: "rpc_pair",
      status: mergedStatus,
      inCycle: false,
      rpcPair: { req: reqEntity, resp: respEntity },
    });
  }

  const filteredEntities = entities.filter((e) => !removedIds.has(e.id));
  const newEntities = [...filteredEntities, ...mergedEntities];

  const newEdges = edges
    .filter((e) => e.kind !== "rpc_link" || !mergedRpcLinkIds.has(e.id))
    .map((e) => {
      const origSource = e.source;
      const origTarget = e.target;
      const newSource = mergedIdFor.get(origSource) ?? origSource;
      const newTarget = mergedIdFor.get(origTarget) ?? origTarget;
      const sourcePort = mergedIdFor.has(origSource) ? portIdFor.get(origSource) : undefined;
      const targetPort = mergedIdFor.has(origTarget) ? portIdFor.get(origTarget) : undefined;
      if (newSource === origSource && newTarget === origTarget) return e;
      return { ...e, source: newSource, target: newTarget, sourcePort, targetPort };
    });

  return { entities: newEntities, edges: newEdges };
}

function coalesceContextEdges(edges: EdgeDef[]): EdgeDef[] {
  // If we already have a richer causal/structural edge for a pair,
  // suppress parallel `touches` to avoid double-rendering the same relation.
  const hasNonTouchesForPair = new Set<string>();
  for (const edge of edges) {
    if (edge.kind === "touches") continue;
    hasNonTouchesForPair.add(`${edge.source}->${edge.target}`);
  }

  return edges.filter((edge) => {
    if (edge.kind !== "touches") return true;
    return !hasNonTouchesForPair.has(`${edge.source}->${edge.target}`);
  });
}

export function convertSnapshot(
  snapshot: SnapshotCutResponse,
  groupMode: SnapshotGroupMode = "none",
): {
  entities: EntityDef[];
  edges: EdgeDef[];
} {
  const allEntities: EntityDef[] = [];
  const allEdges: EdgeDef[] = [];

  // First pass: collect all entities so we can do cross-process edge resolution.
  for (const proc of snapshot.processes) {
    const { process_id, process_name, pid, ptime_now_ms } = proc;
    const anchorUnixMs = snapshot.captured_at_unix_ms - ptime_now_ms;

    for (const e of proc.snapshot.entities) {
      const compositeId = `${process_id}/${e.id}`;
      const ageMs = Math.max(0, ptime_now_ms - e.birth);
      allEntities.push({
        id: compositeId,
        rawEntityId: e.id,
        processId: String(process_id),
        processName: process_name,
        processPid: pid,
        name: e.name,
        kind: bodyToKind(e.body),
        body: e.body,
        source: e.source,
        krate: e.krate,
        birthPtime: e.birth,
        ageMs,
        birthApproxUnixMs: anchorUnixMs + e.birth,
        meta: (e.meta ?? {}) as Record<string, MetaValue>,
        inCycle: false,
        status: deriveStatus(e.body),
        stat: deriveStat(e.body),
        statTone: deriveStatTone(e.body),
      });
    }
  }

  // Build raw entity ID → composite ID lookup for cross-process edge resolution.
  // rpc_link edges have their src set to the request's raw ID from the other process.
  const rawToCompositeId = new Map<string, string>();
  for (const entity of allEntities) {
    rawToCompositeId.set(entity.rawEntityId, entity.id);
  }

  // Second pass: build edges, resolving cross-process src IDs for rpc_link.
  for (const proc of snapshot.processes) {
    const { process_id } = proc;
    for (let i = 0; i < proc.snapshot.edges.length; i++) {
      const e = proc.snapshot.edges[i];
      const localSrc = `${process_id}/${e.src}`;
      const srcComposite =
        e.kind === "rpc_link" ? (rawToCompositeId.get(e.src) ?? localSrc) : localSrc;
      const dstComposite = `${process_id}/${e.dst}`;
      allEdges.push({
        id: `e${i}-${srcComposite}-${dstComposite}-${e.kind}`,
        source: srcComposite,
        target: dstComposite,
        kind: e.kind,
        meta: (e.meta ?? {}) as Record<string, MetaValue>,
        opKind:
          e.meta && typeof e.meta.op_kind === "string"
            ? (e.meta.op_kind as string)
            : undefined,
        state:
          e.meta && typeof e.meta.state === "string" ? (e.meta.state as string) : undefined,
        pendingSincePtimeMs:
          e.meta && typeof e.meta.pending_since_ptime_ms === "number"
            ? (e.meta.pending_since_ptime_ms as number)
            : undefined,
      });
    }
  }

  const { entities: channelMerged, edges: channelEdges } = mergeChannelPairs(allEntities, allEdges);
  const { entities: mergedEntities, edges: mergedEdges } = mergeRpcPairs(
    channelMerged,
    channelEdges,
    groupMode,
  );

  const coalescedEdges = coalesceContextEdges(mergedEdges);
  const cycleIds = detectCycleNodes(mergedEntities, coalescedEdges);
  for (const entity of mergedEntities) {
    entity.inCycle = cycleIds.has(entity.id);
  }

  return { entities: mergedEntities, edges: coalescedEdges };
}

export function getConnectedSubgraph(
  entityId: string,
  entities: EntityDef[],
  edges: EdgeDef[],
): { entities: EntityDef[]; edges: EdgeDef[] } {
  const connectedIds = new Set<string>();
  const queue = [entityId];
  while (queue.length > 0) {
    const id = queue.pop()!;
    if (connectedIds.has(id)) continue;
    connectedIds.add(id);
    for (const e of edges) {
      if (e.source === id && !connectedIds.has(e.target)) queue.push(e.target);
      if (e.target === id && !connectedIds.has(e.source)) queue.push(e.source);
    }
  }
  return {
    entities: entities.filter((e) => connectedIds.has(e.id)),
    edges: edges.filter((e) => connectedIds.has(e.source) && connectedIds.has(e.target)),
  };
}

export function filterLoners(
  entities: EntityDef[],
  edges: EdgeDef[],
): { entities: EntityDef[]; edges: EdgeDef[] } {
  const entityIds = new Set(entities.map((entity) => entity.id));
  const inScopeEdges = edges.filter(
    (edge) => entityIds.has(edge.source) && entityIds.has(edge.target),
  );

  const connectedIds = new Set<string>();
  for (const edge of inScopeEdges) {
    // Self-loops do not connect to "any other node".
    if (edge.source === edge.target) continue;
    connectedIds.add(edge.source);
    connectedIds.add(edge.target);
  }

  return {
    entities: entities.filter((entity) => connectedIds.has(entity.id)),
    edges: inScopeEdges.filter(
      (edge) => connectedIds.has(edge.source) && connectedIds.has(edge.target),
    ),
  };
}

export function collapseEdgesThroughHiddenNodes(
  edges: EdgeDef[],
  visibleEntityIds: ReadonlySet<string>,
): EdgeDef[] {
  const outgoing = new Map<string, EdgeDef[]>();
  const incoming = new Map<string, EdgeDef[]>();
  for (const edge of edges) {
    if (!outgoing.has(edge.source)) outgoing.set(edge.source, []);
    outgoing.get(edge.source)!.push(edge);
    if (!incoming.has(edge.target)) incoming.set(edge.target, []);
    incoming.get(edge.target)!.push(edge);
  }

  const visibleDirectEdges = edges.filter(
    (edge) => visibleEntityIds.has(edge.source) && visibleEntityIds.has(edge.target),
  );
  const resultById = new Map<string, EdgeDef>(visibleDirectEdges.map((edge) => [edge.id, edge]));
  const pairKey = (a: string, b: string): string => (a < b ? `${a}<->${b}` : `${b}<->${a}`);
  const visibleDirectPairs = new Set(visibleDirectEdges.map((edge) => pairKey(edge.source, edge.target)));

  for (const sourceId of visibleEntityIds) {
    const firstHopOutgoing = outgoing.get(sourceId) ?? [];
    const firstHopIncoming = incoming.get(sourceId) ?? [];
    const queue: string[] = [];
    const visitedHidden = new Set<string>();

    for (const edge of firstHopOutgoing) {
      if (visibleEntityIds.has(edge.target)) continue;
      queue.push(edge.target);
    }
    for (const edge of firstHopIncoming) {
      if (visibleEntityIds.has(edge.source)) continue;
      queue.push(edge.source);
    }

    while (queue.length > 0) {
      const hiddenId = queue.shift()!;
      if (visitedHidden.has(hiddenId)) continue;
      visitedHidden.add(hiddenId);

      const visitNeighbor = (targetId: string) => {
        if (targetId === sourceId) return;
        if (visibleEntityIds.has(targetId)) {
          const directPairKey = pairKey(sourceId, targetId);
          if (visibleDirectPairs.has(directPairKey)) return;
          const [left, right] = sourceId < targetId ? [sourceId, targetId] : [targetId, sourceId];
          const collapsedId = `collapsed-${left}-${right}`;
          if (!resultById.has(collapsedId)) {
            resultById.set(collapsedId, {
              id: collapsedId,
              source: left,
              target: right,
              kind: "touches",
              meta: { collapsed: true },
            });
          }
          return;
        }
        if (!visitedHidden.has(targetId)) {
          queue.push(targetId);
        }
      };

      for (const edge of outgoing.get(hiddenId) ?? []) {
        visitNeighbor(edge.target);
      }
      for (const edge of incoming.get(hiddenId) ?? []) {
        visitNeighbor(edge.source);
      }
    }
  }

  return Array.from(resultById.values());
}
