import type { ProcessDump } from "../types";
import {
  detectProblems,
  detectRelationshipIssues,
  summarizeRootCauses,
  type Problem,
  type ProblemCategory,
  type RelationshipIssue,
} from "../problems";
import { Expandable } from "./Expandable";
import { CausalGraph } from "./CausalGraph";
import { classNames } from "../util";
import { isActivePath, resourceHref, tabPath } from "../routes";
import { ResourceLink } from "./ResourceLink";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

const CATEGORY_ORDER: ProblemCategory[] = [
  "Tasks",
  "Threads",
  "Locks",
  "Channels",
  "RPC",
  "SHM",
];

function matchesProblem(p: Problem, lq: string): boolean {
  return (
    p.process.toLowerCase().includes(lq) ||
    p.resource.toLowerCase().includes(lq) ||
    p.description.toLowerCase().includes(lq) ||
    p.category.toLowerCase().includes(lq)
  );
}

function matchesIssue(i: RelationshipIssue, lq: string): boolean {
  return (
    i.process.toLowerCase().includes(lq) ||
    i.blocked.toLowerCase().includes(lq) ||
    i.waitsOn.toLowerCase().includes(lq) ||
    (i.owner?.toLowerCase().includes(lq) ?? false) ||
    i.description.toLowerCase().includes(lq) ||
    i.category.toLowerCase().includes(lq)
  );
}

function problemHref(p: Problem): string {
  switch (p.category) {
    case "Threads":
      return resourceHref({ kind: "thread", process: p.process, thread: p.resource });
    case "Locks":
      return resourceHref({ kind: "lock", process: p.process, lock: p.resource });
    case "Channels":
      return tabPath("sync");
    case "RPC":
      return tabPath("requests");
    case "SHM":
      return tabPath("shm");
    case "Tasks":
      return tabPath("tasks");
  }
}

function issueWaitsOnHref(i: RelationshipIssue): string {
  switch (i.category) {
    case "Locks":
      if (i.waitsOn.startsWith("lock:")) {
        return resourceHref({
          kind: "lock",
          process: i.process,
          lock: i.waitsOn.slice("lock:".length),
        });
      }
      return tabPath("locks");
    case "Channels":
      return tabPath("sync");
    case "RPC":
      return tabPath("requests");
  }
}

function problemKind(p: Problem): "task" | "thread" | "lock" | "mpsc" | "request" | "shm_segment" {
  switch (p.category) {
    case "Tasks":
      return "task";
    case "Threads":
      return "thread";
    case "Locks":
      return "lock";
    case "Channels":
      return "mpsc";
    case "RPC":
      return "request";
    case "SHM":
      return "shm_segment";
  }
}

function waitsOnKind(i: RelationshipIssue): "lock" | "mpsc" | "request" {
  switch (i.category) {
    case "Locks":
      return "lock";
    case "Channels":
      return "mpsc";
    case "RPC":
      return "request";
  }
}

export function ProblemsView({ dumps, filter, selectedPath }: Props) {
  const allProblems = detectProblems(dumps);
  const allIssues = detectRelationshipIssues(dumps);

  const lq = filter.toLowerCase();
  const problems = lq ? allProblems.filter((p) => matchesProblem(p, lq)) : allProblems;
  const issues = lq ? allIssues.filter((i) => matchesIssue(i, lq)) : allIssues;
  const rootCauses = summarizeRootCauses(issues);

  if (problems.length === 0 && issues.length === 0) {
    return (
      <div class="fade-in">
        <div class="empty-state">
          <div class="icon" style="color: var(--green)">
            ✔︎
          </div>
          <p>No problems detected</p>
          <p class="sub">All systems nominal</p>
        </div>
      </div>
    );
  }

  const dangerCount = problems.filter((p) => p.severity === "danger").length
    + issues.filter((i) => i.severity === "danger").length;
  const warnCount = problems.filter((p) => p.severity === "warn").length
    + issues.filter((i) => i.severity === "warn").length;

  const grouped = new Map<ProblemCategory, Problem[]>();
  for (const p of problems) {
    let list = grouped.get(p.category);
    if (!list) {
      list = [];
      grouped.set(p.category, list);
    }
    list.push(p);
  }

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

      {issues.length > 0 && <CausalGraph issues={issues} />}

      {rootCauses.length > 0 && (
        <div class="card">
          <div class="card-head">Likely Root Causes</div>
          <table>
            <thead>
              <tr>
                <th>Severity</th>
                <th>Owner</th>
                <th>Blocked Groups</th>
                <th>Total Edges</th>
                <th>Worst Wait</th>
              </tr>
            </thead>
            <tbody>
              {rootCauses.map((r, idx) => (
                <tr
                  key={`${r.owner}-${idx}`}
                  class={classNames(
                    r.severity === "danger" && "severity-danger",
                    r.severity === "warn" && "severity-warn",
                  )}
                >
                  <td>
                    <span
                      class={classNames(
                        "state-badge",
                        r.severity === "danger" ? "state-dropped" : "state-pending",
                      )}
                    >
                      {r.severity}
                    </span>
                  </td>
                  <td class="mono">{r.owner}</td>
                  <td class="num">{r.blockedCount}</td>
                  <td class="num">{r.edgeCount}</td>
                  <td class="num">{r.worstTimingLabel}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {issues.length > 0 && (
        <div class="card">
          <div class="card-head">Blocking Relationships</div>
          <table>
            <thead>
              <tr>
                <th>Severity</th>
                <th>Category</th>
                <th>Process</th>
                <th>Blocked</th>
                <th>Waits On</th>
                <th>Owner</th>
                <th>Impact</th>
                <th>Backtrace</th>
              </tr>
            </thead>
            <tbody>
              {issues.map((i, idx) => (
                <tr
                  key={`${i.process}-${i.blocked}-${idx}`}
                  class={classNames(
                    i.severity === "danger" && "severity-danger",
                    i.severity === "warn" && "severity-warn",
                  )}
                >
                  <td>
                    <span
                      class={classNames(
                        "state-badge",
                        i.severity === "danger" ? "state-dropped" : "state-pending",
                      )}
                    >
                      {i.severity}
                    </span>
                  </td>
                  <td class="mono">{i.category}</td>
                  <td class="mono">
                    <ResourceLink
                      href={resourceHref({ kind: "process", process: i.process })}
                      active={isActivePath(selectedPath, resourceHref({ kind: "process", process: i.process }))}
                      kind="process"
                    >
                      {i.process}
                    </ResourceLink>
                  </td>
                  <td class="mono">{i.blocked}</td>
                  <td class="mono">
                    <ResourceLink
                      href={issueWaitsOnHref(i)}
                      active={isActivePath(selectedPath, issueWaitsOnHref(i))}
                      kind={waitsOnKind(i)}
                    >
                      {i.waitsOn}
                    </ResourceLink>
                  </td>
                  <td class="mono">{i.owner ?? "—"}</td>
                  <td>
                    {i.description}
                    {i.count > 1 ? <span class="muted"> ({i.count}x)</span> : null}
                  </td>
                  <td>
                    <Expandable content={i.backtrace} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {CATEGORY_ORDER.filter((cat) => grouped.has(cat)).map((cat) => {
        const categoryProblems = grouped.get(cat)!;
        return (
          <div class="card" key={cat}>
            <div class="card-head">Raw Signals: {cat}</div>
            <table>
              <thead>
                <tr>
                  <th>Severity</th>
                  <th>Process</th>
                  <th>Resource</th>
                  <th>Description</th>
                  <th>Timing</th>
                  <th>Backtrace</th>
                </tr>
              </thead>
              <tbody>
                {categoryProblems.map((p, i) => (
                  <tr
                    key={i}
                    class={classNames(
                      p.severity === "danger" && "severity-danger",
                      p.severity === "warn" && "severity-warn",
                    )}
                  >
                    <td>
                      <span
                        class={classNames(
                          "state-badge",
                          p.severity === "danger" ? "state-dropped" : "state-pending",
                        )}
                      >
                        {p.severity}
                      </span>
                    </td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "process", process: p.process })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "process", process: p.process }))}
                        kind="process"
                      >
                        {p.process}
                      </ResourceLink>
                    </td>
                    <td class="mono">
                      <ResourceLink
                        href={problemHref(p)}
                        active={isActivePath(selectedPath, problemHref(p))}
                        kind={problemKind(p)}
                      >
                        {p.resource}
                      </ResourceLink>
                    </td>
                    <td>{p.description}</td>
                    <td class="num">{p.timingLabel}</td>
                    <td>
                      <Expandable content={p.backtrace} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      })}
    </div>
  );
}
