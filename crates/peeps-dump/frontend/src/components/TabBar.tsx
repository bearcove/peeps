import type { Tab } from "../App";
import type { SummaryData } from "../api";
import type { ProcessDump, DeadlockCandidate } from "../types";
import { classNames } from "../util";
import { detectProblems, detectRelationshipIssues, hasDanger } from "../problems";

interface TabBarProps {
  tabs: readonly Tab[];
  active: Tab;
  onSelect: (t: Tab) => void;
  summary: SummaryData | null;
  deadlockCandidates: DeadlockCandidate[];
  problemsDumps: ProcessDump[];
}

function badgeCount(tab: Tab, summary: SummaryData | null): number | null {
  if (!summary) return null;
  switch (tab) {
    case "tasks":
      return summary.task_count;
    case "threads":
      return summary.thread_count;
    case "processes":
      return summary.process_count;
    default:
      return null;
  }
}

const TAB_LABELS: Record<Tab, string> = {
  problems: "Problems",
  deadlocks: "Deadlocks",
  tasks: "Tasks",
  threads: "Threads",
  sync: "Locks",
  locks: "Channels",
  requests: "Requests",
  connections: "Connections",
  processes: "Processes",
  shm: "SHM",
};

export function TabBar({ tabs, active, onSelect, summary, deadlockCandidates, problemsDumps }: TabBarProps) {
  const problems = detectProblems(problemsDumps);
  const relationIssues = detectRelationshipIssues(problemsDumps);
  const problemCount = problems.length + relationIssues.length;
  const danger = hasDanger(problems) || relationIssues.some((i) => i.severity === "danger");

  const hasDeadlockDanger = deadlockCandidates.some((c) => c.severity === "Danger");

  return (
    <div class="tab-bar">
      {tabs.map((t) => {
        if (t === "problems") {
          return (
            <div
              key={t}
              class={classNames("tab", t === active && "active")}
              onClick={() => onSelect(t)}
            >
              {TAB_LABELS[t]}
              {problemCount > 0 && (
                <span
                  class={classNames(
                    "tab-badge",
                    danger ? "tab-badge-danger" : "tab-badge-warn"
                  )}
                >
                  {problemCount}
                </span>
              )}
            </div>
          );
        }

        if (t === "deadlocks") {
          return (
            <div
              key={t}
              class={classNames("tab", t === active && "active")}
              onClick={() => onSelect(t)}
            >
              {TAB_LABELS[t]}
              {deadlockCandidates.length > 0 && (
                <span
                  class={classNames(
                    "tab-badge",
                    hasDeadlockDanger ? "tab-badge-danger" : "tab-badge-warn"
                  )}
                >
                  {deadlockCandidates.length}
                </span>
              )}
            </div>
          );
        }

        const count = badgeCount(t, summary);
        return (
          <div
            key={t}
            class={classNames("tab", t === active && "active")}
            onClick={() => onSelect(t)}
          >
            {TAB_LABELS[t]}
            {count != null && <span class="tab-badge">{count}</span>}
          </div>
        );
      })}
    </div>
  );
}
