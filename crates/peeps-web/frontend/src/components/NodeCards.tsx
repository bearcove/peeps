import { memo } from "react";
import {
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
  Users,
  Check,
  X as XIcon,
  Warning,
  Ghost,
  Plugs,
  WifiHigh,
  ArrowFatLineRight,
  ArrowFatLineLeft,
  Stack,
  Terminal,
  FileText,
  Bell,
  Moon,
  Repeat,
  HourglassSimple,
} from "@phosphor-icons/react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";

// ── Helpers ──────────────────────────────────────────────────

export interface NodeData {
  label: string;
  kind: string;
  process: string;
  attrs: Record<string, unknown>;
  [key: string]: unknown;
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

function firstAttr(attrs: Record<string, unknown>, keys: string[]): string | undefined {
  for (const k of keys) {
    const v = attrs[k];
    if (v != null && v !== "") return String(v);
  }
  return undefined;
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

/** Format nanoseconds to a short human-readable duration */
function fmtDuration(ns: number): string {
  const secs = ns / 1_000_000_000;
  if (secs < 0.001) return `${(ns / 1_000).toFixed(0)}µs`;
  if (secs < 1) return `${(ns / 1_000_000).toFixed(0)}ms`;
  if (secs < 60) return `${secs.toFixed(1)}s`;
  if (secs < 3600) return `${(secs / 60).toFixed(1)}m`;
  return `${(secs / 3600).toFixed(1)}h`;
}

/** Returns a CSS modifier class based on duration thresholds */
function durationSeverity(ns: number, warnNs: number, critNs: number): string {
  if (ns >= critNs) return "crit";
  if (ns >= warnNs) return "warn";
  return "ok";
}

// Stable color from process name
const processColorCache = new Map<string, string>();
export function processColor(process: string): string {
  let color = processColorCache.get(process);
  if (color) return color;
  let hash = 0;
  for (let i = 0; i < process.length; i++) {
    hash = process.charCodeAt(i) + ((hash << 5) - hash);
  }
  const h = ((hash % 360) + 360) % 360;
  color = `hsl(${h}, 65%, 55%)`;
  processColorCache.set(process, color);
  return color;
}

// ── Shared card chrome ───────────────────────────────────────

function CardShell({
  kind,
  process,
  icon,
  label,
  stateClass,
  children,
}: {
  kind: string;
  process: string;
  icon: React.ReactNode;
  label: string;
  stateClass?: string;
  children?: React.ReactNode;
}) {
  const color = processColor(process);
  return (
    <div
      className={`card card--${kind}${stateClass ? ` ${stateClass}` : ""}`}
      style={{ borderLeftColor: color }}
    >
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="card-head">
        <span className="card-icon">{icon}</span>
        <span className="card-label" title={label}>
          {label}
        </span>
      </div>
      {children && <div className="card-body">{children}</div>}
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}

// ── Reusable widgets ─────────────────────────────────────────

function StatePill({ state, variant }: { state: string; variant: "ok" | "warn" | "crit" | "neutral" }) {
  return <span className={`pill pill--${variant}`}>{state}</span>;
}

function CapacityBar({ current, max, label }: { current: number; max: number; label?: string }) {
  const pct = max > 0 ? Math.min(current / max, 1) : 0;
  const severity = pct >= 0.9 ? "crit" : pct >= 0.5 ? "warn" : "ok";
  return (
    <div className="capacity-bar-wrap">
      <div className={`capacity-bar capacity-bar--${severity}`}>
        <div className="capacity-bar-fill" style={{ width: `${pct * 100}%` }} />
      </div>
      <span className="capacity-bar-label">{label ?? `${current}/${max}`}</span>
    </div>
  );
}

function DurationBadge({ ns, warnNs, critNs }: { ns: number; warnNs: number; critNs: number }) {
  const sev = durationSeverity(ns, warnNs, critNs);
  return <span className={`duration duration--${sev}`}>{fmtDuration(ns)}</span>;
}

function WaiterBadge({ count }: { count: number }) {
  if (count <= 0) return null;
  return (
    <span className="waiter-badge waiter-badge--active">
      <Users size={10} weight="bold" />
      {count}
    </span>
  );
}

// ── Per-kind cards ───────────────────────────────────────────

function FutureCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const pollCount = numAttr(attrs, "poll_count");
  const lastPolledNs = numAttr(attrs, "last_polled_ns");
  const state = attr(attrs, "state") ?? "waiting";

  const stateVariant: "ok" | "warn" | "crit" | "neutral" =
    state === "completed" ? "ok" : state === "polling" ? "ok" : "neutral";

  // A future with 0 polls is suspicious
  const pollClass = pollCount === 0 ? "crit" : undefined;

  return (
    <CardShell
      kind="future"
      process={process}
      label={label}
      icon={<Timer size={14} weight="bold" />}
    >
      <div className="card-row">
        <StatePill state={state} variant={stateVariant} />
        {pollCount != null && (
          <span className={`badge ${pollClass ? `badge--${pollClass}` : ""}`}>
            {pollCount} polls
          </span>
        )}
      </div>
      {lastPolledNs != null && (
        <div className="card-row">
          <span className="card-dim">last poll</span>
          <DurationBadge ns={lastPolledNs} warnNs={1_000_000_000} critNs={5_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function MutexCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const holder = attr(attrs, "holder");
  const waiters = numAttr(attrs, "waiters") ?? 0;
  const heldNs = numAttr(attrs, "held_ns");
  const longestWaitNs = numAttr(attrs, "longest_wait_ns");
  const isHeld = holder != null && holder !== "";

  return (
    <CardShell
      kind="mutex"
      process={process}
      label={label}
      icon={<Lock size={14} weight="bold" />}
      stateClass={isHeld ? "card--danger-border" : undefined}
    >
      <div className="card-row">
        <StatePill state={isHeld ? "HELD" : "FREE"} variant={isHeld ? "crit" : "ok"} />
        <WaiterBadge count={waiters} />
      </div>
      {isHeld && (
        <div className="card-row">
          <span className="card-dim holder-text" title={holder}>{holder}</span>
          {heldNs != null && (
            <DurationBadge ns={heldNs} warnNs={100_000_000} critNs={1_000_000_000} />
          )}
        </div>
      )}
      {longestWaitNs != null && longestWaitNs > 0 && (
        <div className="card-row">
          <span className="card-dim">wait</span>
          <DurationBadge ns={longestWaitNs} warnNs={100_000_000} critNs={1_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function RwLockCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const readers = numAttr(attrs, "readers") ?? 0;
  const writerWaiters = numAttr(attrs, "writer_waiters") ?? 0;
  const readerWaiters = numAttr(attrs, "reader_waiters") ?? 0;
  const holder = attr(attrs, "holder");
  const heldNs = numAttr(attrs, "held_ns");
  const isHeld = (holder != null && holder !== "") || readers > 0;
  const totalWaiters = writerWaiters + readerWaiters;

  return (
    <CardShell
      kind="rwlock"
      process={process}
      label={label}
      icon={<LockOpen size={14} weight="bold" />}
      stateClass={isHeld ? "card--danger-border" : undefined}
    >
      <div className="card-row">
        <StatePill
          state={readers > 0 ? `${readers}R` : holder ? "W" : "FREE"}
          variant={isHeld ? "crit" : "ok"}
        />
        {totalWaiters > 0 && (
          <span className="waiter-badge waiter-badge--active">
            <Users size={10} weight="bold" />
            {readerWaiters}R+{writerWaiters}W
          </span>
        )}
      </div>
      {heldNs != null && (
        <div className="card-row">
          <span className="card-dim">held</span>
          <DurationBadge ns={heldNs} warnNs={100_000_000} critNs={1_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function ChannelTxCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const channelKind = firstAttr(attrs, [
    "channel_kind",
    "channel.kind",
    "channel_type",
    "channel.type",
  ]);
  const buffered = numAttr(attrs, "buffered") ?? 0;
  const capacity = numAttr(attrs, "capacity") ?? 0;
  const senderCount = numAttr(attrs, "sender_count");
  const isFull = capacity > 0 && buffered >= capacity;

  const icon =
    channelKind === "watch" ? (
      <Eye size={14} weight="bold" />
    ) : channelKind === "oneshot" ? (
      <ToggleRight size={14} weight="bold" />
    ) : (
      <ArrowLineUp size={14} weight="bold" />
    );

  return (
    <CardShell
      kind="channel-tx"
      process={process}
      label={label}
      icon={icon}
      stateClass={isFull ? "card--danger-border" : undefined}
    >
      {capacity > 0 && (
        <CapacityBar current={buffered} max={capacity} label={`${buffered}/${capacity}`} />
      )}
      <div className="card-row">
        {isFull && <StatePill state="FULL" variant="crit" />}
        {senderCount != null && (
          <span className="badge">{senderCount} senders</span>
        )}
      </div>
    </CardShell>
  );
}

function ChannelRxCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const channelKind = firstAttr(attrs, [
    "channel_kind",
    "channel.kind",
    "channel_type",
    "channel.type",
  ]);
  const state = attr(attrs, "state") ?? "idle";
  const receiverAlive = attr(attrs, "receiver_alive");
  const pending = numAttr(attrs, "pending");
  const isAlive = receiverAlive !== "false" && receiverAlive !== "0";

  const stateVariant: "ok" | "warn" | "crit" | "neutral" =
    state === "draining" ? "ok" : state === "starved" ? "warn" : "neutral";

  const icon =
    channelKind === "watch" ? (
      <Eye size={14} weight="bold" />
    ) : channelKind === "oneshot" ? (
      <ToggleRight size={14} weight="bold" />
    ) : (
      <ArrowLineDown size={14} weight="bold" />
    );

  return (
    <CardShell
      kind="channel-rx"
      process={process}
      label={label}
      icon={icon}
    >
      <div className="card-row">
        <StatePill state={state} variant={stateVariant} />
        <span className={`alive-indicator ${isAlive ? "alive-indicator--ok" : "alive-indicator--dead"}`}>
          {isAlive ? <Check size={10} weight="bold" /> : <XIcon size={10} weight="bold" />}
          rx
        </span>
      </div>
      {pending != null && (
        <div className="card-row">
          <span className="card-dim">pending</span>
          <span className="badge">{pending}</span>
        </div>
      )}
    </CardShell>
  );
}

function OneshotCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const state = attr(attrs, "state") ?? "pending";
  const elapsedNs = numAttr(attrs, "elapsed_ns");

  const stateMap: Record<string, "ok" | "warn" | "crit" | "neutral"> = {
    pending: "neutral",
    sent: "ok",
    completed: "ok",
    dropped: "crit",
  };

  const isDropped = state === "dropped";

  return (
    <CardShell
      kind="oneshot"
      process={process}
      label={label}
      icon={isDropped ? <Warning size={14} weight="bold" /> : <ToggleRight size={14} weight="bold" />}
      stateClass={isDropped ? "card--dropped" : undefined}
    >
      <div className="card-row card-row--center">
        <StatePill state={state.toUpperCase()} variant={stateMap[state] ?? "neutral"} />
      </div>
      {elapsedNs != null && (
        <div className="card-row">
          <span className="card-dim">elapsed</span>
          <DurationBadge ns={elapsedNs} warnNs={1_000_000_000} critNs={5_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function WatchCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const subscribers = numAttr(attrs, "subscriber_count") ?? numAttr(attrs, "subscribers");
  const senderAlive = attr(attrs, "sender_alive");
  const lastUpdatedNs = numAttr(attrs, "last_updated_ns");
  const isSenderAlive = senderAlive !== "false" && senderAlive !== "0";

  return (
    <CardShell
      kind="watch"
      process={process}
      label={label}
      icon={<Eye size={14} weight="bold" />}
      stateClass={senderAlive != null && !isSenderAlive ? "card--danger-border" : undefined}
    >
      <div className="card-row">
        {subscribers != null && (
          <span className="badge">{subscribers} subscribers</span>
        )}
        {senderAlive != null && (
          <span className={`alive-indicator ${isSenderAlive ? "alive-indicator--ok" : "alive-indicator--dead"}`}>
            {isSenderAlive ? <Check size={10} weight="bold" /> : <XIcon size={10} weight="bold" />}
            tx
          </span>
        )}
      </div>
      {lastUpdatedNs != null && (
        <div className="card-row">
          <span className="card-dim">updated</span>
          <DurationBadge ns={lastUpdatedNs} warnNs={5_000_000_000} critNs={30_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function SemaphoreCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const available = numAttr(attrs, "available") ?? 0;
  const total = numAttr(attrs, "total") ?? numAttr(attrs, "permits") ?? 0;
  const waiters = numAttr(attrs, "waiters") ?? 0;
  const longestWaitNs = numAttr(attrs, "longest_wait_ns");

  return (
    <CardShell
      kind="semaphore"
      process={process}
      label={label}
      icon={<Gauge size={14} weight="bold" />}
      stateClass={available === 0 && total > 0 ? "card--danger-border" : undefined}
    >
      {total > 0 && (
        <CapacityBar current={total - available} max={total} label={`${available}/${total} free`} />
      )}
      <div className="card-row">
        <WaiterBadge count={waiters} />
        {longestWaitNs != null && longestWaitNs > 0 && (
          <>
            <span className="card-dim">wait</span>
            <DurationBadge ns={longestWaitNs} warnNs={100_000_000} critNs={1_000_000_000} />
          </>
        )}
      </div>
    </CardShell>
  );
}

function OnceCellCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const state = attr(attrs, "state") ?? "unset";
  const initNs = numAttr(attrs, "init_duration_ns");
  const waiters = numAttr(attrs, "waiters") ?? 0;

  const stateVariant: "ok" | "warn" | "crit" | "neutral" =
    state === "set" ? "ok" : state === "initializing" ? "warn" : "neutral";

  return (
    <CardShell
      kind="oncecell"
      process={process}
      label={label}
      icon={<ToggleRight size={14} weight="bold" />}
    >
      <div className="card-row">
        <StatePill state={state.toUpperCase()} variant={stateVariant} />
        <WaiterBadge count={waiters} />
      </div>
      {initNs != null && state === "initializing" && (
        <div className="card-row">
          <span className="card-dim">init</span>
          <DurationBadge ns={initNs} warnNs={1_000_000_000} critNs={5_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function RequestCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const method = firstAttr(attrs, ["method", "request.method"]) ?? label;
  const elapsedNs = firstNumAttr(attrs, ["elapsed_ns", "request.elapsed_ns"]);
  const status = firstAttr(attrs, ["status", "request.status"]) ?? "in_flight";

  const statusVariant: "ok" | "warn" | "crit" | "neutral" =
    status === "completed" ? "ok" : status === "timed_out" ? "crit" : "neutral";

  return (
    <CardShell
      kind="request"
      process={process}
      label={method}
      icon={<PaperPlaneTilt size={14} weight="bold" />}
    >
      <div className="card-row">
        <StatePill state={status.toUpperCase()} variant={statusVariant} />
        {elapsedNs != null && (
          <DurationBadge ns={elapsedNs} warnNs={1_000_000_000} critNs={10_000_000_000} />
        )}
      </div>
      <div className="card-row">
        <span className="card-dim card-process">{process}</span>
      </div>
    </CardShell>
  );
}

function ResponseCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const status = firstAttr(attrs, ["status", "response.status"]) ?? "in_flight";
  const correlationKey = firstAttr(attrs, ["correlation_key", "response.correlation_key", "request.correlation_key"]);
  const elapsedNs = firstNumAttr(attrs, ["elapsed_ns", "response.elapsed_ns"]);

  const statusVariant: "ok" | "warn" | "crit" | "neutral" =
    status === "completed" ? "ok" : "neutral";

  return (
    <CardShell
      kind="response"
      process={process}
      label={label}
      icon={<ArrowBendDownLeft size={14} weight="bold" />}
    >
      <div className="card-row">
        <StatePill state={status.toUpperCase()} variant={statusVariant} />
        {elapsedNs != null && (
          <DurationBadge ns={elapsedNs} warnNs={1_000_000_000} critNs={10_000_000_000} />
        )}
      </div>
      {correlationKey && (
        <div className="card-row">
          <span className="card-dim" title={correlationKey}>
            {correlationKey.length > 16 ? correlationKey.slice(0, 16) + "…" : correlationKey}
          </span>
        </div>
      )}
    </CardShell>
  );
}

function NetWaitCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, kind, process, attrs } = data;
  const op = attr(attrs, "net.op") ?? kind;
  const endpoint = attr(attrs, "net.endpoint") ?? "";
  const transport = attr(attrs, "net.transport") ?? "";
  const elapsedNs = numAttr(attrs, "elapsed_ns");

  const iconMap: Record<string, React.ReactNode> = {
    net_connect: <Plugs size={14} weight="bold" />,
    net_accept: <WifiHigh size={14} weight="bold" />,
    net_readable: <ArrowFatLineLeft size={14} weight="bold" />,
    net_writable: <ArrowFatLineRight size={14} weight="bold" />,
  };

  const displayOp = op.replace("net_", "").toUpperCase();

  return (
    <CardShell
      kind={kind}
      process={process}
      label={endpoint || label}
      icon={iconMap[kind] ?? <Plugs size={14} weight="bold" />}
    >
      <div className="card-row">
        <StatePill state={displayOp} variant="neutral" />
        <span className="badge">{transport}</span>
      </div>
      {elapsedNs != null && (
        <div className="card-row">
          <span className="card-dim">waited</span>
          <DurationBadge ns={elapsedNs} warnNs={1_000_000_000} critNs={5_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function GhostCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, attrs } = data;
  const reason = attr(attrs, "reason") ?? "unresolved";
  const refProcKey = attr(attrs, "referenced_proc_key");

  return (
    <div className="card card--ghost">
      <Handle type="target" position={Position.Top} style={{ visibility: "hidden" }} />
      <div className="card-head">
        <span className="card-icon"><Ghost size={14} weight="bold" /></span>
        <span className="card-label" title={label}>{label}</span>
      </div>
      <div className="card-body">
        <div className="card-row">
          <StatePill state="GHOST" variant="neutral" />
        </div>
        <div className="card-row">
          <span className="card-dim">reason</span>
          <span className="card-val-truncate">{reason}</span>
        </div>
        {refProcKey && (
          <div className="card-row">
            <span className="card-dim">proc</span>
            <span className="card-val-truncate">{refProcKey}</span>
          </div>
        )}
      </div>
      <Handle type="source" position={Position.Bottom} style={{ visibility: "hidden" }} />
    </div>
  );
}

function JoinSetCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const cancelled = attr(attrs, "cancelled");
  const closeCause = attr(attrs, "close_cause");
  const isCancelled = cancelled === "true";

  return (
    <CardShell
      kind="joinset"
      process={process}
      label={label}
      icon={<Stack size={14} weight="bold" />}
      stateClass={isCancelled ? "card--danger-border" : undefined}
    >
      <div className="card-row">
        <StatePill
          state={isCancelled ? "ABORTED" : "ACTIVE"}
          variant={isCancelled ? "crit" : "ok"}
        />
      </div>
      {closeCause && (
        <div className="card-row">
          <span className="card-dim">cause</span>
          <span className="card-val-truncate">{closeCause}</span>
        </div>
      )}
    </CardShell>
  );
}

function CommandCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const program = attr(attrs, "cmd.program") ?? label;
  const argsPreview = attr(attrs, "cmd.args_preview");
  const pid = numAttr(attrs, "process.pid");
  const exitCode = numAttr(attrs, "exit.code");
  const exitSignal = attr(attrs, "exit.signal");
  const elapsedNs = numAttr(attrs, "elapsed_ns");
  const error = attr(attrs, "error");

  const isRunning = exitCode == null && exitSignal == null && error == null;
  const isFailed = exitCode != null && exitCode !== 0;

  return (
    <CardShell
      kind="command"
      process={process}
      label={program}
      icon={<Terminal size={14} weight="bold" />}
      stateClass={error ? "card--danger-border" : isFailed ? "card--danger-border" : undefined}
    >
      {argsPreview && (
        <div className="card-row">
          <span className="card-dim card-val-truncate" title={argsPreview}>{argsPreview}</span>
        </div>
      )}
      <div className="card-row">
        <StatePill
          state={error ? "ERROR" : isRunning ? "RUNNING" : exitCode === 0 ? "OK" : `EXIT ${exitCode ?? exitSignal}`}
          variant={error ? "crit" : isRunning ? "neutral" : exitCode === 0 ? "ok" : "crit"}
        />
        {pid != null && <span className="badge">pid {pid}</span>}
      </div>
      {elapsedNs != null && (
        <div className="card-row">
          <span className="card-dim">elapsed</span>
          <DurationBadge ns={elapsedNs} warnNs={5_000_000_000} critNs={30_000_000_000} />
        </div>
      )}
      {error && (
        <div className="card-row">
          <span className="card-dim card-val-truncate" title={error}>{error}</span>
        </div>
      )}
    </CardShell>
  );
}

function FileOpCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const fsOp = attr(attrs, "fs.op") ?? label;
  const path = attr(attrs, "resource.path");
  const writeBytes = numAttr(attrs, "write.bytes");
  const readBytes = numAttr(attrs, "read.bytes");
  const elapsedNs = numAttr(attrs, "elapsed_ns");
  const result = attr(attrs, "result") ?? "pending";
  const error = attr(attrs, "error");

  const resultVariant: "ok" | "warn" | "crit" | "neutral" =
    error ? "crit" : result === "ok" ? "ok" : "neutral";

  return (
    <CardShell
      kind="file_op"
      process={process}
      label={path ?? fsOp}
      icon={<FileText size={14} weight="bold" />}
      stateClass={error ? "card--danger-border" : undefined}
    >
      <div className="card-row">
        <StatePill state={fsOp.toUpperCase()} variant={resultVariant} />
        {readBytes != null && <span className="badge">{readBytes}B read</span>}
        {writeBytes != null && <span className="badge">{writeBytes}B written</span>}
      </div>
      {elapsedNs != null && (
        <div className="card-row">
          <span className="card-dim">elapsed</span>
          <DurationBadge ns={elapsedNs} warnNs={100_000_000} critNs={1_000_000_000} />
        </div>
      )}
      {error && (
        <div className="card-row">
          <span className="card-dim card-val-truncate" title={error}>{error}</span>
        </div>
      )}
    </CardShell>
  );
}

function NotifyCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const waiters = numAttr(attrs, "waiters") ?? 0;
  const notifyCount = numAttr(attrs, "notify_count") ?? 0;
  const wakeupCount = numAttr(attrs, "wakeup_count") ?? 0;
  const oldestWaitNs = numAttr(attrs, "oldest_wait_ns");
  const highWatermark = numAttr(attrs, "high_waiters_watermark");

  return (
    <CardShell
      kind="notify"
      process={process}
      label={label}
      icon={<Bell size={14} weight="bold" />}
      stateClass={waiters > 0 ? "card--danger-border" : undefined}
    >
      <div className="card-row">
        <WaiterBadge count={waiters} />
        <span className="badge">{notifyCount} notified</span>
        <span className="badge">{wakeupCount} woken</span>
      </div>
      {oldestWaitNs != null && oldestWaitNs > 0 && (
        <div className="card-row">
          <span className="card-dim">oldest wait</span>
          <DurationBadge ns={oldestWaitNs} warnNs={1_000_000_000} critNs={5_000_000_000} />
        </div>
      )}
      {highWatermark != null && highWatermark > 0 && (
        <div className="card-row">
          <span className="card-dim">peak waiters</span>
          <span className="badge">{highWatermark}</span>
        </div>
      )}
    </CardShell>
  );
}

function SleepCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const durationMs = attr(attrs, "sleep.duration_ms");
  const elapsedNs = firstNumAttr(attrs, ["elapsed_ns", "total_pending_ns"]);

  return (
    <CardShell
      kind="sleep"
      process={process}
      label={label}
      icon={<Moon size={14} weight="bold" />}
    >
      {durationMs != null && (
        <div className="card-row">
          <span className="card-dim">duration</span>
          <span className="badge">{durationMs}ms</span>
        </div>
      )}
      {elapsedNs != null && (
        <div className="card-row">
          <span className="card-dim">elapsed</span>
          <DurationBadge ns={elapsedNs} warnNs={5_000_000_000} critNs={30_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function IntervalCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const periodMs = numAttr(attrs, "period_ms");
  const tickCount = numAttr(attrs, "tick_count") ?? 0;
  const elapsedNs = numAttr(attrs, "elapsed_ns");

  return (
    <CardShell
      kind="interval"
      process={process}
      label={label}
      icon={<Repeat size={14} weight="bold" />}
    >
      <div className="card-row">
        {periodMs != null && <span className="badge">{periodMs}ms period</span>}
        <span className="badge">{tickCount} ticks</span>
      </div>
      {elapsedNs != null && (
        <div className="card-row">
          <span className="card-dim">elapsed</span>
          <DurationBadge ns={elapsedNs} warnNs={60_000_000_000} critNs={300_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

function TimeoutCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, process, attrs } = data;
  const durationMs = attr(attrs, "timeout.duration_ms");
  const elapsedNs = firstNumAttr(attrs, ["elapsed_ns", "total_pending_ns"]);
  const state = attr(attrs, "state");

  const stateVariant: "ok" | "warn" | "crit" | "neutral" =
    state === "completed" ? "ok" : state === "timed_out" ? "crit" : "neutral";

  return (
    <CardShell
      kind="timeout"
      process={process}
      label={label}
      icon={<HourglassSimple size={14} weight="bold" />}
    >
      {durationMs != null && (
        <div className="card-row">
          <span className="card-dim">limit</span>
          <span className="badge">{durationMs}ms</span>
        </div>
      )}
      {state != null && (
        <div className="card-row">
          <StatePill state={state.toUpperCase()} variant={stateVariant} />
        </div>
      )}
      {elapsedNs != null && (
        <div className="card-row">
          <span className="card-dim">elapsed</span>
          <DurationBadge ns={elapsedNs} warnNs={1_000_000_000} critNs={5_000_000_000} />
        </div>
      )}
    </CardShell>
  );
}

/** Fallback for unknown node kinds */
function GenericCard({ data }: NodeProps<Node<NodeData>>) {
  const { label, kind, process, attrs } = data;
  const entries = Object.entries(attrs).filter(([, v]) => v != null).slice(0, 3);

  return (
    <CardShell
      kind={kind}
      process={process}
      label={label ?? kind}
      icon={<Gauge size={14} weight="bold" />}
    >
      {entries.map(([k, v]) => (
        <div key={k} className="card-row">
          <span className="card-dim">{k}</span>
          <span className="card-val-truncate">{String(v)}</span>
        </div>
      ))}
    </CardShell>
  );
}

// ── Dispatch ─────────────────────────────────────────────────

const cardByKind: Record<string, (props: NodeProps<Node<NodeData>>) => React.ReactNode> = {
  future: FutureCard,
  lock: MutexCard,
  mutex: MutexCard,
  rwlock: RwLockCard,
  tx: ChannelTxCard,
  rx: ChannelRxCard,
  channel_tx: ChannelTxCard,
  channel_rx: ChannelRxCard,
  mpsc_tx: ChannelTxCard,
  mpsc_rx: ChannelRxCard,
  remote_tx: ChannelTxCard,
  remote_rx: ChannelRxCard,
  oneshot: OneshotCard,
  oneshot_tx: OneshotCard,
  oneshot_rx: OneshotCard,
  watch: WatchCard,
  watch_tx: WatchCard,
  watch_rx: WatchCard,
  semaphore: SemaphoreCard,
  oncecell: OnceCellCard,
  once_cell: OnceCellCard,
  request: RequestCard,
  response: ResponseCard,
  net_connect: NetWaitCard,
  net_accept: NetWaitCard,
  net_readable: NetWaitCard,
  net_writable: NetWaitCard,
  ghost: GhostCard,
  joinset: JoinSetCard,
  command: CommandCard,
  file_op: FileOpCard,
  notify: NotifyCard,
  sleep: SleepCard,
  interval: IntervalCard,
  timeout: TimeoutCard,
};

/** Icon + human-readable name for each node kind (used by the filter dropdown). */
export const kindMeta: Record<string, { icon: React.ReactNode; displayName: string }> = {
  future:    { icon: <Timer size={14} weight="bold" />,            displayName: "Future" },
  lock:      { icon: <Lock size={14} weight="bold" />,             displayName: "Mutex" },
  mutex:     { icon: <Lock size={14} weight="bold" />,             displayName: "Mutex" },
  rwlock:    { icon: <LockOpen size={14} weight="bold" />,         displayName: "RwLock" },
  tx:        { icon: <ArrowLineUp size={14} weight="bold" />,      displayName: "Channel Tx" },
  rx:        { icon: <ArrowLineDown size={14} weight="bold" />,    displayName: "Channel Rx" },
  channel_tx:{ icon: <ArrowLineUp size={14} weight="bold" />,      displayName: "Channel Tx" },
  channel_rx:{ icon: <ArrowLineDown size={14} weight="bold" />,    displayName: "Channel Rx" },
  mpsc_tx:   { icon: <ArrowLineUp size={14} weight="bold" />,      displayName: "MPSC Tx" },
  mpsc_rx:   { icon: <ArrowLineDown size={14} weight="bold" />,    displayName: "MPSC Rx" },
  remote_tx: { icon: <ArrowLineUp size={14} weight="bold" />,      displayName: "Remote Tx" },
  remote_rx: { icon: <ArrowLineDown size={14} weight="bold" />,    displayName: "Remote Rx" },
  oneshot:   { icon: <ToggleRight size={14} weight="bold" />,      displayName: "Oneshot" },
  oneshot_tx:{ icon: <ToggleRight size={14} weight="bold" />,      displayName: "Oneshot Tx" },
  oneshot_rx:{ icon: <ToggleRight size={14} weight="bold" />,      displayName: "Oneshot Rx" },
  watch:     { icon: <Eye size={14} weight="bold" />,              displayName: "Watch" },
  watch_tx:  { icon: <Eye size={14} weight="bold" />,              displayName: "Watch Tx" },
  watch_rx:  { icon: <Eye size={14} weight="bold" />,              displayName: "Watch Rx" },
  semaphore: { icon: <Gauge size={14} weight="bold" />,            displayName: "Semaphore" },
  oncecell:  { icon: <ToggleRight size={14} weight="bold" />,      displayName: "OnceCell" },
  once_cell: { icon: <ToggleRight size={14} weight="bold" />,      displayName: "OnceCell" },
  request:   { icon: <PaperPlaneTilt size={14} weight="bold" />,   displayName: "Request" },
  response:     { icon: <ArrowBendDownLeft size={14} weight="bold" />,displayName: "Response" },
  net_connect:  { icon: <Plugs size={14} weight="bold" />,           displayName: "Connect" },
  net_accept:   { icon: <WifiHigh size={14} weight="bold" />,        displayName: "Accept" },
  net_readable: { icon: <ArrowFatLineLeft size={14} weight="bold" />,displayName: "Readable" },
  net_writable: { icon: <ArrowFatLineRight size={14} weight="bold" />,displayName: "Writable" },
  ghost:        { icon: <Ghost size={14} weight="bold" />,           displayName: "Ghost" },
  joinset:      { icon: <Stack size={14} weight="bold" />,          displayName: "JoinSet" },
  command:      { icon: <Terminal size={14} weight="bold" />,       displayName: "Command" },
  file_op:      { icon: <FileText size={14} weight="bold" />,      displayName: "File Op" },
  notify:       { icon: <Bell size={14} weight="bold" />,           displayName: "Notify" },
  sleep:        { icon: <Moon size={14} weight="bold" />,           displayName: "Sleep" },
  interval:     { icon: <Repeat size={14} weight="bold" />,        displayName: "Interval" },
  timeout:      { icon: <HourglassSimple size={14} weight="bold" />,displayName: "Timeout" },
};

export const PeepsNode = memo((props: NodeProps<Node<NodeData>>) => {
  const kind = props.data.kind;
  const Card = cardByKind[kind] ?? GenericCard;
  return <>{Card(props)}</>;
});

/** Estimate node height for ELK layout based on kind */
export function estimateNodeHeight(kind: string): number {
  switch (kind) {
    case "request":
    case "response":
      return 120;
    case "tx":
    case "channel_tx":
    case "mpsc_tx":
    case "remote_tx":
      return 120;
    case "lock":
    case "mutex":
    case "rwlock":
      return 110;
    case "semaphore":
      return 110;
    case "future":
      return 100;
    case "rx":
    case "channel_rx":
    case "mpsc_rx":
    case "remote_rx":
      return 100;
    case "oncecell":
    case "once_cell":
      return 90;
    case "net_connect":
    case "net_accept":
    case "net_readable":
    case "net_writable":
      return 100;
    case "ghost":
      return 100;
    case "joinset":
      return 90;
    case "command":
      return 130;
    case "file_op":
      return 110;
    case "notify":
      return 110;
    case "sleep":
      return 90;
    case "interval":
      return 100;
    case "timeout":
      return 100;
    default:
      return 100;
  }
}
