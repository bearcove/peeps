import type {
  BacktraceFrameResolved,
  EdgeKind,
  EntityBody,
  EventKind,
  SnapshotFrameRecord,
  SnapshotBacktraceFrame,
  SnapshotCutResponse,
  SnapshotSymbolicationUpdate,
} from "./api/types.generated";
import { registerCustomKindSpec } from "./nodeKindSpec";
import { canonicalScopeKind } from "./scopeKindSpec";

// ── Body type helpers ──────────────────────────────────────────

// TypeScript's `in` narrowing on complex union types produces `unknown` for
// nested property types. Use `Extract` to safely reference specific variants.
type ResponseBody = Extract<EntityBody, { response: unknown }>;

// ── Display types ──────────────────────────────────────────────

// f[impl display.tone]
export type Tone = "ok" | "warn" | "crit" | "neutral";

export type MetaValue = string | number | boolean | null | MetaValue[] | { [key: string]: MetaValue };

export type RenderSource = {
  path: string;
  line: number;
  krate: string;
};

export type RenderTopFrame = {
  function_name: string;
  crate_name: string;
  module_path: string;
  source_file: string;
  line?: number;
  column?: number;
  frame_id?: number;
};

// f[impl display.entity]
export type EntityDef = {
  id: string;
  processId: string;
  processName: string;
  processPid: number;
  name: string;
  kind: string;
  body: EntityBody;
  backtraceId: number;
  source: RenderSource;
  krate?: string;
  topFrame?: RenderTopFrame;
  /** All non-system resolved frames from the backtrace, outermost first. */
  frames: RenderTopFrame[];
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
  /** Process-relative time when this entity was logically removed (dead). */
  removedAt?: number;
  /** Present when this is a merged TX/RX channel pair node. */
  channelPair?: { tx: EntityDef; rx: EntityDef };
  /** Present when this is a merged request/response RPC pair node. */
  rpcPair?: { req: EntityDef; resp: EntityDef };
  /** For lock entities: name of the entity currently holding the lock. */
  holderName?: string;
};

// f[impl display.edge]
export type EdgeDef = {
  id: string;
  source: string;
  target: string;
  kind: EdgeKind;
  /** ELK port ID on the source node, when the source is a merged channel pair. */
  sourcePort?: string;
  /** ELK port ID on the target node, when the target is a merged channel pair. */
  targetPort?: string;
};

export type SnapshotGroupMode = "none" | "process" | "crate";

// f[impl display.scope]
export type ScopeDef = {
  key: string;
  processId: string;
  processName: string;
  processPid: number;
  scopeId: string;
  scopeName: string;
  /** Canonical scope kind: "process" | "thread" | "task" | "connection" | … */
  scopeKind: string;
  backtraceId: number;
  source: RenderSource;
  krate?: string;
  topFrame?: RenderTopFrame;
  /** Process-relative birth time in ms. */
  birthPtime: number;
  /** Age at capture time: ptime_now_ms - birthPtime (clamped to 0). */
  ageMs: number;
  memberEntityIds: string[];
};

export type ResolvedSnapshotBacktrace = {
  backtrace_id: number;
  frame_ids: number[];
  frames: SnapshotBacktraceFrame[];
};

export type BacktraceIndex = Map<number, ResolvedSnapshotBacktrace>;
export type FrameCatalog = Map<number, SnapshotBacktraceFrame>;

function backtraceSource(backtraceId: number): RenderSource {
  return { path: `backtrace:${backtraceId}`, line: 0, krate: "~no-crate" };
}

// f[impl display.backtrace.required]
function requireBacktraceId(owner: unknown, context: string, processId: string): number {
  const value = (owner as { backtrace?: unknown }).backtrace;
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) {
    throw new Error(`[snapshot] ${context} missing/invalid backtrace in process ${processId}`);
  }
  return value;
}

export function buildBacktraceIndex(snapshot: SnapshotCutResponse): BacktraceIndex {
  // f[impl display.backtrace.catalog]
  const frameCatalog: FrameCatalog = new Map<number, SnapshotBacktraceFrame>();
  for (const frame of snapshot.frames) {
    const id = frame.frame_id;
    if (!Number.isInteger(id) || id <= 0) {
      throw new Error(`[snapshot] invalid frame id ${String(id)} in snapshot frames`);
    }
    if (frameCatalog.has(id)) {
      throw new Error(`[snapshot] duplicate frame id ${id} in snapshot frames`);
    }
    frameCatalog.set(id, frame.frame);
  }

  const index: BacktraceIndex = new Map<number, ResolvedSnapshotBacktrace>();
  for (const record of snapshot.backtraces) {
    const id = record.backtrace_id;
    if (!Number.isInteger(id) || id <= 0) {
      throw new Error(`[snapshot] invalid backtrace id ${String(id)} in snapshot backtraces`);
    }
    if (index.has(id)) {
      throw new Error(`[snapshot] duplicate backtrace ${id} in snapshot backtraces`);
    }
    const frames = record.frame_ids.map((frameId) => {
      const frame = frameCatalog.get(frameId);
      if (!frame) {
        throw new Error(`[snapshot] backtrace ${id} references missing frame id ${frameId}`);
      }
      return frame;
    });
    index.set(id, {
      backtrace_id: id,
      frame_ids: record.frame_ids,
      frames,
    });
  }
  return index;
}

export function isResolvedFrame(frame: SnapshotBacktraceFrame): frame is { resolved: BacktraceFrameResolved } {
  return "resolved" in frame;
}

export function isPendingFrame(frame: SnapshotBacktraceFrame): boolean {
  return "unresolved" in frame && frame.unresolved.reason === "symbolication pending";
}

function crateFromFunctionName(functionName: string): string {
  const crate = functionName.split("::")[0]?.trim();
  return crate && crate.length > 0 ? crate : "~no-crate";
}

const SYSTEM_CRATES = new Set([
  "std",
  "core",
  "alloc",
  "tokio",
  "tokio_util",
  "futures",
  "futures_core",
  "futures_util",
  "moire",
  "moire_trace_capture",
  "moire_runtime",
  "moire_tokio",
]);

export function isSystemCrate(krate: string): boolean {
  return SYSTEM_CRATES.has(krate);
}

function resolveBacktraceDisplay(
  backtraces: Map<number, ResolvedSnapshotBacktrace>,
  backtraceId: number,
  _context: string,
): { source: RenderSource; topFrame?: RenderTopFrame; frames: RenderTopFrame[] } {
  const record = backtraces.get(backtraceId);
  if (!record) {
    return {
      source: backtraceSource(backtraceId),
      topFrame: undefined,
      frames: [],
    };
  }

  // Collect all non-system resolved frames.
  const frames: RenderTopFrame[] = [];
  for (let i = 0; i < record.frames.length; i++) {
    const f = record.frames[i];
    if (!isResolvedFrame(f)) continue;
    const krate = crateFromFunctionName(f.resolved.function_name);
    if (SYSTEM_CRATES.has(krate)) continue;
    frames.push({
      function_name: f.resolved.function_name,
      crate_name: krate,
      module_path: f.resolved.module_path,
      source_file: f.resolved.source_file,
      line: f.resolved.line,
      frame_id: record.frame_ids[i],
    });
  }

  // Prefer the first non-system resolved frame; fall back to first resolved.
  let topFrameIndex = record.frames.findIndex((f) => {
    if (!isResolvedFrame(f)) return false;
    return !SYSTEM_CRATES.has(crateFromFunctionName(f.resolved.function_name));
  });
  if (topFrameIndex === -1) {
    topFrameIndex = record.frames.findIndex(isResolvedFrame);
  }
  const firstResolved = topFrameIndex !== -1 ? record.frames[topFrameIndex] : undefined;

  if (firstResolved && isResolvedFrame(firstResolved)) {
    const krate = crateFromFunctionName(firstResolved.resolved.function_name);
    return {
      source: {
        path: firstResolved.resolved.source_file,
        line: firstResolved.resolved.line ?? 0,
        krate,
      },
      topFrame: {
        function_name: firstResolved.resolved.function_name,
        crate_name: krate,
        module_path: firstResolved.resolved.module_path,
        source_file: firstResolved.resolved.source_file,
        line: firstResolved.resolved.line,
        frame_id: record.frame_ids[topFrameIndex],
      },
      frames,
    };
  }

  const firstUnresolved = record.frames.find((frame) => "unresolved" in frame);
  if (firstUnresolved && "unresolved" in firstUnresolved) {
    return {
      source: {
        path: firstUnresolved.unresolved.module_path,
        line: 0,
        krate: "~unresolved",
      },
      topFrame: undefined,
      frames,
    };
  }

  return {
    source: backtraceSource(backtraceId),
    topFrame: undefined,
    frames,
  };
}

export function applySymbolicationUpdateToSnapshot(
  snapshot: SnapshotCutResponse,
  update: SnapshotSymbolicationUpdate,
): SnapshotCutResponse {
  if (update.snapshot_id !== snapshot.snapshot_id) {
    throw new Error(
      `[snapshot] symbolication update for snapshot ${update.snapshot_id} cannot apply to snapshot ${snapshot.snapshot_id}`,
    );
  }
  if (!Number.isInteger(update.total_frames) || update.total_frames < 0) {
    throw new Error(`[snapshot] invalid symbolication total_frames ${String(update.total_frames)}`);
  }
  if (!Number.isInteger(update.completed_frames) || update.completed_frames < 0) {
    throw new Error(`[snapshot] invalid symbolication completed_frames ${String(update.completed_frames)}`);
  }

  const frameMap = new Map<number, SnapshotFrameRecord>();
  for (const record of snapshot.frames) {
    frameMap.set(record.frame_id, record);
  }
  for (const record of update.updated_frames) {
    frameMap.set(record.frame_id, record);
  }

  return {
    ...snapshot,
    frames: Array.from(frameMap.values()).sort((a, b) => a.frame_id - b.frame_id),
  };
}

// f[impl display.id.scope-key]
export function extractScopes(snapshot: SnapshotCutResponse): ScopeDef[] {
  const backtraces = buildBacktraceIndex(snapshot);
  const result: ScopeDef[] = [];
  for (const proc of snapshot.processes) {
    const { process_id, process_name, pid, ptime_now_ms, scope_entity_links } = proc;
    const processIdStr = String(process_id);

    const membersByScope = new Map<string, string[]>();
    for (const link of scope_entity_links ?? []) {
      let list = membersByScope.get(link.scope_id);
      if (!list) {
        list = [];
        membersByScope.set(link.scope_id, list);
      }
      list.push(link.entity_id);
    }

    for (const scope of proc.snapshot.scopes) {
      const memberEntityIds = membersByScope.get(scope.id) ?? [];
      const backtraceId = requireBacktraceId(scope, `scope ${processIdStr}/${scope.id}`, process_id);
      const resolvedBacktrace = resolveBacktraceDisplay(
        backtraces,
        backtraceId,
        `scope ${processIdStr}/${scope.id}`,
      );
      result.push({
        key: `${processIdStr}:${scope.id}`,
        processId: processIdStr,
        processName: process_name,
        processPid: pid,
        scopeId: scope.id,
        scopeName: scope.name,
        scopeKind: canonicalScopeKind(scope.body),
        backtraceId,
        source: resolvedBacktrace.source,
        krate: resolvedBacktrace.source.krate,
        topFrame: resolvedBacktrace.topFrame,
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
  if ("custom" in body) return body.custom.kind;
  return Object.keys(body)[0];
}

// f[impl display.entity.status]
export function deriveStatus(body: EntityBody): { label: string; tone: Tone } {
  if ("future" in body) return { label: "polling", tone: "neutral" };
  if ("request" in body) return { label: "in_flight", tone: "warn" };
  if ("response" in body) {
    const s = (body as ResponseBody).response.status;
    if (s === "pending") return { label: "pending", tone: "warn" };
    if (s === "cancelled") return { label: "cancelled", tone: "neutral" };
    if ("ok" in s) return { label: "ok", tone: "ok" };
    if ("error" in s) return { label: "error", tone: "crit" };
    return { label: "pending", tone: "warn" };
  }
  if ("lock" in body) return { label: "unlocked", tone: "ok" };
  if ("mpsc_tx" in body || "mpsc_rx" in body) return { label: "active", tone: "ok" };
  if ("broadcast_tx" in body) return { label: "active", tone: "ok" };
  if ("broadcast_rx" in body) {
    const { lag } = body.broadcast_rx;
    return lag > 0 ? { label: `lag: ${lag}`, tone: "warn" } : { label: "active", tone: "ok" };
  }
  if ("watch_tx" in body || "watch_rx" in body) return { label: "active", tone: "ok" };
  if ("oneshot_tx" in body) {
    return body.oneshot_tx.sent
      ? { label: "sent", tone: "ok" }
      : { label: "pending", tone: "neutral" };
  }
  if ("oneshot_rx" in body) return { label: "waiting", tone: "neutral" };
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
  if ("custom" in body) return { label: "active", tone: "neutral" };
  return { label: "unknown", tone: "neutral" };
}

// f[impl display.entity.stat]
export function deriveStat(body: EntityBody): string | undefined {
  if ("semaphore" in body) {
    const { max_permits, handed_out_permits } = body.semaphore;
    return `${max_permits - handed_out_permits}/${max_permits}`;
  }
  if ("mpsc_tx" in body) {
    const { queue_len, capacity } = body.mpsc_tx;
    return `${queue_len}/${capacity ?? "∞"}`;
  }
  if ("notify" in body) {
    return body.notify.waiter_count > 0 ? `${body.notify.waiter_count} waiters` : undefined;
  }
  if ("once_cell" in body) {
    return body.once_cell.waiter_count > 0 ? `${body.once_cell.waiter_count} waiter` : undefined;
  }
  return undefined;
}

// f[impl display.entity.stat-tone]
export function deriveStatTone(body: EntityBody): Tone | undefined {
  if ("mpsc_tx" in body) {
    const { queue_len, capacity } = body.mpsc_tx;
    if (capacity == null) return undefined;
    if (queue_len >= capacity) return "crit";
    if (queue_len / capacity >= 0.75) return "warn";
  }
  return undefined;
}

// f[impl merge.cycles]
export function detectCycleNodes(entities: EntityDef[], edges: EdgeDef[]): Set<string> {
  const adj = new Map<string, string[]>();
  for (const e of edges) {
    if (e.kind !== "waiting_on") continue;
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

const TX_KINDS = new Set(["mpsc_tx", "broadcast_tx", "watch_tx", "oneshot_tx"]);
const RX_KINDS = new Set(["mpsc_rx", "broadcast_rx", "watch_rx", "oneshot_rx"]);

const TX_KIND_TO_PAIR_KIND: Record<string, string> = {
  mpsc_tx: "channel_pair",
  broadcast_tx: "broadcast_pair",
  watch_tx: "watch_pair",
  oneshot_tx: "oneshot_pair",
};

// f[impl merge.channel] f[impl merge.channel.id] f[impl merge.channel.name] f[impl merge.channel.status] f[impl merge.channel.guard]
export function mergeChannelPairs(
  entities: EntityDef[],
  edges: EdgeDef[],
): { entities: EntityDef[]; edges: EdgeDef[] } {
  const entityById = new Map(entities.map((e) => [e.id, e]));
  const channelLinks = edges.filter((e) => {
    if (e.kind !== "paired_with") return false;
    const src = entityById.get(e.source);
    const dst = entityById.get(e.target);
    return !!(src && dst && TX_KINDS.has(src.kind) && RX_KINDS.has(dst.kind));
  });
  const channelLinkIds = new Set(channelLinks.map((e) => e.id));

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

    const pairKind = TX_KIND_TO_PAIR_KIND[txEntity.kind] ?? "channel_pair";

    const mergedStatus =
      txEntity.status.tone === "ok" && rxEntity.status.tone === "ok"
        ? ({ label: "open", tone: "ok" } as const)
        : ({ label: "closed", tone: "neutral" } as const);

    mergedEntities.push({
      ...txEntity,
      id: mergedId,
      name: channelName,
      kind: pairKind,
      status: mergedStatus,
      stat: txEntity.stat,
      statTone: txEntity.statTone,
      inCycle: false, // set later by detectCycleNodes
      channelPair: { tx: txEntity, rx: rxEntity },
    });
  }

  const filteredEntities = entities.filter((e) => !removedIds.has(e.id));
  const newEntities = [...filteredEntities, ...mergedEntities];

  // Remove channel paired_with edges; remap sources/targets that pointed at TX/RX entities
  const newEdges = edges
    .filter((e) => !(e.kind === "paired_with" && channelLinkIds.has(e.id)))
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
  if (mode === "crate") return `crate:${entity.topFrame?.crate_name ?? "~no-crate"}`;
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

// f[impl merge.rpc] f[impl merge.rpc.id] f[impl merge.rpc.name] f[impl merge.rpc.status] f[impl merge.rpc.group]
export function mergeRpcPairs(
  entities: EntityDef[],
  edges: EdgeDef[],
  groupMode: SnapshotGroupMode = "none",
): { entities: EntityDef[]; edges: EdgeDef[] } {
  const entityById = new Map(entities.map((e) => [e.id, e]));
  const groupKeyByEntity = buildGroupKeyByEntity(entities, groupMode);
  const mergedRpcLinkIds = new Set<string>();

  const rpcLinks = edges.filter((e) => {
    if (e.kind !== "paired_with") return false;
    const src = entityById.get(e.source);
    const dst = entityById.get(e.target);
    return !!(
      src &&
      dst &&
      ((src.kind === "request" && dst.kind === "response") ||
        (src.kind === "response" && dst.kind === "request"))
    );
  });

  const mergedIdFor = new Map<string, string>();
  const portIdFor = new Map<string, string>();
  const removedIds = new Set<string>();
  const mergedEntities: EntityDef[] = [];

  for (const link of rpcLinks) {
    const srcEntity = entityById.get(link.source);
    const dstEntity = entityById.get(link.target);
    if (!srcEntity || !dstEntity) continue;

    // Rust emits paired_with as response → request; handle both directions
    let reqEntity: EntityDef, respEntity: EntityDef;
    if (srcEntity.kind === "request") {
      reqEntity = srcEntity;
      respEntity = dstEntity;
    } else {
      respEntity = srcEntity;
      reqEntity = dstEntity;
    }

    if (groupKeyByEntity.get(reqEntity.id) !== groupKeyByEntity.get(respEntity.id)) continue;
    if (mergedIdFor.has(reqEntity.id) || mergedIdFor.has(respEntity.id)) continue;

    const mergedId = `rpc_pair:${reqEntity.id}:${respEntity.id}`;
    const reqPortId = `${mergedId}:req`;
    const respPortId = `${mergedId}:resp`;

    mergedIdFor.set(reqEntity.id, mergedId);
    mergedIdFor.set(respEntity.id, mergedId);
    mergedRpcLinkIds.add(link.id);
    portIdFor.set(reqEntity.id, reqPortId);
    portIdFor.set(respEntity.id, respPortId);
    removedIds.add(reqEntity.id);
    removedIds.add(respEntity.id);

    const rpcName = reqEntity.name.endsWith(":req") ? reqEntity.name.slice(0, -4) : reqEntity.name;

    const respBody =
      "response" in respEntity.body ? (respEntity.body as ResponseBody).response : null;
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
    .filter((e) => e.kind !== "paired_with" || !mergedRpcLinkIds.has(e.id))
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

// f[impl merge.coalesce]
function coalesceContextEdges(edges: EdgeDef[]): EdgeDef[] {
  // If we already have a richer causal/structural edge for a pair,
  // suppress parallel `polls` to avoid double-rendering the same relation.
  const hasNonPollsForPair = new Set<string>();
  for (const edge of edges) {
    if (edge.kind === "polls") continue;
    hasNonPollsForPair.add(`${edge.source}->${edge.target}`);
  }

  return edges.filter((edge) => {
    if (edge.kind !== "polls") return true;
    return !hasNonPollsForPair.has(`${edge.source}->${edge.target}`);
  });
}

// f[impl convert.order] f[impl display.id.composite]
export function convertSnapshot(
  snapshot: SnapshotCutResponse,
  groupMode: SnapshotGroupMode = "none",
): {
  entities: EntityDef[];
  edges: EdgeDef[];
} {
  const backtraces = buildBacktraceIndex(snapshot);
  const allEntities: EntityDef[] = [];
  const allEdges: EdgeDef[] = [];

  // First pass: collect all entities so we can do cross-process edge resolution.
  for (const proc of snapshot.processes) {
    const { process_id, process_name, pid, ptime_now_ms } = proc;
    const anchorUnixMs = snapshot.captured_at_unix_ms - ptime_now_ms;

    for (const e of proc.snapshot.entities) {
      const ageMs = Math.max(0, ptime_now_ms - e.birth);
      const backtraceId = requireBacktraceId(e, `entity ${e.id}`, process_id);
      const resolvedBacktrace = resolveBacktraceDisplay(
        backtraces,
        backtraceId,
        `entity ${e.id}`,
      );
      const kind = bodyToKind(e.body);
      if ("custom" in e.body) {
        const c = e.body.custom;
        registerCustomKindSpec(c.kind, c.display_name, c.category, c.icon);
      }
      allEntities.push({
        id: e.id,
        processId: String(process_id),
        processName: process_name,
        processPid: pid,
        name: e.name,
        kind,
        body: e.body,
        backtraceId,
        source: resolvedBacktrace.source,
        krate: resolvedBacktrace.source.krate,
        topFrame: resolvedBacktrace.topFrame,
        frames: resolvedBacktrace.frames,
        birthPtime: e.birth,
        ageMs,
        birthApproxUnixMs: anchorUnixMs + e.birth,
        removedAt: e.removed_at,
        meta: {},
        inCycle: false,
        status: deriveStatus(e.body),
        stat: deriveStat(e.body),
        statTone: deriveStatTone(e.body),
      });
    }
  }

  // Index lock entities by id so we can patch them when we see `holds` edges.
  const lockEntitiesById = new Map<string, EntityDef>();
  const entityById = new Map<string, EntityDef>();
  for (const ent of allEntities) {
    entityById.set(ent.id, ent);
    if ("lock" in ent.body) lockEntitiesById.set(ent.id, ent);
  }

  // Second pass: build edges, and derive lock state from `holds` edges inline.
  for (const proc of snapshot.processes) {
    for (let i = 0; i < proc.snapshot.edges.length; i++) {
      const e = proc.snapshot.edges[i];
      allEdges.push({
        id: `e${i}-${e.src}-${e.dst}-${e.kind}`,
        source: e.src,
        target: e.dst,
        kind: e.kind,
      });

      if (e.kind === "holds") {
        const lockEntity = lockEntitiesById.get(e.src);
        if (lockEntity) {
          lockEntity.status = { label: "locked", tone: "warn" };
          const holder = entityById.get(e.dst);
          if (holder) lockEntity.holderName = holder.name;
        }
      }
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

// ── Event extraction ───────────────────────────────────────────

export type EventDef = {
  id: string;
  processId: string;
  atPtime: number;
  atApproxUnixMs: number;
  /** Raw EventKind from the wire (string or object). */
  kind: EventKind;
  /** Normalized string key for filtering/display. */
  kindKey: string;
  /** Human-readable display name for this event kind. */
  kindDisplayName: string;
  targetId: string;
  targetKind: "entity" | "scope";
  targetName: string;
  targetEntityKind?: string;
  targetRemoved: boolean;
  backtraceId: number;
  source: RenderSource;
};

const EVENT_KIND_DISPLAY: Record<string, string> = {
  state_changed: "State Changed",
  channel_sent: "Channel Sent",
  channel_received: "Channel Received",
};

export function eventKindKey(kind: EventKind): string {
  if (typeof kind === "object" && "custom" in kind) {
    return `custom:${kind.custom.kind}`;
  }
  return kind;
}

export function eventKindDisplayName(kind: EventKind): string {
  if (typeof kind === "object" && "custom" in kind) {
    return kind.custom.display_name;
  }
  return EVENT_KIND_DISPLAY[kind] ?? kind;
}

export function extractEvents(snapshot: SnapshotCutResponse): EventDef[] {
  const backtraces = buildBacktraceIndex(snapshot);
  const result: EventDef[] = [];

  for (const proc of snapshot.processes) {
    const { process_id, ptime_now_ms } = proc;
    const processIdStr = String(process_id);
    const anchorUnixMs = snapshot.captured_at_unix_ms - ptime_now_ms;

    // Build entity/scope lookup maps for this process.
    const entityMap = new Map<string, { name: string; kind: string; removedAt?: number }>();
    for (const e of proc.snapshot.entities) {
      entityMap.set(e.id, { name: e.name, kind: bodyToKind(e.body), removedAt: e.removed_at });
    }
    const scopeMap = new Map<string, string>();
    for (const s of proc.snapshot.scopes) {
      scopeMap.set(s.id, s.name);
    }

    for (const event of proc.snapshot.events) {
      const backtraceId = requireBacktraceId(event, `event ${event.id}`, String(process_id));
      const resolved = resolveBacktraceDisplay(backtraces, backtraceId, `event ${event.id}`);

      let targetId: string;
      let targetKind: "entity" | "scope";
      let targetName: string;
      let targetEntityKind: string | undefined;
      let targetRemoved = false;

      if ("entity" in event.target) {
        targetId = event.target.entity;
        targetKind = "entity";
        const info = entityMap.get(targetId);
        targetName = info?.name ?? targetId;
        targetEntityKind = info?.kind;
        targetRemoved = info?.removedAt != null;
      } else {
        targetId = event.target.scope;
        targetKind = "scope";
        targetName = scopeMap.get(targetId) ?? targetId;
      }

      const kindKey = eventKindKey(event.kind);
      result.push({
        id: `${processIdStr}:${event.id}`,
        processId: processIdStr,
        atPtime: event.at,
        atApproxUnixMs: anchorUnixMs + event.at,
        kind: event.kind,
        kindKey,
        kindDisplayName: eventKindDisplayName(event.kind),
        targetId,
        targetKind,
        targetName,
        targetEntityKind,
        targetRemoved,
        backtraceId,
        source: resolved.source,
      });
    }
  }

  // Newest first.
  result.sort((a, b) => b.atApproxUnixMs - a.atApproxUnixMs);
  // Cap at 1024 events.
  if (result.length > 1024) result.length = 1024;
  return result;
}

// f[impl filter.control.focus]
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

// f[impl filter.control.loners]
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

const pairKey = (a: string, b: string): string => (a < b ? `${a}<->${b}` : `${b}<->${a}`);

// f[impl graph.collapse] f[impl graph.collapse.id] f[impl graph.collapse.no-dup]
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
              kind: "polls",
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
