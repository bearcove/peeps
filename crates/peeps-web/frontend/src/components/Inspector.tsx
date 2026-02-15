import {
  MagnifyingGlass,
  CaretLeft,
  CaretRight,
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
} from "@phosphor-icons/react";
import type { StuckRequest, SnapshotNode } from "../types";

interface InspectorProps {
  selectedRequest: StuckRequest | null;
  selectedNode: SnapshotNode | null;
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

function numAttr(attrs: Record<string, unknown>, key: string): number | undefined {
  const v = attrs[key];
  if (v == null) return undefined;
  const n = Number(v);
  return isNaN(n) ? undefined : n;
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
};

export function Inspector({ selectedRequest, selectedNode, collapsed, onToggleCollapse }: InspectorProps) {
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
        ) : selectedNode ? (
          <NodeDetail node={selectedNode} />
        ) : (
          <div className="inspector-empty">
            Select a request or graph node to inspect.
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
      <dt>ID</dt>
      <dd>{req.id}</dd>
      <dt>Method</dt>
      <dd>{req.method ?? "unknown"}</dd>
      <dt>Process</dt>
      <dd>{req.process}</dd>
      <dt>Elapsed</dt>
      <dd>{formatElapsedFull(req.elapsed_ns)}</dd>
      <dt>Connection</dt>
      <dd>{req.connection ?? "—"}</dd>
      <dt>Correlation Key</dt>
      <dd>{req.correlation_key ?? "—"}</dd>
    </dl>
  );
}

function NodeDetail({ node }: { node: SnapshotNode }) {
  const icon = kindIcons[node.kind];
  const DetailComponent = kindDetailMap[node.kind];

  return (
    <div className="inspect-node">
      <div className="inspect-node-header">
        {icon && <span className="inspect-node-icon">{icon}</span>}
        <div>
          <div className="inspect-node-kind">{node.kind}</div>
          <div className="inspect-node-label">
            {(node.attrs.label as string) ?? (node.attrs.name as string) ?? (node.attrs.method as string) ?? node.id}
          </div>
        </div>
      </div>

      <div className="inspect-section">
        <div className="inspect-row">
          <span className="inspect-key">ID</span>
          <span className="inspect-val">{node.id}</span>
        </div>
        <div className="inspect-row">
          <span className="inspect-key">Process</span>
          <span className="inspect-val">{node.process}</span>
        </div>
        <div className="inspect-row">
          <span className="inspect-key">Proc Key</span>
          <span className="inspect-val">{node.proc_key}</span>
        </div>
      </div>

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
  const entries = Object.entries(attrs).filter(([, v]) => v != null);
  if (entries.length === 0) return null;

  return (
    <details className="inspect-raw">
      <summary>All attributes ({entries.length})</summary>
      <dl>
        {entries.map(([key, val]) => (
          <div key={key}>
            <dt>{key}</dt>
            <dd>{typeof val === "object" ? JSON.stringify(val) : String(val)}</dd>
          </div>
        ))}
      </dl>
    </details>
  );
}

// ── Kind-specific detail sections ────────────────────────────

type DetailProps = { attrs: Record<string, unknown> };

function FutureDetail({ attrs }: DetailProps) {
  const state = attr(attrs, "state");
  const pollCount = numAttr(attrs, "poll_count");
  const lastPolledNs = numAttr(attrs, "last_polled_ns");
  const source = attr(attrs, "source_location");

  return (
    <>
      {state && (
        <div className="inspect-row">
          <span className="inspect-key">State</span>
          <span className={`inspect-pill inspect-pill--${state === "completed" ? "ok" : state === "polling" ? "ok" : "neutral"}`}>
            {state}
          </span>
        </div>
      )}
      {pollCount != null && (
        <div className="inspect-row">
          <span className="inspect-key">Poll count</span>
          <span className={`inspect-val ${pollCount === 0 ? "inspect-val--crit" : ""}`}>
            {pollCount}{pollCount === 0 ? " (never polled!)" : ""}
          </span>
        </div>
      )}
      {lastPolledNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Last polled</span>
          <span className={`inspect-val ${durationClass(lastPolledNs, 1_000_000_000, 5_000_000_000)}`}>
            {formatDuration(lastPolledNs)} ago
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
  const waiters = numAttr(attrs, "waiters") ?? 0;
  const heldNs = numAttr(attrs, "held_ns");
  const longestWaitNs = numAttr(attrs, "longest_wait_ns");

  return (
    <>
      <div className="inspect-row">
        <span className="inspect-key">State</span>
        <span className={`inspect-pill inspect-pill--${holder ? "crit" : "ok"}`}>
          {holder ? "HELD" : "FREE"}
        </span>
      </div>
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
        <span className={`inspect-val ${waiters > 0 ? "inspect-val--crit" : ""}`}>
          {waiters}
        </span>
      </div>
      {longestWaitNs != null && longestWaitNs > 0 && (
        <div className="inspect-row">
          <span className="inspect-key">Longest wait</span>
          <span className={`inspect-val ${durationClass(longestWaitNs, 100_000_000, 1_000_000_000)}`}>
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
        <span className={`inspect-val ${(writerWaiters + readerWaiters) > 0 ? "inspect-val--crit" : ""}`}>
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
            <div className={`inspect-bar inspect-bar--${isFull ? "crit" : buffered / capacity >= 0.5 ? "warn" : "ok"}`}>
              <div className="inspect-bar-fill" style={{ width: `${Math.min(buffered / capacity, 1) * 100}%` }} />
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
          <span className={`inspect-val ${isSenderAlive ? "inspect-val--ok" : "inspect-val--crit"}`}>
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
          <span className={`inspect-val ${durationClass(lastUpdatedNs, 5_000_000_000, 30_000_000_000)}`}>
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
            <div className={`inspect-bar inspect-bar--${exhausted ? "crit" : (total - available) / total >= 0.5 ? "warn" : "ok"}`}>
              <div className="inspect-bar-fill" style={{ width: `${((total - available) / total) * 100}%` }} />
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
        <span className={`inspect-val ${waiters > 0 ? "inspect-val--crit" : ""}`}>
          {waiters}
        </span>
      </div>
      {longestWaitNs != null && longestWaitNs > 0 && (
        <div className="inspect-row">
          <span className="inspect-key">Longest wait</span>
          <span className={`inspect-val ${durationClass(longestWaitNs, 100_000_000, 1_000_000_000)}`}>
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
        <span className={`inspect-pill inspect-pill--${state === "set" ? "ok" : state === "initializing" ? "warn" : "neutral"}`}>
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
  const method = attr(attrs, "method");
  const elapsedNs = numAttr(attrs, "elapsed_ns");
  const status = attr(attrs, "status") ?? "in_flight";
  const process = attr(attrs, "process");
  const connection = attr(attrs, "connection");
  const correlationKey = attr(attrs, "correlation_key");

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
        <span className={`inspect-pill inspect-pill--${status === "completed" ? "ok" : status === "timed_out" ? "crit" : "neutral"}`}>
          {status.toUpperCase()}
        </span>
      </div>
      {elapsedNs != null && (
        <div className="inspect-row">
          <span className="inspect-key">Elapsed</span>
          <span className={`inspect-val ${durationClass(elapsedNs, 1_000_000_000, 10_000_000_000)}`}>
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
  const status = attr(attrs, "status") ?? "in_flight";
  const elapsedNs = numAttr(attrs, "elapsed_ns");
  const correlationKey = attr(attrs, "correlation_key");

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
          <span className={`inspect-val ${durationClass(elapsedNs, 1_000_000_000, 10_000_000_000)}`}>
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
