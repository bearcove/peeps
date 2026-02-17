import { useMemo, useState } from "react";
import {
  Check,
  CircleNotch,
  CopySimple,
  CaretDown,
  CaretLeft,
  CaretRight,
  Plugs,
} from "@phosphor-icons/react";
import { isResourceKind } from "../resourceKinds";
import type { SnapshotGraph, SnapshotProcessInfo, ProcessDebugResponse } from "../types";
import { requestProcessDebug } from "../api";

type SortKey = "health" | "connection" | "pending" | "last_recv" | "last_sent";
type SortDir = "asc" | "desc";
type Health = "healthy" | "warning" | "critical";
type SeverityFilter = "all" | "warning_plus" | "critical";

const WARN_PENDING = 10;
const CRIT_PENDING = 25;
const WARN_STALE_NS = 15_000_000_000;
const CRIT_STALE_NS = 60_000_000_000;

interface ResourcesPanelProps {
  graph: SnapshotGraph | null;
  snapshotCapturedAtNs: number | null;
  snapshotId: number | null;
  snapshotProcesses: SnapshotProcessInfo[];
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
  collapsed: boolean;
  onToggleCollapse: () => void;
  processFilter?: string | null;
  onClearProcessFilter?: () => void;
  fullHeight?: boolean;
  allowCollapse?: boolean;
}

interface DuplexLegRow {
  nodeId: string;
  connectionToken: string;
  pendingRefsKey: string;
  legLabel: string;
  directionFrom: string;
  directionTo: string;
  process: string;
  procKey: string;
  state: string;
  pendingRequests: number;
  pendingResponses: number;
  pendingRequestIds: string[];
  pendingResponseIds: string[];
  pid: number | null;
  snapshotStatus: string;
  command: string | null;
  cmdArgsPreview: string | null;
  errorText: string | null;
  lastRecvAgeNs: number | null;
  lastSentAgeNs: number | null;
  health: Health;
  isMissing: boolean;
}

interface DuplexRow {
  key: string;
  duplexLabel: string;
  endpointA: string;
  endpointB: string;
  health: Health;
  pendingRequests: number;
  pendingResponses: number;
  pendingTotal: number;
  lastRecvAgeNs: number | null;
  lastSentAgeNs: number | null;
  legs: DuplexLegRow[];
}

interface ResourcesPayload {
  capturedAtNs: number | null;
  summary: {
    total: number;
    warningCount: number;
    criticalCount: number;
  };
  filters: {
    severity: SeverityFilter;
  };
  sort: {
    key: SortKey;
    dir: SortDir;
  };
  rows: DuplexRow[];
  visibleRows: DuplexRow[];
}

interface PendingNodeRefs {
  requestIds: string[];
  responseIds: string[];
}

const ARROW_RIGHT = " \u2192 ";
const ARROW_BIDIR_LABEL = " \u21cc ";
const DASH = "—";
const PROCESS_STATUS_UNKNOWN = "unknown";

function firstString(attrs: Record<string, unknown>, keys: string[]): string | undefined {
  for (const k of keys) {
    const v = attrs[k];
    if (v != null && v !== "") return String(v);
  }
  return undefined;
}

function firstNumber(attrs: Record<string, unknown>, keys: string[]): number | undefined {
  for (const k of keys) {
    const v = attrs[k];
    if (v == null || v === "") continue;
    const n = Number(v);
    if (!Number.isNaN(n)) return n;
  }
  return undefined;
}

function parseDuplexPair(value: string): [string, string] | null {
  const normalized = value
    .replace(/\u21cc/g, "<->")
    .replace(/\u21d4/g, "<->")
    .replace(/\s+<->\s+/g, "<->");
  const parts = normalized.split("<->").map((part) => part.trim());
  if (parts.length !== 2) return null;
  const [left, right] = parts;
  if (!left || !right) return null;
  return left < right ? [left, right] : [right, left];
}

function parseDirectionalConnectionToken(raw: string): {
  src: string;
  dst: string;
  link: string;
} {
  const token = raw.trim();
  const withPrefixRemoved = token.startsWith("connection:") ? token.slice("connection:".length) : token;
  const [left, right] = withPrefixRemoved.split(":", 2);
  if (left && right) {
    const arrowIndex = left.indexOf("->");
    if (arrowIndex > 0) {
      return {
        src: left.slice(0, arrowIndex).trim(),
        dst: left.slice(arrowIndex + 2).trim(),
        link: right.trim(),
      };
    }
  }
  return {
    src: "",
    dst: "",
    link: right ? right.trim() : withPrefixRemoved,
  };
}

function connectionToken(nodeId: string, attrs: Record<string, unknown>): string {
  const raw = firstString(attrs, ["connection.id", "rpc.connection", "connection"]) ??
    (nodeId.startsWith("connection:") ? nodeId.slice("connection:".length) : nodeId);
  return raw?.trim() || nodeId;
}

function pendingRefsKey(token: string, src: string, dst: string): string {
  if (src && dst) return `${token}|${src}->${dst}`;
  return token;
}

function resolveConnectionIdentity(nodeId: string, attrs: Record<string, unknown>) {
  const token = connectionToken(nodeId, attrs);
  const parsed = parseDirectionalConnectionToken(token);
  const srcFromAttr = firstString(attrs, ["connection.src"]);
  const dstFromAttr = firstString(attrs, ["connection.dst"]);
  const linkFromAttr = firstString(attrs, ["connection.link"]);

  const src = (srcFromAttr ?? parsed.src).trim() || "unknown";
  const dst = (dstFromAttr ?? parsed.dst).trim() || "unknown";
  const link = (linkFromAttr ?? parsed.link).trim() || `${src} <-> ${dst}`;
  const [left, right] = parseDuplexPair(link) ?? parseDuplexPair(`${src} <-> ${dst}`) ?? ["unknown", "unknown"];

  return {
    token,
    src,
    dst,
    endpointA: left,
    endpointB: right,
    duplexKey: `${left}<->${right}`,
    duplexLabel: `${left}${ARROW_BIDIR_LABEL}${right}`,
    legLabel: `${src}${ARROW_RIGHT}${dst}`,
    pendingRefsKey: pendingRefsKey(token, src, dst),
  };
}

function formatAge(ageNs: number | null): string {
  if (ageNs == null) return DASH;
  if (ageNs < 1_000_000) return `${Math.round(ageNs / 1_000)}us ago`;
  if (ageNs < 1_000_000_000) return `${Math.round(ageNs / 1_000_000)}ms ago`;
  const seconds = ageNs / 1_000_000_000;
  if (seconds < 60) return `${seconds.toFixed(1)}s ago`;
  return `${(seconds / 60).toFixed(1)}m ago`;
}

function toAgeNs(snapshotCapturedAtNs: number | null, tsNs: number | undefined): number | null {
  if (snapshotCapturedAtNs == null || tsNs == null) return null;
  if (!Number.isFinite(snapshotCapturedAtNs) || !Number.isFinite(tsNs)) return null;
  return Math.max(0, snapshotCapturedAtNs - tsNs);
}

function connectionState(attrs: Record<string, unknown>): string {
  const state = firstString(attrs, ["connection.state", "state"]);
  if (state === "open" || state === "closed") return state;
  return "unknown";
}

function connectionHealth(
  pendingRequests: number,
  pendingResponses: number,
  lastRecvAgeNs: number | null,
): Health {
  const pending = Math.max(pendingRequests, pendingResponses);
  if (pending >= CRIT_PENDING) return "critical";
  if (pending >= WARN_PENDING) return "warning";
  if ((lastRecvAgeNs ?? -1) >= CRIT_STALE_NS) return "critical";
  if ((lastRecvAgeNs ?? -1) >= WARN_STALE_NS) return "warning";
  return "healthy";
}

function isRequestPendingStatus(status: string | undefined): boolean {
  const normalized = status?.trim().toLowerCase() ?? "";
  return !["completed", "timed_out"].includes(normalized);
}

function isResponsePendingStatus(status: string | undefined): boolean {
  const normalized = status?.trim().toLowerCase() ?? "";
  return !["completed", "delivered", "cancelled"].includes(normalized);
}

function toSnapshotStatus(value: string | undefined): string {
  if (!value) return PROCESS_STATUS_UNKNOWN;
  const normalized = value.trim().toLowerCase();
  if (normalized.length === 0) return PROCESS_STATUS_UNKNOWN;
  return normalized;
}

function collectPendingNodeRefs(graph: SnapshotGraph | null): Map<string, PendingNodeRefs> {
  const map = new Map<string, PendingNodeRefs>();
  if (!graph) return map;

  for (const node of graph.nodes) {
    if (node.kind !== "request" && node.kind !== "response") continue;
    const connectionToken = firstString(node.attrs, ["rpc.connection", "connection.id", "connection"]);
    if (!connectionToken) continue;
    const key = connectionToken.trim();
    const src = firstString(node.attrs, ["connection.src"])?.trim() ?? "";
    const dst = firstString(node.attrs, ["connection.dst"])?.trim() ?? "";
    const directionalKey = pendingRefsKey(key, src, dst);

    const entry = map.get(key) ?? { requestIds: [], responseIds: [] };
    const directionalEntry = map.get(directionalKey) ?? { requestIds: [], responseIds: [] };
    const status = firstString(node.attrs, ["status"])?.toLowerCase();

    if (node.kind === "request" && isRequestPendingStatus(status)) {
      entry.requestIds.push(node.id);
      if (directionalKey !== key) {
        directionalEntry.requestIds.push(node.id);
      }
    }
    if (node.kind === "response" && isResponsePendingStatus(status)) {
      entry.responseIds.push(node.id);
      if (directionalKey !== key) {
        directionalEntry.responseIds.push(node.id);
      }
    }

    map.set(key, entry);
    map.set(directionalKey, directionalEntry);
  }

  return map;
}

function buildProcessLookup(processes: SnapshotProcessInfo[]) {
  const byProcKey = new Map<string, SnapshotProcessInfo>();
  const byProcessName = new Map<string, SnapshotProcessInfo>();
  for (const proc of processes) {
    if (!byProcKey.has(proc.proc_key)) {
      byProcKey.set(proc.proc_key, proc);
    }
    if (!byProcessName.has(proc.process)) {
      byProcessName.set(proc.process, proc);
    }
  }
  return { byProcKey, byProcessName };
}

function getProcessInfo(
  lookup: ReturnType<typeof buildProcessLookup>,
  processName: string,
  procKey: string,
): SnapshotProcessInfo | undefined {
  if (procKey) {
    const byKey = lookup.byProcKey.get(procKey);
    if (byKey) return byKey;
  }
  if (processName) {
    return lookup.byProcessName.get(processName);
  }
  return undefined;
}

function healthRank(health: Health): number {
  if (health === "critical") return 2;
  if (health === "warning") return 1;
  return 0;
}

function isSevere(row: DuplexRow, filter: SeverityFilter): boolean {
  if (filter === "all") return true;
  if (filter === "critical") return row.health === "critical";
  return row.health === "critical" || row.health === "warning";
}

function sortRows(rows: DuplexRow[], key: SortKey, dir: SortDir): DuplexRow[] {
  const sign = dir === "asc" ? 1 : -1;
  const sorted = [...rows];
  sorted.sort((a, b) => {
    const cmpNumber = (av: number | null, bv: number | null, missingLast: boolean): number => {
      if (av == null && bv == null) return 0;
      if (av == null) return missingLast ? 1 : -1;
      if (bv == null) return missingLast ? -1 : 1;
      return av - bv;
    };

    let primary = 0;
    if (key === "health") primary = healthRank(a.health) - healthRank(b.health);
    if (key === "connection") primary = a.duplexLabel.localeCompare(b.duplexLabel);
    if (key === "pending") primary = cmpNumber(a.pendingTotal, b.pendingTotal, true);
    if (key === "last_recv")
      primary = cmpNumber(a.lastRecvAgeNs, b.lastRecvAgeNs, true);
    if (key === "last_sent")
      primary = cmpNumber(a.lastSentAgeNs, b.lastSentAgeNs, true);

    if (primary !== 0) return primary * sign;

    if (key === "pending") {
      const byPendingAge = cmpNumber(a.lastRecvAgeNs, b.lastRecvAgeNs, true);
      if (byPendingAge !== 0) return byPendingAge * -1;
    }
    if (a.key !== b.key) return a.key.localeCompare(b.key);
    return 0;
  });
  return sorted;
}

export function ResourcesPanel({
  graph,
  snapshotCapturedAtNs,
  snapshotId,
  snapshotProcesses,
  selectedNodeId,
  onSelectNode,
  collapsed,
  onToggleCollapse,
  processFilter = null,
  onClearProcessFilter,
  fullHeight = false,
  allowCollapse = true,
}: ResourcesPanelProps) {
  const [sortKey, setSortKey] = useState<SortKey>("pending");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [severityFilter, setSeverityFilter] = useState<SeverityFilter>("all");
  const [copiedResources, setCopiedResources] = useState(false);
  const [debugMessage, setDebugMessage] = useState<string | null>(null);
  const [pendingCursor, setPendingCursor] = useState<Record<string, number>>({});
  const [runningDebugActions, setRunningDebugActions] = useState<Set<string>>(new Set());
  const [debugResultUrls, setDebugResultUrls] = useState<Record<string, string>>({});

  const pendingNodeRefs = useMemo(() => collectPendingNodeRefs(graph), [graph]);
  const processLookup = useMemo(() => buildProcessLookup(snapshotProcesses), [snapshotProcesses]);

  const rows = useMemo(() => {
    if (!graph) return [] as DuplexRow[];

    const grouped = new Map<string, DuplexRow>();

    const connectionNodes = graph.nodes.filter((node) => node.kind === "connection" && isResourceKind(node.kind));
    for (const node of connectionNodes) {
      const identity = resolveConnectionIdentity(node.id, node.attrs);
      if (connectionState(node.attrs) === "closed") continue;
      const refs =
        pendingNodeRefs.get(identity.pendingRefsKey) ??
        pendingNodeRefs.get(identity.token) ??
        { requestIds: [], responseIds: [] };

      const pendingRequests = firstNumber(node.attrs, [
        "connection.pending_requests_outgoing",
        "pending_requests_outgoing",
        "connection.pending_requests",
        "pending_requests",
        "pending",
        "connection.pending",
      ]);
      const pendingResponses = firstNumber(node.attrs, [
        "connection.pending_responses",
        "pending_responses",
      ]);

      const legPendingRequests = pendingRequests ?? refs.requestIds.length;
      const legPendingResponses = pendingResponses ?? refs.responseIds.length;

      const processInfo = getProcessInfo(processLookup, node.process, node.proc_key);
      const lastRecvTsNs = firstNumber(node.attrs, [
        "connection.last_frame_recv_at_ns",
        "last_frame_recv_at_ns",
        "connection.last_received_at_ns",
        "last_received_at_ns",
        "connection.last_recv_at_ns",
        "last_recv_at_ns",
      ]);
      const lastSentTsNs = firstNumber(node.attrs, [
        "connection.last_frame_sent_at_ns",
        "last_frame_sent_at_ns",
        "connection.last_sent_at_ns",
        "last_sent_at_ns",
        "connection.last_transmit_at_ns",
        "last_transmit_at_ns",
      ]);
      const lastRecvAgeNs = toAgeNs(snapshotCapturedAtNs, lastRecvTsNs);
      const lastSentAgeNs = toAgeNs(snapshotCapturedAtNs, lastSentTsNs);

      const legRow: DuplexLegRow = {
        nodeId: node.id,
        connectionToken: identity.token,
        pendingRefsKey: identity.pendingRefsKey,
        legLabel: identity.legLabel,
        directionFrom: identity.src,
        directionTo: identity.dst,
        process: node.process,
        procKey: node.proc_key,
        state: connectionState(node.attrs),
        pendingRequests: legPendingRequests,
        pendingResponses: legPendingResponses,
        pendingRequestIds: refs.requestIds,
        pendingResponseIds: refs.responseIds,
        pid: processInfo?.pid ?? null,
        snapshotStatus: toSnapshotStatus(processInfo?.status),
        command: processInfo?.command ?? null,
        cmdArgsPreview: processInfo?.cmd_args_preview ?? null,
        errorText: processInfo?.error_text ?? null,
        lastRecvAgeNs,
        lastSentAgeNs,
        health: connectionHealth(legPendingRequests, legPendingResponses, lastRecvAgeNs),
        isMissing: false,
      };

      const existing = grouped.get(identity.duplexKey);
      if (existing) {
        existing.legs.push(legRow);
      } else {
        grouped.set(identity.duplexKey, {
          key: identity.duplexKey,
          duplexLabel: identity.duplexLabel,
          endpointA: identity.endpointA,
          endpointB: identity.endpointB,
          health: legRow.health,
          pendingRequests: 0,
          pendingResponses: 0,
          pendingTotal: 0,
          lastRecvAgeNs: null,
          lastSentAgeNs: null,
          legs: [legRow],
        });
      }
    }

    const normalized = Array.from(grouped.values()).map((row) => {
      const expectedDirections = [
        { src: row.endpointA, dst: row.endpointB },
        { src: row.endpointB, dst: row.endpointA },
      ];

      const legByDirection = new Map<string, DuplexLegRow>();
      for (const leg of row.legs) {
        legByDirection.set(`${leg.directionFrom}->${leg.directionTo}`, leg);
      }

      for (const expected of expectedDirections) {
        const key = `${expected.src}->${expected.dst}`;
        if (legByDirection.has(key)) continue;
        const refs =
          pendingNodeRefs.get(pendingRefsKey(row.key, expected.src, expected.dst)) ??
          pendingNodeRefs.get(row.key) ?? { requestIds: [], responseIds: [] };
        const processInfo = getProcessInfo(processLookup, expected.src, "");
        const pendingRequests = refs.requestIds.length;
        const pendingResponses = refs.responseIds.length;
        legByDirection.set(key, {
          nodeId: `${row.key}:missing:${expected.src}->${expected.dst}`,
          connectionToken: row.key,
          pendingRefsKey: pendingRefsKey(row.key, expected.src, expected.dst),
          legLabel: `${expected.src}${ARROW_RIGHT}${expected.dst}`,
          directionFrom: expected.src,
          directionTo: expected.dst,
          process: expected.src,
          procKey: processInfo?.proc_key ?? "",
          state: "missing",
          pendingRequests,
          pendingResponses,
          pendingRequestIds: refs.requestIds,
          pendingResponseIds: refs.responseIds,
          pid: processInfo?.pid ?? null,
          snapshotStatus: toSnapshotStatus(processInfo?.status),
          command: processInfo?.command ?? null,
          cmdArgsPreview: processInfo?.cmd_args_preview ?? null,
          errorText: processInfo?.error_text ?? "missing connection leg",
          lastRecvAgeNs: toAgeNs(snapshotCapturedAtNs, undefined),
          lastSentAgeNs: toAgeNs(snapshotCapturedAtNs, undefined),
          health: "warning",
          isMissing: true,
        });
      }

      const orderedLegs = Array.from(legByDirection.values()).sort((a, b) =>
        a.directionFrom.localeCompare(b.directionFrom),
      );
      const pendingRequests = orderedLegs.reduce((sum, leg) => sum + leg.pendingRequests, 0);
      const pendingResponses = orderedLegs.reduce((sum, leg) => sum + leg.pendingResponses, 0);
      const pendingTotal = pendingRequests + pendingResponses;
      const lastRecvAgeNs = orderedLegs.reduce<number | null>((age, leg) => {
        if (leg.lastRecvAgeNs == null) return age;
        if (age == null) return leg.lastRecvAgeNs;
        return Math.max(age, leg.lastRecvAgeNs);
      }, null);
      const lastSentAgeNs = orderedLegs.reduce<number | null>((age, leg) => {
        if (leg.lastSentAgeNs == null) return age;
        if (age == null) return leg.lastSentAgeNs;
        return Math.max(age, leg.lastSentAgeNs);
      }, null);

      const health = orderedLegs.reduce<Health>((result, leg) => {
        if (healthRank(leg.health) > healthRank(result)) return leg.health;
        return result;
      }, orderedLegs[0]?.health ?? "healthy");

      return {
        ...row,
        health,
        pendingRequests,
        pendingResponses,
        pendingTotal,
        lastRecvAgeNs,
        lastSentAgeNs,
        legs: orderedLegs,
      };
    });

    return sortRows(normalized, sortKey, sortDir);
  }, [graph, sortDir, sortKey, snapshotCapturedAtNs, pendingNodeRefs, processLookup]);

  const visibleRows = useMemo(
    () =>
      rows.filter(
        (row) =>
          isSevere(row, severityFilter) &&
          (processFilter == null ||
            row.endpointA === processFilter ||
            row.endpointB === processFilter ||
            row.legs.some((leg) => leg.process === processFilter)),
      ),
    [processFilter, rows, severityFilter],
  );

  const summary = useMemo(() => {
    const warningCount = rows.filter((row) => row.health === "warning").length;
    const criticalCount = rows.filter((row) => row.health === "critical").length;
    return { total: rows.length, warningCount, criticalCount };
  }, [rows]);

  const resourcesPayload = useMemo<ResourcesPayload>(() => {
    return {
      capturedAtNs: snapshotCapturedAtNs,
      summary,
      filters: {
        severity: severityFilter,
      },
      sort: {
        key: sortKey,
        dir: sortDir,
      },
      rows,
      visibleRows,
    };
  }, [snapshotCapturedAtNs, summary, severityFilter, sortKey, sortDir, rows, visibleRows]);

  const resourcesJson = useMemo(() => JSON.stringify(resourcesPayload, null, 2), [resourcesPayload]);

  async function onCopyResources() {
    try {
      await navigator.clipboard.writeText(resourcesJson);
      setCopiedResources(true);
      window.setTimeout(() => setCopiedResources(false), 1200);
    } catch {
      setCopiedResources(false);
    }
  }

  function toggleSort(nextKey: SortKey) {
    if (sortKey === nextKey) {
      setSortDir((prev) => (prev === "asc" ? "desc" : "asc"));
      return;
    }
    setSortKey(nextKey);
    setSortDir(nextKey === "connection" ? "asc" : "desc");
  }

  function sortArrow(key: SortKey): string {
    if (sortKey !== key) return "";
    return sortDir === "asc" ? " \u2191" : " \u2193";
  }

  function onPendingCellClick(
    rowKey: string,
    requestIds: string[],
    responseIds: string[],
    type: "request" | "response",
  ) {
    const ids = type === "request" ? requestIds : responseIds;
    if (ids.length === 0) return;
    const next = ((pendingCursor[rowKey] ?? -1) + 1) % ids.length;
    setPendingCursor((prev) => ({ ...prev, [rowKey]: next }));
    onSelectNode(ids[next]);
  }

  async function onProcessDebugClick(
    action: "sample" | "spindump",
    leg: DuplexLegRow,
  ): Promise<ProcessDebugResponse | null> {
    const actionKey = `${leg.procKey}:${action}`;
    setRunningDebugActions((prev) => {
      const next = new Set(prev);
      next.add(actionKey);
      return next;
    });
    if (snapshotId == null) {
      setRunningDebugActions((prev) => {
        const next = new Set(prev);
        next.delete(actionKey);
        return next;
      });
      setDebugMessage("No active snapshot to run debug for");
      return null;
    }
    if (!leg.procKey) {
      setRunningDebugActions((prev) => {
        const next = new Set(prev);
        next.delete(actionKey);
        return next;
      });
      setDebugMessage(`Missing process key for ${leg.process || "unknown process"}`);
      return null;
    }
    if (leg.pid == null) {
      setRunningDebugActions((prev) => {
        const next = new Set(prev);
        next.delete(actionKey);
        return next;
      });
      setDebugMessage(`No PID available for ${leg.process || leg.procKey}`);
      return null;
    }
    try {
      const response = await requestProcessDebug(snapshotId, leg.procKey, action, true);
      if (response.result_url) {
        setDebugResultUrls((prev) => ({ ...prev, [actionKey]: response.result_url! }));
        setDebugMessage(`${action} output ready for ${response.process}`);
      } else {
        setDebugResultUrls((prev) => {
          if (!(actionKey in prev)) return prev;
          const next = { ...prev };
          delete next[actionKey];
          return next;
        });
        setDebugMessage(`${action} did not run for ${response.process}: ${response.status}`);
      }
      return response;
    } catch (err) {
      setDebugMessage(err instanceof Error ? `Debug command error: ${err.message}` : "Debug command failed");
      return null;
    } finally {
      setRunningDebugActions((prev) => {
        const next = new Set(prev);
        next.delete(actionKey);
        return next;
      });
    }
  }

  function clearDebugMessage() {
    window.setTimeout(() => setDebugMessage(null), 1600);
  }

  if (collapsed && allowCollapse) {
    return (
      <div className="panel panel--resources-collapsed">
        <button className="panel-collapse-btn" onClick={onToggleCollapse} title="Expand panel">
          <CaretRight size={14} weight="bold" />
        </button>
        <span className="resources-collapsed-label">Resources</span>
      </div>
    );
  }

  return (
    <div className={`panel panel--resources${fullHeight ? " panel--resources-full" : ""}`}>
      <div className="panel-header">
        <Plugs size={14} weight="bold" /> Resources ({summary.total})
        <button
          type="button"
          className="resources-copy-btn"
          onClick={onCopyResources}
          title={copiedResources ? "Copied resources JSON" : "Copy resources JSON"}
          aria-label="Copy resources JSON"
        >
          {copiedResources ? <Check size={12} weight="bold" /> : <CopySimple size={12} weight="bold" />}
          {copiedResources ? "Copied" : "Copy JSON"}
        </button>
        {allowCollapse && (
          <button className="panel-collapse-btn" onClick={onToggleCollapse} title="Collapse panel">
            <CaretLeft size={14} weight="bold" />
          </button>
        )}
      </div>

      <div className="resources-summary-row">
        <span className="resources-chip">total {summary.total}</span>
        <span className="resources-chip resources-chip--warn">warning {summary.warningCount}</span>
        <span className="resources-chip resources-chip--crit">critical {summary.criticalCount}</span>
        {debugMessage && <span className="resources-debug-message">{debugMessage}</span>}
      </div>

      {processFilter && (
        <div className="resources-filter-row resources-filter-row--process">
          <span className="resources-chip resources-chip--process">process {processFilter}</span>
          <button
            type="button"
            className="resources-filter-btn"
            onClick={onClearProcessFilter}
            disabled={!onClearProcessFilter}
          >
            Clear
          </button>
        </div>
      )}

      <div className="resources-filter-row">
        <button
          type="button"
          className={`resources-filter-btn${severityFilter === "all" ? " resources-filter-btn--active" : ""}`}
          onClick={() => setSeverityFilter("all")}
        >
          All
        </button>
        <button
          type="button"
          className={`resources-filter-btn${severityFilter === "warning_plus" ? " resources-filter-btn--active" : ""}`}
          onClick={() => setSeverityFilter("warning_plus")}
        >
          Warning+
        </button>
        <button
          type="button"
          className={`resources-filter-btn${severityFilter === "critical" ? " resources-filter-btn--active" : ""}`}
          onClick={() => setSeverityFilter("critical")}
        >
          Critical
        </button>
      </div>

      {rows.length === 0 ? (
        <div className="resources-empty">No connection resources in this snapshot.</div>
      ) : visibleRows.length === 0 ? (
        <div className="resources-empty">
          {processFilter
            ? `No connections match this filter for process ${processFilter}.`
            : "No connections match this health filter."}
        </div>
      ) : (
        <div className="resources-table-wrap">
          <div className="resources-row-grid resources-row-grid--header">
            <button type="button" className="resources-sort" onClick={() => toggleSort("health")}>
              Health{sortArrow("health")}
            </button>
            <button type="button" className="resources-sort" onClick={() => toggleSort("connection")}>
              Link{sortArrow("connection")}
            </button>
            <span>Issues / Details</span>
            <button type="button" className="resources-sort" onClick={() => toggleSort("pending")}>
              Req{sortArrow("pending")}
            </button>
            <span>Resp</span>
            <button type="button" className="resources-sort" onClick={() => toggleSort("last_recv")}>
              Last recv{sortArrow("last_recv")}
            </button>
            <button type="button" className="resources-sort" onClick={() => toggleSort("last_sent")}>
              Last sent{sortArrow("last_sent")}
            </button>
          </div>
          <div className="resources-duplex-card-list">
            {visibleRows.map((duplexRow) => {
              const isLegOrDuplexSelected = duplexRow.legs.some((leg) => leg.nodeId === selectedNodeId);
              const problematicLegs = duplexRow.legs
                .filter(
                  (leg) =>
                    leg.isMissing ||
                    (leg.snapshotStatus !== "responded" && leg.snapshotStatus !== PROCESS_STATUS_UNKNOWN),
                )
                .sort((a, b) => {
                  if (a.isMissing && !b.isMissing) return -1;
                  if (!a.isMissing && b.isMissing) return 1;
                  return a.process.localeCompare(b.process);
                });
              const summaryPendingRequestIds = Array.from(
                new Set(duplexRow.legs.flatMap((leg) => leg.pendingRequestIds)),
              );
              const summaryPendingResponseIds = Array.from(
                new Set(duplexRow.legs.flatMap((leg) => leg.pendingResponseIds)),
              );
              const requestSummaryKey = `${duplexRow.key}:summary:request`;
              const responseSummaryKey = `${duplexRow.key}:summary:response`;
              return (
                <div
                  key={`${duplexRow.key}:duplex`}
                  className={`resources-duplex-card ${isLegOrDuplexSelected ? "resources-row--selected" : ""}`}
                >
                  <div className="resources-row resources-row--duplex resources-row-grid">
                    <span>
                      <span
                        className={`resources-health-pill resources-health-pill--${
                          duplexRow.health === "critical"
                            ? "crit"
                            : duplexRow.health === "warning"
                              ? "warn"
                              : "ok"
                        }`}
                      >
                        {duplexRow.health}
                      </span>
                    </span>
                    <span className="resources-cell-mono resources-duplex-label">{duplexRow.duplexLabel}</span>
                    <div className="resources-cell-mono resources-duplex-summary">
                      {problematicLegs.length > 0 ? (
                        <div className="resources-topbar-processes">
                              {problematicLegs.map((leg) => {
                              const isProblematic =
                                leg.isMissing ||
                                (leg.snapshotStatus !== "responded" &&
                                  leg.snapshotStatus !== PROCESS_STATUS_UNKNOWN);
                              if (!isProblematic) return null;
                              const canInspect =
                                !leg.isMissing && leg.nodeId.startsWith("connection:");
                              const processLabel = leg.process || "unknown";
                              const chipLabel = `${leg.legLabel}${leg.isMissing ? " (missing)" : ""}`;
                              const sampleActionKey = `${leg.procKey}:sample`;
                              const spindumpActionKey = `${leg.procKey}:spindump`;
                              const sampleResultUrl = debugResultUrls[sampleActionKey];
                              const spindumpResultUrl = debugResultUrls[spindumpActionKey];
                              const sampleRunning = runningDebugActions.has(sampleActionKey);
                              const spindumpRunning = runningDebugActions.has(spindumpActionKey);
                              return (
                                <div
                                  key={`missing:${duplexRow.key}:${leg.nodeId}`}
                                  className={`resources-topbar-chip ${
                                    leg.isMissing ? "resources-topbar-chip--missing" : "resources-topbar-chip--warn"
                                  }`}
                                >
                                  <div className="resources-topbar-chip-main">
                                    {canInspect ? (
                                      <button
                                        type="button"
                                        className="resources-topbar-chip-label-btn"
                                        onClick={(event) => {
                                          event.stopPropagation();
                                          onSelectNode(leg.nodeId);
                                        }}
                                        title={`Inspect ${processLabel} leg node`}
                                      >
                                        {chipLabel}
                                      </button>
                                    ) : (
                                      <span>
                                        {chipLabel}
                                      </span>
                                    )}
                                  </div>
                                  <div className="resources-topbar-chip-meta">
                                    {`proc: ${leg.procKey || "—"}`}
                                    {` • status: ${leg.isMissing ? "missing-leg" : leg.snapshotStatus}`}
                                    {leg.pid == null ? " • pid: —" : ` • pid: ${leg.pid}`}
                                    {` • req: ${leg.pendingRequests} / resp: ${leg.pendingResponses}`}
                                  </div>
                                  {(leg.command || leg.cmdArgsPreview) && (
                                    <div className="resources-topbar-chip-meta resources-topbar-chip-cmd" title={`${leg.command ?? ""} ${leg.cmdArgsPreview ?? ""}`}>
                                      {`${leg.command ?? ""}${leg.command && leg.cmdArgsPreview ? " " : ""}${leg.cmdArgsPreview ?? ""}`.trim()}
                                    </div>
                                  )}
                                  {leg.pid != null && (leg.snapshotStatus !== "responded" || leg.isMissing) ? (
                                    <div className="resources-topbar-chip-actions">
                                      {sampleRunning ? (
                                        <button
                                          type="button"
                                          className="resources-debug-btn resources-debug-btn--loading"
                                          disabled
                                        >
                                          <CircleNotch size={10} weight="bold" className="resources-debug-btn__spinner" />
                                          sample running
                                        </button>
                                      ) : sampleResultUrl ? (
                                        <a
                                          href={sampleResultUrl}
                                          className="resources-debug-btn"
                                          target="_blank"
                                          rel="noreferrer"
                                        >
                                          Open sample txt
                                        </a>
                                      ) : (
                                        <button
                                          type="button"
                                          className="resources-debug-btn"
                                          onClick={(event) => {
                                            event.stopPropagation();
                                            void onProcessDebugClick("sample", leg).then(() => {
                                              clearDebugMessage();
                                            });
                                          }}
                                          title="Run sample command"
                                        >
                                          sample
                                        </button>
                                      )}
                                      {spindumpRunning ? (
                                        <button
                                          type="button"
                                          className="resources-debug-btn resources-debug-btn--loading"
                                          disabled
                                        >
                                          <CircleNotch size={10} weight="bold" className="resources-debug-btn__spinner" />
                                          spindump running
                                        </button>
                                      ) : spindumpResultUrl ? (
                                        <a
                                          href={spindumpResultUrl}
                                          className="resources-debug-btn"
                                          target="_blank"
                                          rel="noreferrer"
                                        >
                                          Open spindump txt
                                        </a>
                                      ) : (
                                        <button
                                          type="button"
                                          className="resources-debug-btn"
                                          onClick={(event) => {
                                            event.stopPropagation();
                                            void onProcessDebugClick("spindump", leg).then(() => {
                                              clearDebugMessage();
                                            });
                                          }}
                                          title="Run spindump command"
                                        >
                                          spindump
                                        </button>
                                      )}
                                    </div>
                                  ) : null}
                                </div>
                              );
                            })}
                        </div>
                      ) : (
                        <span className="resources-topbar-summary">
                          Both directions responded
                        </span>
                      )}
                      <div className="resources-topbar-summary-badges">
                        <button
                          type="button"
                          className={`resources-topbar-summary-btn ${summaryPendingRequestIds.length === 0 ? "resources-pending-btn--empty" : ""}`}
                          onClick={(event) => {
                            event.stopPropagation();
                            onPendingCellClick(
                              requestSummaryKey,
                              summaryPendingRequestIds,
                              summaryPendingResponseIds,
                              "request",
                            );
                          }}
                          title={
                            summaryPendingRequestIds.length === 0
                              ? "No pending request nodes for this link"
                              : "Click to highlight pending request nodes"
                          }
                          disabled={summaryPendingRequestIds.length === 0}
                        >
                          {`Req ${duplexRow.pendingRequests}`}
                        </button>
                        <button
                          type="button"
                          className={`resources-topbar-summary-btn ${summaryPendingResponseIds.length === 0 ? "resources-pending-btn--empty" : ""}`}
                          onClick={(event) => {
                            event.stopPropagation();
                            onPendingCellClick(
                              responseSummaryKey,
                              summaryPendingRequestIds,
                              summaryPendingResponseIds,
                              "response",
                            );
                          }}
                          title={
                            summaryPendingResponseIds.length === 0
                              ? "No pending response nodes for this link"
                              : "Click to highlight pending response nodes"
                          }
                          disabled={summaryPendingResponseIds.length === 0}
                        >
                          {`Resp ${duplexRow.pendingResponses}`}
                        </button>
                      </div>
                    </div>
                    <span className="resources-cell-mono resources-mini-value">{duplexRow.pendingRequests}</span>
                    <span className="resources-cell-mono resources-mini-value">{duplexRow.pendingResponses}</span>
                    <span className="resources-cell-mono resources-mini-value">{formatAge(duplexRow.lastRecvAgeNs)}</span>
                    <span className="resources-cell-mono resources-mini-value">{formatAge(duplexRow.lastSentAgeNs)}</span>
                  </div>
                  <div className="resources-duplex-legs">
                    {duplexRow.legs.map((leg) => {
                      const requestKey = `${duplexRow.key}:request:${leg.nodeId}`;
                      const responseKey = `${duplexRow.key}:response:${leg.nodeId}`;
                      const statusClass =
                        leg.snapshotStatus === PROCESS_STATUS_UNKNOWN
                          ? "unknown"
                          : leg.snapshotStatus === "responded"
                            ? "ok"
                            : "warn";
                      const sampleActionKey = `${leg.procKey}:sample`;
                      const spindumpActionKey = `${leg.procKey}:spindump`;
                      const sampleResultUrl = debugResultUrls[sampleActionKey];
                      const spindumpResultUrl = debugResultUrls[spindumpActionKey];
                      const sampleRunning = runningDebugActions.has(sampleActionKey);
                      const spindumpRunning = runningDebugActions.has(spindumpActionKey);
                      const processLabel = leg.process || "unknown";
                      return (
                        <div
                          key={leg.nodeId}
                          className={`resources-row resources-row-grid resources-row--leg ${leg.isMissing ? "resources-row--leg-missing" : ""}${selectedNodeId === leg.nodeId ? " resources-row--selected" : ""}`}
                          onClick={leg.isMissing ? undefined : () => onSelectNode(leg.nodeId)}
                          role={leg.isMissing ? undefined : "button"}
                          title={leg.nodeId}
                        >
                          <span>
                            <span
                              className={`resources-health-pill resources-health-pill--${
                                leg.health === "critical"
                                  ? "crit"
                                  : leg.health === "warning"
                                    ? "warn"
                                    : "ok"
                              }`}
                            >
                              {leg.health}
                            </span>
                          </span>
                          <span className="resources-cell-mono resources-leg-label">{leg.legLabel}</span>
                          <span className="resources-cell-mono">
                            <span
                              className={`resources-process-status resources-process-status--${statusClass}`}
                            >
                              {processLabel}
                            </span>
                            <span className="resources-process-metadata">
                              {`status: ${leg.snapshotStatus}`}
                              {` • proc: ${leg.procKey || "—"}`}
                              {leg.pid == null ? " • pid: —" : ` • pid: ${leg.pid}`}
                              {leg.errorText && ` • ${leg.errorText}`}
                            </span>
                            {leg.command && (
                              <span className="resources-process-cmd" title={leg.command}>
                                {leg.command}
                                {leg.cmdArgsPreview ? ` ${leg.cmdArgsPreview}` : ""}
                              </span>
                            )}
                          {leg.pid != null && (leg.snapshotStatus !== "responded" || leg.isMissing) ? (
                            <div className="resources-process-debug-actions">
                              {sampleRunning ? (
                                <button
                                  type="button"
                                  className="resources-debug-btn resources-debug-btn--loading"
                                  disabled
                                >
                                  <CircleNotch size={10} weight="bold" className="resources-debug-btn__spinner" />
                                  sample running
                                </button>
                              ) : sampleResultUrl ? (
                                <a
                                  href={sampleResultUrl}
                                  className="resources-debug-btn"
                                  target="_blank"
                                  rel="noreferrer"
                                >
                                  Open sample txt
                                </a>
                              ) : (
                                <button
                                  type="button"
                                  className="resources-debug-btn"
                                  onClick={(event) => {
                                    event.stopPropagation();
                                    void onProcessDebugClick("sample", leg).then(() => {
                                      clearDebugMessage();
                                    });
                                  }}
                                  title="Run sample command"
                                >
                                  sample
                                </button>
                              )}
                              {spindumpRunning ? (
                                <button
                                  type="button"
                                  className="resources-debug-btn resources-debug-btn--loading"
                                  disabled
                                >
                                  <CircleNotch size={10} weight="bold" className="resources-debug-btn__spinner" />
                                  spindump running
                                </button>
                              ) : spindumpResultUrl ? (
                                <a
                                  href={spindumpResultUrl}
                                  className="resources-debug-btn"
                                  target="_blank"
                                  rel="noreferrer"
                                >
                                  Open spindump txt
                                </a>
                              ) : (
                                <button
                                  type="button"
                                  className="resources-debug-btn"
                                  onClick={(event) => {
                                    event.stopPropagation();
                                    void onProcessDebugClick("spindump", leg).then(() => {
                                      clearDebugMessage();
                                    });
                                  }}
                                  title="Run spindump command"
                                >
                                  spindump
                                </button>
                              )}
                            </div>
                          ) : null}
                          </span>
                          <div className="resources-pending-cell">
                            <button
                              type="button"
                              className={`resources-pending-btn ${leg.pendingRequestIds.length === 0 ? "resources-pending-btn--empty" : ""}`}
                              onClick={(event) => {
                                event.stopPropagation();
                                onPendingCellClick(requestKey, leg.pendingRequestIds, leg.pendingResponseIds, "request");
                              }}
                              title={
                                leg.pendingRequestIds.length === 0
                                  ? "No pending request nodes for this side"
                                  : "Click to highlight pending request nodes"
                              }
                              aria-label={`Pending requests for ${leg.connectionToken}`}
                              disabled={leg.pendingRequestIds.length === 0}
                            >
                              Req {leg.pendingRequests}
                            </button>
                          </div>
                          <div className="resources-pending-cell">
                            <button
                              type="button"
                              className={`resources-pending-btn ${leg.pendingResponseIds.length === 0 ? "resources-pending-btn--empty" : ""}`}
                              onClick={(event) => {
                                event.stopPropagation();
                                onPendingCellClick(responseKey, leg.pendingRequestIds, leg.pendingResponseIds, "response");
                              }}
                              title={
                                leg.pendingResponseIds.length === 0
                                  ? "No pending response nodes for this side"
                                  : "Click to highlight pending response nodes"
                              }
                              aria-label={`Pending responses for ${leg.connectionToken}`}
                              disabled={leg.pendingResponseIds.length === 0}
                            >
                              Resp {leg.pendingResponses}
                            </button>
                          </div>
                          <span className="resources-cell-mono">{formatAge(leg.lastRecvAgeNs)}</span>
                          <span className="resources-cell-mono">{formatAge(leg.lastSentAgeNs)}</span>
                        </div>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
          <div className="resources-sort-hint">
            <CaretDown size={10} weight="bold" /> Click column headers to sort.
          </div>
        </div>
      )}
    </div>
  );
}
