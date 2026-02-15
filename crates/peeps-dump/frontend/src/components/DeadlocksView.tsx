import { useState } from "preact/hooks";
import type { DeadlockCandidate, CycleNode } from "../types";
import { classNames, fmtDuration } from "../util";
import { isActivePath, resourceHref } from "../routes";
import { ResourceLink } from "./ResourceLink";

interface Props {
  candidates: DeadlockCandidate[];
  filter: string;
  selectedPath: string;
}

function matchesCandidate(c: DeadlockCandidate, lq: string): boolean {
  return (
    c.title.toLowerCase().includes(lq) ||
    c.cycle_path.some((n) => n.label.toLowerCase().includes(lq) || n.process.toLowerCase().includes(lq)) ||
    c.cycle_edges.some((e) => e.explanation.toLowerCase().includes(lq)) ||
    c.rationale.some((r) => r.toLowerCase().includes(lq))
  );
}

function nodeHref(node: CycleNode): string {
  switch (node.kind) {
    case "task":
      return node.task_id != null
        ? resourceHref({ kind: "task", process: node.process, taskId: node.task_id })
        : resourceHref({ kind: "process", process: node.process });
    case "lock":
      return resourceHref({ kind: "lock", process: node.process, lock: node.label });
    case "thread":
      return resourceHref({ kind: "thread", process: node.process, thread: node.label });
    default:
      return resourceHref({ kind: "process", process: node.process });
  }
}

function nodeKind(node: CycleNode): "task" | "lock" | "thread" | "process" {
  switch (node.kind) {
    case "task": return "task";
    case "lock": return "lock";
    case "thread": return "thread";
    default: return "process";
  }
}

function triageText(c: DeadlockCandidate): string {
  const lines: string[] = [];
  lines.push(`[${c.severity}] ${c.title}`);
  lines.push(`Score: ${c.score.toFixed(1)} | Worst wait: ${fmtDuration(c.worst_wait_secs)} | Blocked tasks: ${c.blocked_task_count}`);
  if (c.cross_process) lines.push("Cross-process deadlock");
  lines.push("");
  lines.push("Cycle:");
  for (const edge of c.cycle_edges) {
    const from = c.cycle_path[edge.from_node];
    const to = c.cycle_path[edge.to_node];
    lines.push(`  ${from?.label ?? "?"} -> ${to?.label ?? "?"}: ${edge.explanation} (${fmtDuration(edge.wait_secs)})`);
  }
  if (c.rationale.length > 0) {
    lines.push("");
    lines.push("Rationale:");
    for (const r of c.rationale) {
      lines.push(`  - ${r}`);
    }
  }
  return lines.join("\n");
}

function CyclePathView({
  candidate,
  selectedPath,
}: {
  candidate: DeadlockCandidate;
  selectedPath: string;
}) {
  const { cycle_path, cycle_edges } = candidate;

  return (
    <div class="cycle-path">
      {cycle_edges.map((edge, idx) => {
        const fromNode = cycle_path[edge.from_node];
        const toNode = cycle_path[edge.to_node];
        if (!fromNode || !toNode) return null;

        const fromHref = nodeHref(fromNode);
        const toHref = nodeHref(toNode);
        const isLast = idx === cycle_edges.length - 1;

        return (
          <div key={idx} class="cycle-edge">
            <div class="cycle-edge-nodes">
              <ResourceLink
                href={fromHref}
                active={isActivePath(selectedPath, fromHref)}
                kind={nodeKind(fromNode)}
              >
                {fromNode.label}
              </ResourceLink>
              <span class="cycle-arrow">{"\u2192"}</span>
              <ResourceLink
                href={toHref}
                active={isActivePath(selectedPath, toHref)}
                kind={nodeKind(toNode)}
              >
                {toNode.label}
              </ResourceLink>
              <span class="cycle-edge-timing num">{fmtDuration(edge.wait_secs)}</span>
            </div>
            <div class="cycle-edge-explanation">{edge.explanation}</div>
            {!isLast && <div class="cycle-connector" />}
          </div>
        );
      })}
    </div>
  );
}

function CandidateCard({
  candidate,
  selectedPath,
  expanded,
  onToggle,
}: {
  candidate: DeadlockCandidate;
  selectedPath: string;
  expanded: boolean;
  onToggle: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const isDanger = candidate.severity === "Danger";

  const handleCopy = (e: Event) => {
    e.stopPropagation();
    navigator.clipboard.writeText(triageText(candidate)).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  return (
    <div class={classNames("card", "deadlock-card", isDanger && "deadlock-card-danger")}>
      <div class="deadlock-card-head" onClick={onToggle}>
        <span
          class={classNames(
            "state-badge",
            isDanger ? "state-dropped" : "state-pending",
          )}
        >
          {candidate.severity === "Danger" ? "danger" : "warn"}
        </span>
        <span class="deadlock-title">{candidate.title}</span>
        <span class="deadlock-meta">
          <span class="num">{fmtDuration(candidate.worst_wait_secs)}</span>
          {candidate.blocked_task_count > 0 && (
            <span class="muted">
              {" \u00b7 "}{candidate.blocked_task_count} blocked task{candidate.blocked_task_count !== 1 ? "s" : ""}
            </span>
          )}
          {candidate.cross_process && (
            <span class="deadlock-cross-process">cross-process</span>
          )}
        </span>
        <span class="deadlock-chevron">{expanded ? "\u25b4" : "\u25be"}</span>
      </div>

      {expanded && (
        <div class="deadlock-card-body">
          <div class="deadlock-section">
            <div class="deadlock-section-head">Cycle Path</div>
            <CyclePathView candidate={candidate} selectedPath={selectedPath} />
          </div>

          {candidate.rationale.length > 0 && (
            <div class="deadlock-section">
              <div class="deadlock-section-head">Rationale</div>
              <ul class="deadlock-rationale">
                {candidate.rationale.map((r, i) => (
                  <li key={i}>{r}</li>
                ))}
              </ul>
            </div>
          )}

          <div class="deadlock-actions">
            <button class="deadlock-copy-btn" onClick={handleCopy}>
              {copied ? "Copied" : "Copy triage text"}
            </button>
            <span class="muted">Score: {candidate.score.toFixed(1)}</span>
          </div>
        </div>
      )}
    </div>
  );
}

export function DeadlocksView({ candidates, filter, selectedPath }: Props) {
  const [expandedIds, setExpandedIds] = useState<Set<number>>(new Set());

  const lq = filter.toLowerCase();
  const filtered = lq ? candidates.filter((c) => matchesCandidate(c, lq)) : candidates;
  const sorted = [...filtered].sort((a, b) => {
    if (a.severity !== b.severity) return a.severity === "Danger" ? -1 : 1;
    return b.score - a.score;
  });

  const toggle = (id: number) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  if (sorted.length === 0) {
    return (
      <div class="fade-in">
        <div class="empty-state">
          <div class="icon" style="color: var(--green)">
            {"\u2714\ufe0e"}
          </div>
          <p>No deadlock candidates detected</p>
          <p class="sub">Wait graph analysis found no cycles</p>
        </div>
      </div>
    );
  }

  const dangerCount = sorted.filter((c) => c.severity === "Danger").length;
  const warnCount = sorted.filter((c) => c.severity === "Warn").length;

  return (
    <div class="fade-in">
      <div class="problems-summary">
        {dangerCount > 0 && (
          <span class="problems-count problems-count-danger">{dangerCount} danger</span>
        )}
        {warnCount > 0 && (
          <span class="problems-count problems-count-warn">
            {warnCount} warning{warnCount !== 1 ? "s" : ""}
          </span>
        )}
      </div>

      {sorted.map((c) => (
        <CandidateCard
          key={c.id}
          candidate={c}
          selectedPath={selectedPath}
          expanded={expandedIds.has(c.id)}
          onToggle={() => toggle(c.id)}
        />
      ))}
    </div>
  );
}
