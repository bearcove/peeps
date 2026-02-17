import { useEffect, useMemo, useState } from "react";
import {
  MagnifyingGlass,
  CaretLeft,
  CaretRight,
  Tag,
  Hash,
  LinkSimple,
  Key,
  Hourglass,
  Users,
  CheckCircle,
  XCircle,
  CircleNotch,
  Timer,
  Lock,
  LockOpen,
  ArrowLineUp,
  ArrowLineDown,
  Gauge,
  Eye,
  PaperPlaneTilt,
  ArrowBendDownLeft,
  Warning,
  Crosshair,
  CopySimple,
  Check,
  ArrowSquareOut,
  Ghost,
  ArrowRight,
  GitFork,
} from "@phosphor-icons/react";
import { fetchTimelinePage } from "../api";
import { isResourceKind } from "../resourceKinds";
import { kindIcon } from "../nodeKindSpec";
import {
  CommonInspectorFields,
  formatRelativeTimestampFromOrigin,
  getCorrelation,
  getCreatedAtNs,
  getMethod,
  type InspectorProcessAction,
  getSource,
  resolveTimelineOriginNs,
} from "./inspectorShared";
import { DurationDisplay } from "../ui/primitives/DurationDisplay";
import { KeyValueRow } from "../ui/primitives/KeyValueRow";
import type {
  InspectorSnapshotNode,
  StuckRequest,
  SnapshotNode,
  SnapshotEdge,
  SnapshotGraph,
  TimelineCursor,
  TimelineRow,
} from "../types";

interface InspectorProps {
  snapshotId: number | null;
  snapshotCapturedAtNs: number | null;
  selectedRequest: StuckRequest | null;
  selectedNode: SnapshotNode | null;
  selectedEdge: SnapshotEdge | null;
  graph: SnapshotGraph | null;
  filteredNodeId: string | null;
  onFocusNode: (nodeId: string | null) => void;
  onSelectNode: (nodeId: string) => void;
  onProcessAction?: (action: InspectorProcessAction, process: string) => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
}

function durationTone(
  ns: number,
  warnNs: number,
  critNs: number,
): "ok" | "warn" | "crit" {
  if (ns >= critNs) return "crit";
  if (ns >= warnNs) return "warn";
  return "ok";
}

function attr(attrs: Record<string, unknown>, key: string): string | undefined {
  const v = attrs[key];
  if (v == null) return undefined;
  return String(v);
}

function firstAttr(attrs: Record<string, unknown>, keys: string[]): string | undefined {
  for (const k of keys) {
    const v = attrs[k];
    if (v != null && v !== "") return String(v);
  }
  return undefined;
}

function numAttr(attrs: Record<string, unknown>, key: string): number | undefined {
  const v = attrs[key];
  if (v == null) return undefined;
  const n = Number(v);
  return isNaN(n) ? undefined : n;
}

function firstNumAttr(attrs: Record<string, unknown>, keys: string[]): number | undefined {
  for (const k of keys) {
    const v = attrs[k];
    if (v == null || v === "") continue;
    const n = Number(v);
    if (!isNaN(n)) return n;
  }
  return undefined;
}

function rpcConnectionAttr(attrs: Record<string, unknown>): string | undefined {
  return firstAttr(attrs, ["connection", "rpc.connection"]);
}

function elapsedBetween(startNs?: number, endNs?: number): number | undefined {
  if (startNs == null || endNs == null) return undefined;
  if (!Number.isFinite(startNs) || !Number.isFinite(endNs)) return undefined;
  if (endNs < startNs) return undefined;
  return endNs - startNs;
}

function requestElapsedNs(attrs: Record<string, unknown>, status: string): number | undefined {
  const snapshotAtNs = firstNumAttr(attrs, ["_ui_snapshot_captured_at_ns"]);
  const queuedAtNs = firstNumAttr(attrs, ["queued_at_ns"]);
  const startedAtNs = getCreatedAtNs(attrs);
  const completedAtNs = firstNumAttr(attrs, ["completed_at_ns"]);
  const elapsedNs = firstNumAttr(attrs, ["elapsed_ns"]);

  const startNs = status === "queued" ? (queuedAtNs ?? startedAtNs) : (startedAtNs ?? queuedAtNs);
  const completedElapsed = elapsedBetween(startNs, completedAtNs);
  if (status === "completed" && completedElapsed != null) return completedElapsed;

  const inFlightElapsed = elapsedBetween(startNs, snapshotAtNs);
  if (inFlightElapsed != null) return inFlightElapsed;

  return elapsedNs;
}

function responseTiming(attrs: Record<string, unknown>, status: string): {
  elapsedNs?: number;
  handledElapsedNs?: number;
  queueWaitNs?: number;
} {
  const snapshotAtNs = firstNumAttr(attrs, ["_ui_snapshot_captured_at_ns"]);
  const startedAtNs = getCreatedAtNs(attrs);
  const handledAtNs = firstNumAttr(attrs, ["handled_at_ns"]);
  const deliveredAtNs = firstNumAttr(attrs, ["delivered_at_ns"]);
  const cancelledAtNs = firstNumAttr(attrs, ["cancelled_at_ns"]);
  const elapsedNs = firstNumAttr(attrs, ["elapsed_ns"]);
  const handledElapsedNs = firstNumAttr(attrs, ["handled_elapsed_ns"]);

  let endAtNs = deliveredAtNs;
  if (endAtNs == null && status === "cancelled") {
    endAtNs = cancelledAtNs;
  }
  if (endAtNs == null) {
    endAtNs = snapshotAtNs;
  }

  const computedElapsedNs = elapsedBetween(startedAtNs, endAtNs) ?? elapsedNs;
  const computedHandledElapsedNs = elapsedBetween(startedAtNs, handledAtNs) ?? handledElapsedNs;
  const queueWaitNs = elapsedBetween(handledAtNs, deliveredAtNs ?? snapshotAtNs);
  return { elapsedNs: computedElapsedNs, handledElapsedNs: computedHandledElapsedNs, queueWaitNs };
}

export function Inspector({
  snapshotId,
  snapshotCapturedAtNs,
  selectedRequest,
  selectedNode,
  selectedEdge,
  graph,
  filteredNodeId,
  onFocusNode,
  onSelectNode,
  onProcessAction,
  collapsed,
  onToggleCollapse,
}: InspectorProps) {
  if (collapsed) {
    return (
      <div className="panel panel--collapsed">
        <button className="panel-collapse-btn" onClick={onToggleCollapse} title="Expand panel">
          <CaretLeft size={14} weight="bold" />
        </button>
        <span className="panel-collapsed-label">Inspector</span>
      </div>
    );
  }

  return (
    <div className="panel">
      <div className="panel-header">
        <MagnifyingGlass size={14} weight="bold" /> Inspector
        <button className="panel-collapse-btn" onClick={onToggleCollapse} title="Collapse panel">
          <CaretRight size={14} weight="bold" />
        </button>
      </div>
      <div className="inspector">
        {selectedRequest ? (
          <RequestDetail req={selectedRequest} graph={graph} onSelectNode={onSelectNode} />
        ) : selectedEdge ? (
          <EdgeDetail edge={selectedEdge} graph={graph} onSelectNode={onSelectNode} />
        ) : selectedNode ? (
          selectedNode.kind === "ghost" ? (
            <GhostDetail node={selectedNode} graph={graph} />
          ) : (
            <NodeDetail
              snapshotId={snapshotId}
              snapshotCapturedAtNs={snapshotCapturedAtNs}
              node={toInspectorSnapshotNode(selectedNode)}
              graph={graph}
              filteredNodeId={filteredNodeId}
              onFocusNode={onFocusNode}
              onSelectNode={onSelectNode}
              onProcessAction={onProcessAction}
            />
          )
        ) : (
          <div className="inspector-empty">
            Select a request, graph node, or edge to inspect.
            <br />
            <br />
            Keyboard: arrows to navigate, enter to select, esc to deselect.
          </div>
        )}
      </div>
    </div>
  );
}

function toInspectorSnapshotNode(node: SnapshotNode): InspectorSnapshotNode {
  const createdAtNs = getCreatedAtNs(node.attrs);
  const source = getSource(node.attrs);
  if (createdAtNs == null || source == null) {
    throw new Error(
      `Inspector canonical attrs missing for node ${node.id}: requires created_at and source`,
    );
  }
  return node as InspectorSnapshotNode;
}

function RequestDetail({
  req,
  graph,
  onSelectNode,
}: {
  req: StuckRequest;
  graph: SnapshotGraph | null;
  onSelectNode: (nodeId: string) => void;
}) {
  return (
    <div className="inspect-section">
      <KeyValueRow label="Method">{req.method ?? "unknown"}</KeyValueRow>
      <KeyValueRow label="Process">{req.process}</KeyValueRow>
      <KeyValueRow label="Elapsed">
        <DurationDisplay ms={req.elapsed_ns / 1_000_000} />
      </KeyValueRow>
      <KeyValueRow label="Connection">
        {req.connection ? (
          <RelatedConnectionLink
            connection={req.connection}
            graph={graph}
            onSelectNode={onSelectNode}
          />
        ) : (
          "—"
        )}
      </KeyValueRow>
    </div>
  );
}

function nodeLabel(graph: SnapshotGraph | null, nodeId: string): string {
  if (!graph) return nodeId;
  const node = graph.nodes.find((n) => n.id === nodeId);
  if (!node) return nodeId;
  return firstAttr(node.attrs, ["label", "method", "name"]) ?? nodeId;
}

function resourceNodeLabel(node: SnapshotNode): string {
  return (
    firstAttr(node.attrs, ["label", "name", "connection.id", "rpc.connection", "method"]) ?? node.id
  );
}

function EdgeDetail({
  edge,
  graph,
  onSelectNode,
}: {
  edge: SnapshotEdge;
  graph: SnapshotGraph | null;
  onSelectNode: (nodeId: string) => void;
}) {
  const srcLabel = nodeLabel(graph, edge.src_id);
  const dstLabel = nodeLabel(graph, edge.dst_id);
  const srcNode = graph?.nodes.find((n) => n.id === edge.src_id);
  const dstNode = graph?.nodes.find((n) => n.id === edge.dst_id);

  const kindVariant =
    edge.kind === "needs" ? "crit" : edge.kind === "spawned" ? "ok" : "neutral";

  return (
    <div className="inspect-node">
      <div className="inspect-node-header">
        <span className="inspect-node-icon">
          {edge.kind === "spawned" ? (
            <GitFork size={16} weight="bold" />
          ) : (
            <ArrowRight size={16} weight="bold" />
          )}
        </span>
        <div>
          <div className="inspect-node-kind">edge</div>
          <div className="inspect-node-label">{edge.kind}</div>
        </div>
      </div>

      <div className="inspect-section">
        <KeyValueRow label="Kind">
          <span className={`inspect-pill inspect-pill--${kindVariant}`}>{edge.kind.toUpperCase()}</span>
        </KeyValueRow>
      </div>

      <div className="inspect-section">
        <div className="inspect-edge-endpoint">
          <span className="inspect-edge-label">Source</span>
          <button
            className="inspect-edge-node-btn"
            onClick={() => onSelectNode(edge.src_id)}
            title={edge.src_id}
          >
            {srcNode && (
              <span className="inspect-edge-node-kind">{srcNode.kind}</span>
            )}
            <span className="inspect-edge-node-name">{srcLabel}</span>
          </button>
          <span className="inspect-edge-id">{edge.src_id}</span>
        </div>

        <div className="inspect-edge-arrow">
          <ArrowRight size={14} weight="bold" />
        </div>

        <div className="inspect-edge-endpoint">
          <span className="inspect-edge-label">Target</span>
          <button
            className="inspect-edge-node-btn"
            onClick={() => onSelectNode(edge.dst_id)}
            title={edge.dst_id}
          >
            {dstNode && (
              <span className="inspect-edge-node-kind">{dstNode.kind}</span>
            )}
            <span className="inspect-edge-node-name">{dstLabel}</span>
          </button>
          <span className="inspect-edge-id">{edge.dst_id}</span>
        </div>
      </div>
    </div>
  );
}

function GhostDetail({ node, graph }: { node: SnapshotNode; graph: SnapshotGraph | null }) {
  const reason = attr(node.attrs, "reason") ?? "unresolved";

  // Count incoming/outgoing edges
  let incoming = 0;
  let outgoing = 0;
  if (graph) {
    for (const e of graph.edges) {
      if (e.dst_id === node.id) incoming++;
      if (e.src_id === node.id) outgoing++;
    }
  }

  return (
    <div className="inspect-node">
      <div className="inspect-node-header">
        <span className="inspect-node-icon inspect-node-icon--ghost">
          <Ghost size={16} weight="bold" />
        </span>
        <div>
          <div className="inspect-node-kind">ghost (unresolved)</div>
          <div className="inspect-node-label">{node.id}</div>
        </div>
      </div>

      <div className="inspect-alert inspect-alert--ghost">
        This node does not exist in the current snapshot. It appears as an endpoint
        of an unresolved edge.
      </div>

      <div className="inspect-section">
        <KeyValueRow label="ID">
          <span className="inspect-val inspect-val--copyable">
            <span className="inspect-val-copy-text" title={node.id}>
              {node.id}
            </span>
            <CopyIdButton id={node.id} />
          </span>
        </KeyValueRow>
        <KeyValueRow label="Reason">
          <span className="inspect-pill inspect-pill--neutral">{reason}</span>
        </KeyValueRow>
        <KeyValueRow label="Incoming">
          <span className="inspect-val">{incoming}</span>
        </KeyValueRow>
        <KeyValueRow label="Outgoing">
          <span className="inspect-val">{outgoing}</span>
        </KeyValueRow>
      </div>
    </div>
  );
}

function CopyIdButton({ id }: { id: string }) {
  const [copied, setCopied] = useState(false);

  async function onCopy() {
    try {
      await navigator.clipboard.writeText(id);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      setCopied(false);
    }
  }

  return (
    <button
      type="button"
      className="inspect-copy-btn"
      onClick={onCopy}
      title={copied ? "Copied" : "Copy ID"}
      aria-label="Copy node ID"
    >
      {copied ? <Check size={12} weight="bold" /> : <CopySimple size={12} weight="bold" />}
    </button>
  );
}

function CopyTextButton({ text, title = "Copy value" }: { text: string; title?: string }) {
  const [copied, setCopied] = useState(false);

  async function onCopy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      setCopied(false);
    }
  }

  return (
    <button
      type="button"
      className="inspect-copy-btn"
      onClick={onCopy}
      title={copied ? "Copied" : title}
      aria-label={title}
    >
      {copied ? <Check size={12} weight="bold" /> : <CopySimple size={12} weight="bold" />}
    </button>
  );
}

const TIMELINE_NODE_KINDS = new Set(["request", "response", "tx", "rx", "remote_tx", "remote_rx"]);
const TIMELINE_PAGE_SIZE = 25;

function formatTimelineTimestamp(tsNs: number): string {
  if (!Number.isFinite(tsNs)) return "—";
  const date = new Date(Math.floor(tsNs / 1_000_000));
  const micros = Math.floor((tsNs % 1_000_000) / 1_000);
  return `${date.toLocaleTimeString()}.${
    String(micros).padStart(3, "0")
  }`;
}

function compactPreviewValue(val: unknown): string {
  if (val == null) return "null";
  if (typeof val === "string") return val.length > 48 ? `${val.slice(0, 48)}…` : val;
  if (typeof val === "number" || typeof val === "boolean") return String(val);
  const serialized = JSON.stringify(val);
  if (!serialized) return "null";
  return serialized.length > 48 ? `${serialized.slice(0, 48)}…` : serialized;
}

function compactAttrsPreview(attrs: Record<string, unknown>): string {
  const entries = Object.entries(attrs).filter(([, val]) => val != null).slice(0, 3);
  if (entries.length === 0) return "—";
  return entries.map(([key, val]) => `${key}=${compactPreviewValue(val)}`).join("  ");
}

function NodeDetail({
  snapshotId,
  snapshotCapturedAtNs,
  node,
  graph,
  filteredNodeId,
  onFocusNode,
  onSelectNode,
  onProcessAction,
}: {
  snapshotId: number | null;
  snapshotCapturedAtNs: number | null;
  node: InspectorSnapshotNode;
  graph: SnapshotGraph | null;
  filteredNodeId: string | null;
  onFocusNode: (nodeId: string | null) => void;
  onSelectNode: (nodeId: string) => void;
  onProcessAction?: (action: InspectorProcessAction, process: string) => void;
}) {
  const channelKind =
    node.kind === "tx" ||
    node.kind === "rx" ||
    node.kind.endsWith("_tx") ||
    node.kind.endsWith("_rx")
      ? firstAttr(node.attrs, ["channel_kind", "channel.kind", "channel_type", "channel.type"])
      : undefined;
  const icon =
    (node.kind === "tx" || node.kind.endsWith("_tx")) && channelKind === "watch" ? (
      <Eye size={16} weight="bold" />
    ) : (node.kind === "rx" || node.kind.endsWith("_rx")) && channelKind === "watch" ? (
      <Eye size={16} weight="bold" />
    ) : (node.kind === "tx" || node.kind.endsWith("_tx")) && channelKind === "oneshot" ? (
      kindIcon("oneshot", 16)
    ) : (node.kind === "rx" || node.kind.endsWith("_rx")) && channelKind === "oneshot" ? (
      kindIcon("oneshot", 16)
    ) : node.kind === "tx" || node.kind.endsWith("_tx") ? (
      <ArrowLineUp size={16} weight="bold" />
    ) : node.kind === "rx" || node.kind.endsWith("_rx") ? (
      <ArrowLineDown size={16} weight="bold" />
    ) : (
      kindIcon(node.kind, 16)
    );
  const DetailComponent = kindDetailMap[node.kind];
  const isFocused = filteredNodeId === node.id;
  const method = getMethod(node.attrs);
  const correlationKey = getCorrelation(node.attrs);
  const deadlockReason = attr(node.attrs, "_ui_deadlock_reason");
  const blockers =
    graph?.edges
      .filter((e) => e.kind === "needs" && e.src_id === node.id)
      .map((e) => e.dst_id) ?? [];
  const dependents =
    graph?.edges
      .filter((e) => e.kind === "needs" && e.dst_id === node.id)
      .map((e) => e.src_id) ?? [];
  const uniqueBlockers = Array.from(new Set(blockers));
  const uniqueDependents = Array.from(new Set(dependents));
  const supportsTimeline = TIMELINE_NODE_KINDS.has(node.kind);
  const relatedResources = useMemo(() => {
    if (isResourceKind(node.kind) || !graph) return [] as SnapshotNode[];

    const nodeById = new Map(graph.nodes.map((n) => [n.id, n]));
    const relatedIds = new Set<string>();
    for (const edge of graph.edges) {
      if (edge.kind !== "touches") continue;
      let otherId: string | null = null;
      if (edge.src_id === node.id) otherId = edge.dst_id;
      else if (edge.dst_id === node.id) otherId = edge.src_id;
      if (!otherId) continue;

      const other = nodeById.get(otherId);
      if (!other || !isResourceKind(other.kind)) continue;
      relatedIds.add(otherId);
    }

    return Array.from(relatedIds)
      .map((id) => nodeById.get(id))
      .filter((n): n is SnapshotNode => n != null)
      .sort((a, b) => {
        const byKind = a.kind.localeCompare(b.kind);
        if (byKind !== 0) return byKind;
        return a.id.localeCompare(b.id);
      });
  }, [graph, node.id, node.kind]);
  const [timelineRows, setTimelineRows] = useState<TimelineRow[]>([]);
  const [timelineCursor, setTimelineCursor] = useState<TimelineCursor | null>(null);
  const [timelineLoading, setTimelineLoading] = useState(false);
  const [timelineLoadingOlder, setTimelineLoadingOlder] = useState(false);
  const [timelineError, setTimelineError] = useState<string | null>(null);

  useEffect(() => {
    if (!supportsTimeline || snapshotId == null || snapshotCapturedAtNs == null) {
      setTimelineRows([]);
      setTimelineCursor(null);
      setTimelineLoading(false);
      setTimelineLoadingOlder(false);
      setTimelineError(null);
      return;
    }

    let cancelled = false;
    setTimelineLoading(true);
    setTimelineLoadingOlder(false);
    setTimelineError(null);
    setTimelineRows([]);
    setTimelineCursor(null);

    fetchTimelinePage(snapshotId, {
      procKey: node.proc_key,
      entityId: node.id,
      capturedAtNs: snapshotCapturedAtNs,
      limit: TIMELINE_PAGE_SIZE,
      cursor: null,
    })
      .then((page) => {
        if (cancelled) return;
        setTimelineRows(page.rows);
        setTimelineCursor(page.nextCursor);
      })
      .catch((err) => {
        if (cancelled) return;
        setTimelineError(err instanceof Error ? err.message : "Failed to load timeline");
      })
      .finally(() => {
        if (cancelled) return;
        setTimelineLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [node.id, node.proc_key, snapshotCapturedAtNs, snapshotId, supportsTimeline]);

  async function loadOlderTimeline() {
    if (
      !supportsTimeline ||
      timelineLoading ||
      timelineLoadingOlder ||
      snapshotId == null ||
      snapshotCapturedAtNs == null ||
      timelineCursor == null
    ) {
      return;
    }

    setTimelineLoadingOlder(true);
    setTimelineError(null);
    try {
      const page = await fetchTimelinePage(snapshotId, {
        procKey: node.proc_key,
        entityId: node.id,
        capturedAtNs: snapshotCapturedAtNs,
        limit: TIMELINE_PAGE_SIZE,
        cursor: timelineCursor,
      });
      setTimelineRows((prev) => [...prev, ...page.rows]);
      setTimelineCursor(page.nextCursor);
    } catch (err) {
      setTimelineError(err instanceof Error ? err.message : "Failed to load older timeline rows");
    } finally {
      setTimelineLoadingOlder(false);
    }
  }

  const timelineFirstEventTsNs =
    timelineRows.length > 0 ? Math.min(...timelineRows.map((row) => row.ts_ns)) : null;
  const timelineOriginNs = resolveTimelineOriginNs(node.attrs, timelineFirstEventTsNs);

  return (
    <div className="inspect-node">
      <div className="inspect-node-header">
        {icon && <span className="inspect-node-icon">{icon}</span>}
        <div>
          <div className="inspect-node-kind">{node.kind}</div>
          <div className="inspect-node-label">
            {(() => {
              if (node.kind === "request") {
                return (
                  method ??
                  firstAttr(node.attrs, ["label", "name"]) ??
                  node.id
                );
              }
              if (node.kind === "response") {
                return (
                  method ??
                  firstAttr(node.attrs, ["label", "name"]) ??
                  correlationKey ??
                  node.id
                );
              }
              return firstAttr(node.attrs, ["label", "name"]) ?? method ?? node.id;
            })()}
          </div>
        </div>
        <button
          className="filter-clear-btn"
          onClick={() => onFocusNode(node.id)}
          title="Filter graph to the subgraph connected to this node"
          disabled={isFocused}
          style={isFocused ? { opacity: 0.6, cursor: "default" } : undefined}
        >
          <Crosshair size={12} weight="bold" />
          {isFocused ? "focused" : "focus"}
        </button>
      </div>

      {deadlockReason && (
        <div className="inspect-alert inspect-alert--crit">
          Suspect deadlock signal: <code>{deadlockReason}</code>
        </div>
      )}

      <CommonInspectorFields id={node.id} process={node.process} attrs={node.attrs} onProcessAction={onProcessAction} />

      {(uniqueBlockers.length > 0 || uniqueDependents.length > 0) && (
        <div className="inspect-section">
          <KeyValueRow label="Wait blockers">
            <span
              className={`inspect-val ${uniqueBlockers.length > 0 ? "inspect-val--crit" : ""}`}
            >
              {uniqueBlockers.length}
            </span>
          </KeyValueRow>
          {uniqueBlockers.slice(0, 8).map((id) => (
            <KeyValueRow label="waits on" key={`blk:${id}`}>
              <div className="inspect-edge-target-row">
                <button className="inspect-edge-node-btn" onClick={() => onSelectNode(id)} title={id}>
                  {nodeLabel(graph, id)}
                </button>
                <button
                  type="button"
                  className="inspect-edge-focus-btn"
                  onClick={() => {
                    onSelectNode(id);
                    onFocusNode(id);
                  }}
                  title="Focus this node in graph"
                  aria-label="Focus this node in graph"
                >
                  <Crosshair size={12} weight="bold" />
                  focus
                </button>
              </div>
            </KeyValueRow>
          ))}
          <KeyValueRow label="Dependents">
            <span
              className={`inspect-val ${uniqueDependents.length > 0 ? "inspect-val--warn" : ""}`}
            >
              {uniqueDependents.length}
            </span>
          </KeyValueRow>
          {uniqueDependents.slice(0, 8).map((id) => (
            <KeyValueRow label="blocking" key={`dep:${id}`}>
              <div className="inspect-edge-target-row">
                <button className="inspect-edge-node-btn" onClick={() => onSelectNode(id)} title={id}>
                  {nodeLabel(graph, id)}
                </button>
                <button
                  type="button"
                  className="inspect-edge-focus-btn"
                  onClick={() => {
                    onSelectNode(id);
                    onFocusNode(id);
                  }}
                  title="Focus this node in graph"
                  aria-label="Focus this node in graph"
                >
                  <Crosshair size={12} weight="bold" />
                  focus
                </button>
              </div>
            </KeyValueRow>
          ))}
        </div>
      )}

      {!isResourceKind(node.kind) && (
        <div className="inspect-section">
          <KeyValueRow label="Related resources">
            <span
              className={`inspect-val ${relatedResources.length > 0 ? "inspect-val--ok" : ""}`}
            >
              {relatedResources.length}
            </span>
          </KeyValueRow>
          {relatedResources.length === 0 ? (
            <div className="inspect-alert inspect-alert--ghost">No related resources.</div>
          ) : (
            relatedResources.map((resource) => (
              <KeyValueRow label={resource.kind} key={`res:${resource.id}`}>
                <button
                  className="inspect-edge-node-btn"
                  onClick={() => onSelectNode(resource.id)}
                  title={resource.id}
                >
                  {resource.kind} · {resourceNodeLabel(resource)}
                </button>
              </KeyValueRow>
            ))
          )}
        </div>
      )}

      {DetailComponent && (
        <div className="inspect-section">
          <DetailComponent attrs={node.attrs} graph={graph} onSelectNode={onSelectNode} />
        </div>
      )}

      {supportsTimeline && (
        <div className="inspect-section inspect-timeline">
          <div className="inspect-raw-head">
            <div className="inspect-raw-title">Timeline</div>
          </div>
          {timelineLoading && timelineRows.length === 0 && (
            <div className="inspect-alert inspect-alert--ghost">Loading timeline…</div>
          )}
          {!timelineLoading && timelineRows.length === 0 && !timelineError && (
            <div className="inspect-alert inspect-alert--ghost">No timeline rows for this node.</div>
          )}
          {timelineError && (
            <div className="inspect-alert inspect-alert--crit">
              Timeline query failed: <code>{timelineError}</code>
            </div>
          )}
          {timelineRows.length > 0 && (
            <div className="inspect-timeline-list">
              {timelineRows.map((row) => (
                <div className="inspect-timeline-item" key={`${row.ts_ns}:${row.id}`}>
                  <div className="inspect-timeline-top">
                    <span
                      className="inspect-timeline-ts"
                      title={`at ${formatTimelineTimestamp(row.ts_ns)}${
                        timelineOriginNs != null
                          ? `\nfrom node start: ${formatRelativeTimestampFromOrigin(row.ts_ns, timelineOriginNs)}`
                          : ""
                      }`}
                    >
                      {formatRelativeTimestampFromOrigin(row.ts_ns, timelineOriginNs)}
                    </span>
                    <span
                      className={`inspect-pill inspect-pill--${
                        row.relation === "self"
                          ? "ok"
                          : row.relation === "parent"
                            ? "warn"
                            : "neutral"
                      }`}
                    >
                      {row.relation}
                    </span>
                  </div>
                  <div className="inspect-timeline-name">{row.name}</div>
                  <div className="inspect-timeline-attrs">{compactAttrsPreview(row.attrs)}</div>
                </div>
              ))}
            </div>
          )}
          {timelineCursor && (
            <button
              type="button"
              className="inspect-timeline-more"
              onClick={loadOlderTimeline}
              disabled={timelineLoading || timelineLoadingOlder}
            >
              {timelineLoadingOlder ? "Loading…" : "Load older"}
            </button>
          )}
        </div>
      )}

      <RawAttrs attrs={node.attrs} />
    </div>
  );
}

function RawAttrs({ attrs }: { attrs: Record<string, unknown> }) {
  const [expanded, setExpanded] = useState(false);
  const entries = Object.entries(attrs).filter(([k, v]) => v != null && !k.startsWith("_ui_"));
  if (entries.length === 0) return null;
  const rawJson = useMemo(() => JSON.stringify(Object.fromEntries(entries), null, 2), [entries]);

  function asFiniteNumber(v: unknown): number | null {
    if (typeof v === "number" && Number.isFinite(v)) return v;
    if (typeof v === "string" && /^[0-9]+$/.test(v) && v.length <= 15) {
      const n = Number(v);
      return Number.isFinite(n) ? n : null;
    }
    return null;
  }

  function formatValue(key: string, val: unknown): React.ReactNode {
    if (typeof val === "boolean") {
      return (
        <span className={`inspect-pill inspect-pill--${val ? "ok" : "neutral"}`}>
          {String(val)}
        </span>
      );
    }

    const k = key.toLowerCase();
    const maybeNs = asFiniteNumber(val);
    if (maybeNs != null && (k.endsWith("_ns") || k.includes("duration") || k.includes("age"))) {
      return <DurationDisplay ms={maybeNs / 1_000_000} />;
    }

    if (typeof val === "number" && Number.isFinite(val)) {
      return val.toLocaleString();
    }

    if (typeof val === "object") {
      return JSON.stringify(val);
    }

    return String(val);
  }

  function iconForKey(key: string, val: unknown): React.ReactNode | null {
    // Exact matches first.
    switch (key) {
      case "name":
      case "label":
        return <Tag size={12} weight="bold" />;
    }

    // Heuristic mapping by convention (keys are stable, values vary).
    const k = key.toLowerCase();
    if (k.endsWith(".id")) return <Hash size={12} weight="bold" />;
    if (k.includes("connection")) return <LinkSimple size={12} weight="bold" />;

    if (k.includes("created_at") || k.includes("age") || k.includes("duration"))
      return <Timer size={12} weight="bold" />;

    if (k.includes("lock_kind")) return <Lock size={12} weight="bold" />;
    if (k.includes("waiter")) return <Hourglass size={12} weight="bold" />;
    if (k.includes("holder") || k.includes("sender_count") || k.includes("receiver_count"))
      return <Users size={12} weight="bold" />;

    if (k.includes("sent") || k.includes("send")) return <ArrowLineUp size={12} weight="bold" />;
    if (k.includes("recv") || k.includes("received"))
      return <ArrowLineDown size={12} weight="bold" />;

    if (
      k.includes("capacity") ||
      k.includes("bounded") ||
      k.includes("queue") ||
      k.includes("high_watermark") ||
      k.includes("utilization")
    ) {
      return <Gauge size={12} weight="bold" />;
    }

    if (k === "state" || k.includes(".state")) return <CircleNotch size={12} weight="bold" />;
    if (k === "closed") return <XCircle size={12} weight="bold" />;
    if (k === "ready_count") return <CheckCircle size={12} weight="bold" />;
    if (k === "pending_count") return <CircleNotch size={12} weight="bold" />;

    // Special-case value-dependent "closed" fields where the key isn't literally "closed".
    if (k.includes("closed") && typeof val === "boolean")
      return <XCircle size={12} weight="bold" />;

    return null;
  }

  return (
    <div className="inspect-raw">
      <div className="inspect-raw-head">
        <div className="inspect-raw-title">All attributes ({entries.length})</div>
        <CopyTextButton text={rawJson} title="Copy all attributes" />
        <button
          className="panel-expand-btn"
          type="button"
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded ? "Hide raw" : "Show raw"}
        </button>
      </div>
      {expanded && (
        <dl>
          {entries.map(([key, val]) => (
            <div key={key}>
              <dt>
                <span className="inspect-raw-key-icon">{iconForKey(key, val)}</span>
                <span className="inspect-raw-key-text">{key}</span>
              </dt>
              <dd>{formatValue(key, val)}</dd>
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}

// ── Kind-specific detail sections ────────────────────────────

type DetailProps = {
  attrs: Record<string, unknown>;
  graph: SnapshotGraph | null;
  onSelectNode: (nodeId: string) => void;
};

function connectionNodeId(connection: string): string {
  return `connection:${connection}`;
}

function RelatedConnectionLink({
  connection,
  graph,
  onSelectNode,
}: {
  connection: string;
  graph: SnapshotGraph | null;
  onSelectNode: (nodeId: string) => void;
}) {
  const connectionId = connectionNodeId(connection);
  const hasNode = graph?.nodes.some((node) => node.id === connectionId) ?? false;

  if (!hasNode) {
    return <span className="inspect-val inspect-val--mono">{connection}</span>;
  }

  return (
    <button
      type="button"
      className="inspect-meta-val inspect-val--mono inspect-link"
      onClick={() => onSelectNode(connectionId)}
      title={`Select ${connectionId}`}
    >
      <ArrowSquareOut size={12} weight="bold" className="inspect-link-icon" />
      {connection}
    </button>
  );
}

function FutureDetail({ attrs }: DetailProps) {
  const state =
    attr(attrs, "state") ??
    ((firstNumAttr(attrs, ["poll_in_flight_ns", "in_poll_ns", "current_poll_ns"]) ?? 0) > 0
      ? "polling"
      : "waiting");
  const pendingCount = numAttr(attrs, "pending_count") ?? 0;
  const readyCount = numAttr(attrs, "ready_count") ?? 0;
  const pollCount = firstNumAttr(attrs, ["poll_count"]) ?? pendingCount + readyCount;
  const lastPolledNs = firstNumAttr(attrs, ["last_polled_ns", "idle_ns"]);
  const inPollNs = firstNumAttr(attrs, ["poll_in_flight_ns", "in_poll_ns", "current_poll_ns"]);

  return (
    <>
      {state && (
        <KeyValueRow label="State">
          <span
            className={`inspect-pill inspect-pill--${state === "completed" ? "ok" : state === "polling" ? "ok" : "neutral"}`}
          >
            {state}
          </span>
        </KeyValueRow>
      )}
      {pollCount != null && (
        <KeyValueRow label="Poll count">
          <span className={`inspect-val ${pollCount === 0 ? "inspect-val--crit" : ""}`}>
            {pollCount}
            {pollCount === 0 ? " (never polled!)" : ""}
          </span>
        </KeyValueRow>
      )}
      {lastPolledNs != null && (
        <KeyValueRow label="Idle">
          <span className="inspect-val">
            <DurationDisplay
              ms={lastPolledNs / 1_000_000}
              tone={durationTone(lastPolledNs, 1_000_000_000, 5_000_000_000)}
            />
            {" ago"}
          </span>
        </KeyValueRow>
      )}
      {inPollNs != null && inPollNs > 0 && (
        <KeyValueRow label="In poll">
          <span className="inspect-val">
            <DurationDisplay
              ms={inPollNs / 1_000_000}
              tone={durationTone(inPollNs, 1_000_000_000, 5_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
    </>
  );
}

function MutexDetail({ attrs }: DetailProps) {
  const holder = attr(attrs, "holder");
  const holderCount = firstNumAttr(attrs, ["holder_count"]) ?? (holder ? 1 : 0);
  const waiters = firstNumAttr(attrs, ["waiters", "waiter_count"]) ?? 0;
  const heldNs = numAttr(attrs, "held_ns");
  const longestWaitNs = firstNumAttr(attrs, ["longest_wait_ns", "oldest_wait_ns"]);

  return (
    <>
      <KeyValueRow label="State">
        <span className={`inspect-pill inspect-pill--${holder ? "crit" : "ok"}`}>
          {holderCount > 0 ? "HELD" : "FREE"}
        </span>
      </KeyValueRow>
      {!holder && holderCount > 0 && (
        <KeyValueRow label="Holders">
          <span className="inspect-val">{holderCount}</span>
        </KeyValueRow>
      )}
      {holder && (
        <KeyValueRow label="Holder">
          <span className="inspect-val inspect-val--mono">{holder}</span>
        </KeyValueRow>
      )}
      {heldNs != null && (
        <KeyValueRow label="Hold duration">
          <span className="inspect-val">
            <DurationDisplay
              ms={heldNs / 1_000_000}
              tone={durationTone(heldNs, 100_000_000, 1_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
      <KeyValueRow label="Waiters">
        <span className={`inspect-val ${waiters > 0 ? "inspect-val--crit" : ""}`}>{waiters}</span>
      </KeyValueRow>
      {longestWaitNs != null && longestWaitNs > 0 && (
        <KeyValueRow label="Longest wait">
          <span className="inspect-val">
            <DurationDisplay
              ms={longestWaitNs / 1_000_000}
              tone={durationTone(longestWaitNs, 100_000_000, 1_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
    </>
  );
}

function RwLockDetail({ attrs }: DetailProps) {
  const readers = numAttr(attrs, "readers") ?? 0;
  const holder = attr(attrs, "holder");
  const writerWaiters = numAttr(attrs, "writer_waiters") ?? 0;
  const readerWaiters = numAttr(attrs, "reader_waiters") ?? 0;
  const heldNs = numAttr(attrs, "held_ns");

  return (
    <>
      <KeyValueRow label="State">
        <span className={`inspect-pill inspect-pill--${readers > 0 || holder ? "crit" : "ok"}`}>
          {readers > 0 ? `${readers} readers` : holder ? `Write: ${holder}` : "FREE"}
        </span>
      </KeyValueRow>
      {heldNs != null && (
        <KeyValueRow label="Hold duration">
          <span className="inspect-val">
            <DurationDisplay
              ms={heldNs / 1_000_000}
              tone={durationTone(heldNs, 100_000_000, 1_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
      <KeyValueRow label="Waiters">
        <span
          className={`inspect-val ${writerWaiters + readerWaiters > 0 ? "inspect-val--crit" : ""}`}
        >
          {readerWaiters}R + {writerWaiters}W
        </span>
      </KeyValueRow>
    </>
  );
}

function ChannelTxDetail({ attrs }: DetailProps) {
  const buffered = numAttr(attrs, "buffered") ?? 0;
  const capacity = numAttr(attrs, "capacity") ?? 0;
  const senderCount = numAttr(attrs, "sender_count");
  const isFull = capacity > 0 && buffered >= capacity;
  const pct = capacity > 0 ? ((buffered / capacity) * 100).toFixed(0) : "—";

  return (
    <>
      {capacity > 0 && (
        <>
          <KeyValueRow label="Buffer">
            <span className={`inspect-val ${isFull ? "inspect-val--crit" : ""}`}>
              {buffered} / {capacity} ({pct}%)
            </span>
          </KeyValueRow>
          <div className="inspect-bar-wrap">
            <div
              className={`inspect-bar inspect-bar--${isFull ? "crit" : buffered / capacity >= 0.5 ? "warn" : "ok"}`}
            >
              <div
                className="inspect-bar-fill"
                style={{ width: `${Math.min(buffered / capacity, 1) * 100}%` }}
              />
            </div>
          </div>
        </>
      )}
      {isFull && (
        <KeyValueRow label="Capacity">
          <span className="inspect-pill inspect-pill--crit">FULL — senders blocking</span>
        </KeyValueRow>
      )}
      {senderCount != null && (
        <KeyValueRow label="Sender handles">
          <span className="inspect-val">{senderCount}</span>
        </KeyValueRow>
      )}
    </>
  );
}

function ChannelRxDetail({ attrs }: DetailProps) {
  const state = attr(attrs, "state") ?? "idle";
  const receiverAlive = attr(attrs, "receiver_alive");
  const pending = numAttr(attrs, "pending");
  const isAlive = receiverAlive !== "false" && receiverAlive !== "0";

  return (
    <>
      <KeyValueRow label="State">
        <span className={`inspect-pill inspect-pill--${state === "starved" ? "warn" : "neutral"}`}>
          {state}
        </span>
      </KeyValueRow>
      {receiverAlive != null && (
        <KeyValueRow label="Receiver">
          <span className={`inspect-val ${isAlive ? "inspect-val--ok" : "inspect-val--crit"}`}>
            {isAlive ? "alive" : "DEAD"}
          </span>
        </KeyValueRow>
      )}
      {pending != null && (
        <KeyValueRow label="Pending">
          <span className="inspect-val">{pending}</span>
        </KeyValueRow>
      )}
    </>
  );
}

function OneshotDetail({ attrs }: DetailProps) {
  const state = attr(attrs, "state") ?? "pending";
  const elapsedNs = numAttr(attrs, "elapsed_ns");
  const isDropped = state === "dropped";

  const variant = isDropped ? "crit" : state === "sent" || state === "completed" ? "ok" : "neutral";

  return (
    <>
      <KeyValueRow label="State">
        <span className={`inspect-pill inspect-pill--${variant}`}>
          {isDropped && <Warning size={12} weight="bold" />}
          {state.toUpperCase()}
        </span>
      </KeyValueRow>
      {isDropped && (
        <div className="inspect-alert inspect-alert--crit">
          Sender dropped without sending. Receiver will never resolve — potential deadlock.
        </div>
      )}
      {elapsedNs != null && (
        <KeyValueRow label="Elapsed">
          <span className="inspect-val">
            <DurationDisplay
              ms={elapsedNs / 1_000_000}
              tone={durationTone(elapsedNs, 1_000_000_000, 5_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
    </>
  );
}

function WatchDetail({ attrs }: DetailProps) {
  const subscribers = numAttr(attrs, "subscriber_count") ?? numAttr(attrs, "subscribers");
  const senderAlive = attr(attrs, "sender_alive");
  const lastUpdatedNs = numAttr(attrs, "last_updated_ns");
  const isSenderAlive = senderAlive !== "false" && senderAlive !== "0";

  return (
    <>
      {subscribers != null && (
        <KeyValueRow label="Subscribers">
          <span className="inspect-val">{subscribers}</span>
        </KeyValueRow>
      )}
      {senderAlive != null && (
        <KeyValueRow label="Sender">
          <span
            className={`inspect-val ${isSenderAlive ? "inspect-val--ok" : "inspect-val--crit"}`}
          >
            {isSenderAlive ? "alive" : "DROPPED"}
          </span>
        </KeyValueRow>
      )}
      {senderAlive != null && !isSenderAlive && (
        <div className="inspect-alert inspect-alert--crit">
          Sender dropped. All receivers will see stale data forever.
        </div>
      )}
      {lastUpdatedNs != null && (
        <KeyValueRow label="Last updated">
          <span className="inspect-val">
            <DurationDisplay
              ms={lastUpdatedNs / 1_000_000}
              tone={durationTone(lastUpdatedNs, 5_000_000_000, 30_000_000_000)}
            />
            {" ago"}
          </span>
        </KeyValueRow>
      )}
    </>
  );
}

function SemaphoreDetail({ attrs }: DetailProps) {
  const available = numAttr(attrs, "available") ?? 0;
  const total = numAttr(attrs, "total") ?? numAttr(attrs, "permits") ?? 0;
  const waiters = numAttr(attrs, "waiters") ?? 0;
  const longestWaitNs = numAttr(attrs, "longest_wait_ns");
  const exhausted = available === 0 && total > 0;

  return (
    <>
      {total > 0 && (
        <>
          <KeyValueRow label="Permits">
            <span className={`inspect-val ${exhausted ? "inspect-val--crit" : ""}`}>
              {available} / {total} available
            </span>
          </KeyValueRow>
          <div className="inspect-bar-wrap">
            <div
              className={`inspect-bar inspect-bar--${exhausted ? "crit" : (total - available) / total >= 0.5 ? "warn" : "ok"}`}
            >
              <div
                className="inspect-bar-fill"
                style={{ width: `${((total - available) / total) * 100}%` }}
              />
            </div>
          </div>
        </>
      )}
      {exhausted && (
        <div className="inspect-alert inspect-alert--crit">
          All permits exhausted. {waiters > 0 ? `${waiters} tasks waiting.` : ""}
        </div>
      )}
      <KeyValueRow label="Waiters">
        <span className={`inspect-val ${waiters > 0 ? "inspect-val--crit" : ""}`}>{waiters}</span>
      </KeyValueRow>
      {longestWaitNs != null && longestWaitNs > 0 && (
        <KeyValueRow label="Longest wait">
          <span className="inspect-val">
            <DurationDisplay
              ms={longestWaitNs / 1_000_000}
              tone={durationTone(longestWaitNs, 100_000_000, 1_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
    </>
  );
}

function OnceCellDetail({ attrs }: DetailProps) {
  const state = attr(attrs, "state") ?? "unset";
  const initNs = numAttr(attrs, "init_duration_ns");
  const waiters = numAttr(attrs, "waiters") ?? 0;

  return (
    <>
      <KeyValueRow label="State">
        <span
          className={`inspect-pill inspect-pill--${state === "set" ? "ok" : state === "initializing" ? "warn" : "neutral"}`}
        >
          {state.toUpperCase()}
        </span>
      </KeyValueRow>
      {initNs != null && (
        <KeyValueRow label="Init duration">
          <span className="inspect-val">
            <DurationDisplay
              ms={initNs / 1_000_000}
              tone={durationTone(initNs, 1_000_000_000, 5_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
      {waiters > 0 && (
        <KeyValueRow label="Waiters">
          <span className="inspect-val inspect-val--crit">{waiters}</span>
        </KeyValueRow>
      )}
    </>
  );
}

function RpcRequestDetail({ attrs, graph, onSelectNode }: DetailProps) {
  const status = firstAttr(attrs, ["status"]) ?? "in_flight";
  const elapsedNs = requestElapsedNs(attrs, status);
  const connection = rpcConnectionAttr(attrs);

  return (
    <>
      <KeyValueRow label="Status">
        <span
          className={`inspect-pill inspect-pill--${status === "completed" ? "ok" : status === "timed_out" ? "crit" : "neutral"}`}
        >
          {status.toUpperCase()}
        </span>
      </KeyValueRow>
      {elapsedNs != null && (
        <KeyValueRow label="Elapsed">
          <span className="inspect-val">
            <DurationDisplay
              ms={elapsedNs / 1_000_000}
              tone={durationTone(elapsedNs, 1_000_000_000, 10_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
      {connection && (
        <KeyValueRow label="Connection">
          <RelatedConnectionLink
            connection={connection}
            graph={graph}
            onSelectNode={onSelectNode}
          />
        </KeyValueRow>
      )}
    </>
  );
}

function RpcResponseDetail({ attrs, graph, onSelectNode }: DetailProps) {
  const status = firstAttr(attrs, ["status"]) ?? "handling";
  const { elapsedNs, handledElapsedNs, queueWaitNs } = responseTiming(attrs, status);
  const connection = rpcConnectionAttr(attrs);

  return (
    <>
      <KeyValueRow label="Status">
        <span
          className={`inspect-pill inspect-pill--${status === "delivered" || status === "completed" ? "ok" : status === "cancelled" ? "crit" : "warn"}`}
        >
          {status.toUpperCase()}
        </span>
      </KeyValueRow>
      {connection && (
        <KeyValueRow label="Connection">
          <RelatedConnectionLink
            connection={connection}
            graph={graph}
            onSelectNode={onSelectNode}
          />
        </KeyValueRow>
      )}
      {elapsedNs != null && (
        <KeyValueRow label="Elapsed">
          <span className="inspect-val">
            <DurationDisplay
              ms={elapsedNs / 1_000_000}
              tone={durationTone(elapsedNs, 1_000_000_000, 10_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
      {handledElapsedNs != null && (
        <KeyValueRow label="Handled">
          <span className="inspect-val">
            <DurationDisplay
              ms={handledElapsedNs / 1_000_000}
              tone={durationTone(handledElapsedNs, 500_000_000, 5_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
      {queueWaitNs != null && (
        <KeyValueRow label="Queue wait">
          <span className="inspect-val">
            <DurationDisplay
              ms={queueWaitNs / 1_000_000}
              tone={durationTone(queueWaitNs, 250_000_000, 2_000_000_000)}
            />
          </span>
        </KeyValueRow>
      )}
    </>
  );
}

function formatOptionalTimestampNs(value?: number): string {
  if (value == null) return "—";
  return formatTimelineTimestamp(getCreatedAtNs({ created_at: value }) ?? value);
}

function ConnectionDetail({ attrs }: DetailProps) {
  const connectionId = firstAttr(attrs, ["connection.id", "rpc.connection", "connection"]);
  const state = firstAttr(attrs, ["connection.state", "state"]);
  const openedAtNs = firstNumAttr(attrs, ["connection.opened_at_ns", "opened_at_ns"]);
  const closedAtNs = firstNumAttr(attrs, ["connection.closed_at_ns", "closed_at_ns"]);
  const lastFrameRecvAtNs = firstNumAttr(attrs, [
    "connection.last_frame_recv_at_ns",
    "last_frame_recv_at_ns",
  ]);
  const lastFrameSentAtNs = firstNumAttr(attrs, [
    "connection.last_frame_sent_at_ns",
    "last_frame_sent_at_ns",
  ]);
  const pendingRequests = firstNumAttr(attrs, ["connection.pending_requests", "pending_requests"]);

  return (
    <>
      <KeyValueRow label="Connection">
        <span className="inspect-val inspect-val--mono">{connectionId ?? "—"}</span>
      </KeyValueRow>
      <KeyValueRow label="State">
        <span
          className={`inspect-pill inspect-pill--${
            state === "open" ? "ok" : state === "closed" ? "crit" : "neutral"
          }`}
        >
          {(state ?? "—").toUpperCase()}
        </span>
      </KeyValueRow>
      <KeyValueRow label="Opened">
        <span className="inspect-val">{formatOptionalTimestampNs(openedAtNs)}</span>
      </KeyValueRow>
      <KeyValueRow label="Closed">
        <span className="inspect-val">{formatOptionalTimestampNs(closedAtNs)}</span>
      </KeyValueRow>
      <KeyValueRow label="Last frame recv">
        <span className="inspect-val">
          {formatOptionalTimestampNs(lastFrameRecvAtNs)}
        </span>
      </KeyValueRow>
      <KeyValueRow label="Last frame sent">
        <span className="inspect-val">
          {formatOptionalTimestampNs(lastFrameSentAtNs)}
        </span>
      </KeyValueRow>
      <KeyValueRow label="Pending requests">
        <span className="inspect-val">{pendingRequests != null ? pendingRequests : "—"}</span>
      </KeyValueRow>
    </>
  );
}

const kindDetailMap: Record<string, (props: DetailProps) => React.ReactNode> = {
  future: FutureDetail,
  task: FutureDetail,
  mutex: MutexDetail,
  rwlock: RwLockDetail,
  channel_tx: ChannelTxDetail,
  channel_rx: ChannelRxDetail,
  mpsc_tx: ChannelTxDetail,
  mpsc_rx: ChannelRxDetail,
  oneshot: OneshotDetail,
  oneshot_tx: OneshotDetail,
  oneshot_rx: OneshotDetail,
  watch: WatchDetail,
  watch_tx: WatchDetail,
  watch_rx: WatchDetail,
  semaphore: SemaphoreDetail,
  oncecell: OnceCellDetail,
  once_cell: OnceCellDetail,
  connection: ConnectionDetail,
  request: RpcRequestDetail,
  response: RpcResponseDetail,
};
