import { useState } from "react";
import {
  MagnifyingGlass,
  CaretLeft,
  CaretRight,
  Tag,
  BracketsCurly,
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
  ToggleRight,
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
import type { StuckRequest, SnapshotNode, SnapshotEdge, SnapshotGraph } from "../types";

interface InspectorProps {
  selectedRequest: StuckRequest | null;
  selectedNode: SnapshotNode | null;
  selectedEdge: SnapshotEdge | null;
  graph: SnapshotGraph | null;
  filteredNodeId: string | null;
  onFocusNode: (nodeId: string | null) => void;
  onSelectNode: (nodeId: string) => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
}

function formatElapsedFull(ns: number): string {
  const ms = ns / 1_000_000;
  const secs = ns / 1_000_000_000;
  if (secs >= 60) {
    const mins = Math.floor(secs / 60);
    const rem = secs % 60;
    return `${mins}m ${rem.toFixed(1)}s (${ms.toLocaleString()}ms)`;
  }
  return `${secs.toFixed(3)}s (${ms.toLocaleString()}ms)`;
}

function formatDuration(ns: number): string {
  const secs = ns / 1_000_000_000;
  if (secs < 0.001) return `${(ns / 1_000).toFixed(0)}µs`;
  if (secs < 1) return `${(ns / 1_000_000).toFixed(0)}ms`;
  if (secs < 60) return `${secs.toFixed(3)}s`;
  if (secs < 3600) return `${(secs / 60).toFixed(1)}m`;
  return `${(secs / 3600).toFixed(1)}h`;
}

function durationClass(ns: number, warnNs: number, critNs: number): string {
  if (ns >= critNs) return "inspect-val--crit";
  if (ns >= warnNs) return "inspect-val--warn";
  return "inspect-val--ok";
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

const kindIcons: Record<string, React.ReactNode> = {
  future: <Timer size={16} weight="bold" />,
  task: <Timer size={16} weight="bold" />,
  mutex: <Lock size={16} weight="bold" />,
  rwlock: <LockOpen size={16} weight="bold" />,
  channel_tx: <ArrowLineUp size={16} weight="bold" />,
  channel_rx: <ArrowLineDown size={16} weight="bold" />,
  mpsc_tx: <ArrowLineUp size={16} weight="bold" />,
  mpsc_rx: <ArrowLineDown size={16} weight="bold" />,
  oneshot: <ToggleRight size={16} weight="bold" />,
  oneshot_tx: <ToggleRight size={16} weight="bold" />,
  oneshot_rx: <ToggleRight size={16} weight="bold" />,
  watch: <Eye size={16} weight="bold" />,
  watch_tx: <Eye size={16} weight="bold" />,
  watch_rx: <Eye size={16} weight="bold" />,
  semaphore: <Gauge size={16} weight="bold" />,
  oncecell: <ToggleRight size={16} weight="bold" />,
  once_cell: <ToggleRight size={16} weight="bold" />,
  request: <PaperPlaneTilt size={16} weight="bold" />,
  response: <ArrowBendDownLeft size={16} weight="bold" />,
  ghost: <Ghost size={16} weight="bold" />,
};

export function Inspector({
  selectedRequest,
  selectedNode,
  selectedEdge,
  graph,
  filteredNodeId,
  onFocusNode,
  onSelectNode,
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
          <RequestDetail req={selectedRequest} />
        ) : selectedEdge ? (
          <EdgeDetail edge={selectedEdge} graph={graph} onSelectNode={onSelectNode} />
        ) : selectedNode ? (
          selectedNode.kind === "ghost" ? (
            <GhostDetail node={selectedNode} graph={graph} />
          ) : (
            <NodeDetail
              node={selectedNode}
              graph={graph}
              filteredNodeId={filteredNodeId}
              onFocusNode={onFocusNode}
              onSelectNode={onSelectNode}
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

function RequestDetail({ req }: { req: StuckRequest }) {
  return (
    <dl>
      <dt>Method</dt>
      <dd>{req.method ?? "unknown"}</dd>
      <dt>Process</dt>
      <dd>{req.process}</dd>
      <dt>Elapsed</dt>
      <dd>{formatElapsedFull(req.elapsed_ns)}</dd>
      <dt>Connection</dt>
      <dd>{req.connection ?? "—"}</dd>
    </dl>
  );
}

function nodeLabel(graph: SnapshotGraph | null, nodeId: string): string {
  if (!graph) return nodeId;
  const node = graph.nodes.find((n) => n.id === nodeId);
  if (!node) return nodeId;
  return (
    firstAttr(node.attrs, ["label", "method", "request.method", "name"]) ?? nodeId
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
        <div className="inspect-row">
          <span className="inspect-key">Kind</span>
          <span className={`inspect-pill inspect-pill--${kindVariant}`}>
            {edge.kind.toUpperCase()}
          </span>
        </div>
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
  const refProcKey = attr(node.attrs, "referenced_proc_key");

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
        <div className="inspect-row">
          <span className="inspect-key">ID</span>
          <span className="inspect-val inspect-val--copyable">
            {node.id}
            <CopyIdButton id={node.id} />
          </span>
        </div>
        <div className="inspect-row">
          <span className="inspect-key">Reason</span>
          <span className="inspect-pill inspect-pill--neutral">{reason}</span>
        </div>
        {refProcKey && (
          <div className="inspect-row">
            <span className="inspect-key">Proc Key</span>
            <span className="inspect-val inspect-val--mono">{refProcKey}</span>
          </div>
        )}
        <div className="inspect-row">
          <span className="inspect-key">Incoming</span>
          <span className="inspect-val">{incoming}</span>
        </div>
        <div className="inspect-row">
          <span className="inspect-key">Outgoing</span>
          <span className="inspect-val">{outgoing}</span>
        </div>
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

function NodeDetail({
  node,
  graph,
  filteredNodeId,
  onFocusNode,
  onSelectNode,
}: {
  node: SnapshotNode;
  graph: SnapshotGraph | null;
  filteredNodeId: string | null;
  onFocusNode: (nodeId: string | null) => void;
  onSelectNode: (nodeId: string) => void;
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
      <ToggleRight size={16} weight="bold" />
    ) : (node.kind === "rx" || node.kind.endsWith("_rx")) && channelKind === "oneshot" ? (
      <ToggleRight size={16} weight="bold" />
    ) : node.kind === "tx" || node.kind.endsWith("_tx") ? (
      <ArrowLineUp size={16} weight="bold" />
    ) : node.kind === "rx" || node.kind.endsWith("_rx") ? (
      <ArrowLineDown size={16} weight="bold" />
    ) : (
      kindIcons[node.kind]
    );
  const DetailComponent = kindDetailMap[node.kind];
  const isFocused = filteredNodeId === node.id;
  const method =
    node.kind === "request" || node.kind === "response"
      ? firstAttr(node.attrs, ["method", "request.method", "response.method"])
      : undefined;
  const correlationKey =
    node.kind === "request" || node.kind === "response"
      ? firstAttr(node.attrs, [
          "correlation_key",
          "request.correlation_key",
          "response.correlation_key",
        ])
      : undefined;
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
                  firstAttr(node.attrs, ["method", "request.method"]) ??
                  firstAttr(node.attrs, ["label", "name"]) ??
                  node.id
                );
              }
              if (node.kind === "response") {
                return (
                  firstAttr(node.attrs, ["method", "response.method", "request.method"]) ??
                  firstAttr(node.attrs, ["label", "name"]) ??
                  firstAttr(node.attrs, [
                    "correlation_key",
                    "response.correlation_key",
                    "request.correlation_key",
                  ]) ??
                  node.id
                );
              }
              return firstAttr(node.attrs, ["label", "name", "method"]) ?? node.id;
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

      <div className="inspect-section">
        <div className="inspect-row">
          <span className="inspect-key">ID</span>
          <span className="inspect-val inspect-val--copyable">
            {node.id}
            <CopyIdButton id={node.id} />
          </span>
        </div>
        {method && (
          <div className="inspect-row">
            <span className="inspect-key">Method</span>
            <span className="inspect-val inspect-val--mono">{method}</span>
          </div>
        )}
        {correlationKey && (
          <div className="inspect-row">
            <span className="inspect-key">Correlation</span>
            <span className="inspect-val inspect-val--mono">{correlationKey}</span>
          </div>
        )}
        <div className="inspect-row">
          <span className="inspect-key">Process</span>
          <span className="inspect-val">{node.process}</span>
        </div>
        <div className="inspect-row">
          <span className="inspect-key">Proc Key</span>
          <span className="inspect-val">{node.proc_key}</span>
        </div>
      </div>

      {(uniqueBlockers.length > 0 || uniqueDependents.length > 0) && (
        <div className="inspect-section">
          <div className="inspect-row">
            <span className="inspect-key">Wait blockers</span>
            <span className={`inspect-val ${uniqueBlockers.length > 0 ? "inspect-val--crit" : ""}`}>
              {uniqueBlockers.length}
            </span>
          </div>
          {uniqueBlockers.slice(0, 8).map((id) => (
            <div className="inspect-row" key={`blk:${id}`}>
              <span className="inspect-key">waits on</span>
              <button className="inspect-edge-node-btn" onClick={() => onSelectNode(id)} title={id}>
                {nodeLabel(graph, id)}
              </button>
            </div>
          ))}
          <div className="inspect-row">
            <span className="inspect-key">Dependents</span>
            <span className={`inspect-val ${uniqueDependents.length > 0 ? "inspect-val--warn" : ""}`}>
              {uniqueDependents.length}
            </span>
          </div>
          {uniqueDependents.slice(0, 8).map((id) => (
            <div className="inspect-row" key={`dep:${id}`}>
              <span className="inspect-key">blocking</span>
              <button className="inspect-edge-node-btn" onClick={() => onSelectNode(id)} title={id}>
                {nodeLabel(graph, id)}
              </button>
            </div>
          ))}
        </div>
      )}

      {DetailComponent && (
        <div className="inspect-section">
          <DetailComponent attrs={node.attrs} />
        </div>
      )}

      <RawAttrs attrs={node.attrs} />
    </div>
  );
}

function RawAttrs({ attrs }: { attrs: Record<string, unknown> }) {
  const [expanded, setExpanded] = useState(false);
  const entries = Object.entries(attrs).filter(([, v]) => v != null);
  if (entries.length === 0) return null;

  function parseJsonObject(s: string): Record<string, unknown> | null {
    const t = s.trim();
    if (!t.startsWith("{") || !t.endsWith("}")) return null;
    try {
      const v = JSON.parse(t) as unknown;
      if (v && typeof v === "object" && !Array.isArray(v)) return v as Record<string, unknown>;
      return null;
    } catch {
      return null;
    }
  }

  function asObject(val: unknown): Record<string, unknown> | null {
    if (val && typeof val === "object" && !Array.isArray(val))
      return val as Record<string, unknown>;
    if (typeof val === "string") return parseJsonObject(val);
    return null;
  }

  function metaLocation(meta: Record<string, unknown>): string | null {
    let loc: string | null = null;
    if (meta["ctx.location"] != null) loc = String(meta["ctx.location"]);
    if (!loc) return null;

    return loc;
  }

  function MetaView({ meta }: { meta: Record<string, unknown> }) {
    const loc = metaLocation(meta);
    if (!loc) return <span className="inspect-val inspect-val--mono">—</span>;

    function displayLocation(location: string): string {
      if (!location.startsWith("/")) return location;

      const roots = [
        "/crates/",
        "/apps/",
        "/docs/",
        "/internal/",
        "/scripts/",
        "/tests/",
        "/xtask/",
      ];
      for (const root of roots) {
        const idx = location.indexOf(root);
        if (idx >= 0) return location.slice(idx + 1);
      }
      return location;
    }

    const href = `zed://file/${encodeURIComponent(loc)}`;
    const display = displayLocation(loc);
    return (
      <div className="inspect-meta">
        <div className="inspect-meta-row">
          <span className="inspect-meta-key inspect-meta-key--icon" title="Rust source location">
            rs
          </span>
          <a
            className="inspect-meta-val inspect-val--mono inspect-link"
            href={href}
            title="Open in Zed"
          >
            <ArrowSquareOut size={12} weight="bold" className="inspect-link-icon" />
            {display}
          </a>
        </div>
      </div>
    );
  }

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
    if (k === "ctx.location" && typeof val === "string") {
      return <MetaView meta={{ "ctx.location": val }} />;
    }
    if (k === "meta" || k.endsWith(".meta")) {
      const obj = asObject(val);
      if (obj) return <MetaView meta={obj} />;
    }

    const maybeNs = asFiniteNumber(val);
    if (maybeNs != null && (k.endsWith("_ns") || k.includes("duration") || k.includes("age"))) {
      return formatDuration(maybeNs);
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
      case "meta":
        return <BracketsCurly size={12} weight="bold" />;
    }

    // Heuristic mapping by convention (keys are stable, values vary).
    const k = key.toLowerCase();
    if (k.endsWith(".id") || k === "request.id") return <Hash size={12} weight="bold" />;
    if (k.includes("correlation_key")) return <Key size={12} weight="bold" />;
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
        <button
          className="inspect-raw-toggle"
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

type DetailProps = { attrs: Record<string, unknown> };

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
  const source = attr(attrs, "source_location");

  return (
    <>
      {state && (
        <div className="inspect-row">
          <span className="inspect-key">State</span>
          <span
            className={`inspect-pill inspect-pill--${state === "completed" ? "ok" : state === "polling" ? "ok" : "neutral"}`}
          >
            {state}
          </span>
        </div>
      )}
      {pollCount != null && (
        <div className="inspect-row">
          <span className="inspect-key">Poll count</span>
          <span className={`inspect-val ${pollCount === 0 ? "inspect-val--crit" : ""}`}>
            {pollCount}
            {pollCount === 0 ? " (never polled!)" : ""}
          </span>
        </div>
      )}
      {lastPolledNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Idle</span>
          <span
            className={`inspect-val ${durationClass(lastPolledNs, 1_000_000_000, 5_000_000_000)}`}
          >
            {formatDuration(lastPolledNs)} ago
          </span>
        </div>
      )}
      {inPollNs != null && inPollNs > 0 && (
        <div className="inspect-row">
          <span className="inspect-key">In poll</span>
          <span className={`inspect-val ${durationClass(inPollNs, 1_000_000_000, 5_000_000_000)}`}>
            {formatDuration(inPollNs)}
          </span>
        </div>
      )}
      {source && (
        <div className="inspect-row">
          <span className="inspect-key">Source</span>
          <span className="inspect-val inspect-val--mono">{source}</span>
        </div>
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
      <div className="inspect-row">
        <span className="inspect-key">State</span>
        <span className={`inspect-pill inspect-pill--${holder ? "crit" : "ok"}`}>
          {holderCount > 0 ? "HELD" : "FREE"}
        </span>
      </div>
      {!holder && holderCount > 0 && (
        <div className="inspect-row">
          <span className="inspect-key">Holders</span>
          <span className="inspect-val">{holderCount}</span>
        </div>
      )}
      {holder && (
        <div className="inspect-row">
          <span className="inspect-key">Holder</span>
          <span className="inspect-val inspect-val--mono">{holder}</span>
        </div>
      )}
      {heldNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Hold duration</span>
          <span className={`inspect-val ${durationClass(heldNs, 100_000_000, 1_000_000_000)}`}>
            {formatDuration(heldNs)}
          </span>
        </div>
      )}
      <div className="inspect-row">
        <span className="inspect-key">Waiters</span>
        <span className={`inspect-val ${waiters > 0 ? "inspect-val--crit" : ""}`}>{waiters}</span>
      </div>
      {longestWaitNs != null && longestWaitNs > 0 && (
        <div className="inspect-row">
          <span className="inspect-key">Longest wait</span>
          <span
            className={`inspect-val ${durationClass(longestWaitNs, 100_000_000, 1_000_000_000)}`}
          >
            {formatDuration(longestWaitNs)}
          </span>
        </div>
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
      <div className="inspect-row">
        <span className="inspect-key">State</span>
        <span className={`inspect-pill inspect-pill--${readers > 0 || holder ? "crit" : "ok"}`}>
          {readers > 0 ? `${readers} readers` : holder ? `Write: ${holder}` : "FREE"}
        </span>
      </div>
      {heldNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Hold duration</span>
          <span className={`inspect-val ${durationClass(heldNs, 100_000_000, 1_000_000_000)}`}>
            {formatDuration(heldNs)}
          </span>
        </div>
      )}
      <div className="inspect-row">
        <span className="inspect-key">Waiters</span>
        <span
          className={`inspect-val ${writerWaiters + readerWaiters > 0 ? "inspect-val--crit" : ""}`}
        >
          {readerWaiters}R + {writerWaiters}W
        </span>
      </div>
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
          <div className="inspect-row">
            <span className="inspect-key">Buffer</span>
            <span className={`inspect-val ${isFull ? "inspect-val--crit" : ""}`}>
              {buffered} / {capacity} ({pct}%)
            </span>
          </div>
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
        <div className="inspect-row">
          <span className="inspect-pill inspect-pill--crit">FULL — senders blocking</span>
        </div>
      )}
      {senderCount != null && (
        <div className="inspect-row">
          <span className="inspect-key">Sender handles</span>
          <span className="inspect-val">{senderCount}</span>
        </div>
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
      <div className="inspect-row">
        <span className="inspect-key">State</span>
        <span className={`inspect-pill inspect-pill--${state === "starved" ? "warn" : "neutral"}`}>
          {state}
        </span>
      </div>
      {receiverAlive != null && (
        <div className="inspect-row">
          <span className="inspect-key">Receiver</span>
          <span className={`inspect-val ${isAlive ? "inspect-val--ok" : "inspect-val--crit"}`}>
            {isAlive ? "alive" : "DEAD"}
          </span>
        </div>
      )}
      {pending != null && (
        <div className="inspect-row">
          <span className="inspect-key">Pending</span>
          <span className="inspect-val">{pending}</span>
        </div>
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
      <div className="inspect-row">
        <span className="inspect-key">State</span>
        <span className={`inspect-pill inspect-pill--${variant}`}>
          {isDropped && <Warning size={12} weight="bold" />}
          {state.toUpperCase()}
        </span>
      </div>
      {isDropped && (
        <div className="inspect-alert inspect-alert--crit">
          Sender dropped without sending. Receiver will never resolve — potential deadlock.
        </div>
      )}
      {elapsedNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Elapsed</span>
          <span className={`inspect-val ${durationClass(elapsedNs, 1_000_000_000, 5_000_000_000)}`}>
            {formatDuration(elapsedNs)}
          </span>
        </div>
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
        <div className="inspect-row">
          <span className="inspect-key">Subscribers</span>
          <span className="inspect-val">{subscribers}</span>
        </div>
      )}
      {senderAlive != null && (
        <div className="inspect-row">
          <span className="inspect-key">Sender</span>
          <span
            className={`inspect-val ${isSenderAlive ? "inspect-val--ok" : "inspect-val--crit"}`}
          >
            {isSenderAlive ? "alive" : "DROPPED"}
          </span>
        </div>
      )}
      {senderAlive != null && !isSenderAlive && (
        <div className="inspect-alert inspect-alert--crit">
          Sender dropped. All receivers will see stale data forever.
        </div>
      )}
      {lastUpdatedNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Last updated</span>
          <span
            className={`inspect-val ${durationClass(lastUpdatedNs, 5_000_000_000, 30_000_000_000)}`}
          >
            {formatDuration(lastUpdatedNs)} ago
          </span>
        </div>
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
          <div className="inspect-row">
            <span className="inspect-key">Permits</span>
            <span className={`inspect-val ${exhausted ? "inspect-val--crit" : ""}`}>
              {available} / {total} available
            </span>
          </div>
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
      <div className="inspect-row">
        <span className="inspect-key">Waiters</span>
        <span className={`inspect-val ${waiters > 0 ? "inspect-val--crit" : ""}`}>{waiters}</span>
      </div>
      {longestWaitNs != null && longestWaitNs > 0 && (
        <div className="inspect-row">
          <span className="inspect-key">Longest wait</span>
          <span
            className={`inspect-val ${durationClass(longestWaitNs, 100_000_000, 1_000_000_000)}`}
          >
            {formatDuration(longestWaitNs)}
          </span>
        </div>
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
      <div className="inspect-row">
        <span className="inspect-key">State</span>
        <span
          className={`inspect-pill inspect-pill--${state === "set" ? "ok" : state === "initializing" ? "warn" : "neutral"}`}
        >
          {state.toUpperCase()}
        </span>
      </div>
      {initNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Init duration</span>
          <span className={`inspect-val ${durationClass(initNs, 1_000_000_000, 5_000_000_000)}`}>
            {formatDuration(initNs)}
          </span>
        </div>
      )}
      {waiters > 0 && (
        <div className="inspect-row">
          <span className="inspect-key">Waiters</span>
          <span className="inspect-val inspect-val--crit">{waiters}</span>
        </div>
      )}
    </>
  );
}

function RpcRequestDetail({ attrs }: DetailProps) {
  const method = firstAttr(attrs, ["method", "request.method"]);
  const elapsedNs = firstNumAttr(attrs, ["elapsed_ns", "request.elapsed_ns"]);
  const status = firstAttr(attrs, ["status", "request.status"]) ?? "in_flight";
  const process = firstAttr(attrs, ["process", "request.process"]);
  const connection = firstAttr(attrs, ["connection", "rpc.connection"]);
  const correlationKey = firstAttr(attrs, ["correlation_key", "request.correlation_key"]);

  return (
    <>
      {method && (
        <div className="inspect-row">
          <span className="inspect-key">Method</span>
          <span className="inspect-val inspect-val--mono">{method}</span>
        </div>
      )}
      <div className="inspect-row">
        <span className="inspect-key">Status</span>
        <span
          className={`inspect-pill inspect-pill--${status === "completed" ? "ok" : status === "timed_out" ? "crit" : "neutral"}`}
        >
          {status.toUpperCase()}
        </span>
      </div>
      {elapsedNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Elapsed</span>
          <span
            className={`inspect-val ${durationClass(elapsedNs, 1_000_000_000, 10_000_000_000)}`}
          >
            {formatElapsedFull(elapsedNs)}
          </span>
        </div>
      )}
      {process && (
        <div className="inspect-row">
          <span className="inspect-key">Process</span>
          <span className="inspect-val">{process}</span>
        </div>
      )}
      {connection && (
        <div className="inspect-row">
          <span className="inspect-key">Connection</span>
          <span className="inspect-val inspect-val--mono">{connection}</span>
        </div>
      )}
      {correlationKey && (
        <div className="inspect-row">
          <span className="inspect-key">Correlation</span>
          <span className="inspect-val inspect-val--mono">{correlationKey}</span>
        </div>
      )}
    </>
  );
}

function RpcResponseDetail({ attrs }: DetailProps) {
  const status = firstAttr(attrs, ["status", "response.status"]) ?? "in_flight";
  const elapsedNs = firstNumAttr(attrs, ["elapsed_ns", "response.elapsed_ns"]);
  const correlationKey = firstAttr(attrs, [
    "correlation_key",
    "response.correlation_key",
    "request.correlation_key",
  ]);

  return (
    <>
      <div className="inspect-row">
        <span className="inspect-key">Status</span>
        <span className={`inspect-pill inspect-pill--${status === "completed" ? "ok" : "neutral"}`}>
          {status.toUpperCase()}
        </span>
      </div>
      {correlationKey && (
        <div className="inspect-row">
          <span className="inspect-key">Correlation</span>
          <span className="inspect-val inspect-val--mono">{correlationKey}</span>
        </div>
      )}
      {elapsedNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Elapsed</span>
          <span
            className={`inspect-val ${durationClass(elapsedNs, 1_000_000_000, 10_000_000_000)}`}
          >
            {formatElapsedFull(elapsedNs)}
          </span>
        </div>
      )}
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
  request: RpcRequestDetail,
  response: RpcResponseDetail,
};
