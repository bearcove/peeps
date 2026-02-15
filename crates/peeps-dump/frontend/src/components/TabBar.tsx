import type { Tab } from "../App";
import type { ProcessDump, DeadlockCandidate } from "../types";
import { classNames } from "../util";
import { detectProblems, detectRelationshipIssues, hasDanger } from "../problems";

interface TabBarProps {
  tabs: readonly Tab[];
  active: Tab;
  onSelect: (t: Tab) => void;
  dumps: ProcessDump[];
  deadlockCandidates: DeadlockCandidate[];
}

function badgeCount(tab: Tab, dumps: ProcessDump[], deadlockCandidates: DeadlockCandidate[]): number | null {
  switch (tab) {
    case "problems":
      return null;
    case "deadlocks":
      return deadlockCandidates.length || null;
    case "tasks":
      return dumps.reduce((s, d) => s + d.tasks.length, 0);
    case "threads":
      return dumps.reduce((s, d) => s + d.threads.length, 0);
    case "locks":
      return dumps.reduce(
        (s, d) => s + (d.locks?.locks.length ?? 0),
        0
      );
    case "sync": {
      let n = 0;
      for (const d of dumps) {
        if (!d.sync) continue;
        n +=
          d.sync.mpsc_channels.length +
          d.sync.oneshot_channels.length +
          d.sync.watch_channels.length +
          d.sync.once_cells.length;
      }
      return n;
    }
    case "requests":
      return dumps.reduce(
        (s, d) =>
          s +
          (d.roam?.connections.reduce(
            (s2, c) => s2 + c.in_flight.length,
            0
          ) ?? 0),
        0
      );
    case "connections":
      return dumps.reduce(
        (s, d) => s + (d.roam?.connections.length ?? 0),
        0
      );
    case "processes":
      return dumps.length;
    case "shm":
      return null;
  }
}

const TAB_LABELS: Record<Tab, string> = {
  problems: "Problems",
  deadlocks: "Deadlocks",
  tasks: "Tasks",
  threads: "Threads",
  locks: "Locks",
  sync: "Sync",
  requests: "Requests",
  connections: "Connections",
  processes: "Processes",
  shm: "SHM",
};

export function TabBar({ tabs, active, onSelect, dumps, deadlockCandidates }: TabBarProps) {
  const problems = detectProblems(dumps);
  const relationIssues = detectRelationshipIssues(dumps);
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

        const count = badgeCount(t, dumps, deadlockCandidates);
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
