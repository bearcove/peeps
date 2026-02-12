import type { ProcessDump } from "../types";
import {
  detectProblems,
  type Problem,
  type ProblemCategory,
} from "../problems";
import { Expandable } from "./Expandable";
import { classNames } from "../util";

interface Props {
  dumps: ProcessDump[];
  filter: string;
}

const CATEGORY_ORDER: ProblemCategory[] = [
  "Tasks",
  "Threads",
  "Locks",
  "Channels",
  "RPC",
  "SHM",
];

export function ProblemsView({ dumps, filter }: Props) {
  const all = detectProblems(dumps);

  const lq = filter.toLowerCase();
  const filtered = lq
    ? all.filter(
        (p) =>
          p.process.toLowerCase().includes(lq) ||
          p.resource.toLowerCase().includes(lq) ||
          p.description.toLowerCase().includes(lq) ||
          p.category.toLowerCase().includes(lq)
      )
    : all;

  if (filtered.length === 0) {
    return (
      <div class="fade-in">
        <div class="empty-state">
          <div class="icon" style="color: var(--green)">
            &check;
          </div>
          <p>No problems detected</p>
          <p class="sub">All systems nominal</p>
        </div>
      </div>
    );
  }

  const dangerCount = filtered.filter((p) => p.severity === "danger").length;
  const warnCount = filtered.filter((p) => p.severity === "warn").length;

  const grouped = new Map<ProblemCategory, Problem[]>();
  for (const p of filtered) {
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
          <span class="problems-count problems-count-danger">
            {dangerCount} danger
          </span>
        )}
        {warnCount > 0 && (
          <span class="problems-count problems-count-warn">
            {warnCount} warning{warnCount !== 1 ? "s" : ""}
          </span>
        )}
      </div>

      {CATEGORY_ORDER.filter((cat) => grouped.has(cat)).map((cat) => {
        const problems = grouped.get(cat)!;
        return (
          <div class="card" key={cat}>
            <div class="card-head">{cat}</div>
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
                {problems.map((p, i) => (
                  <tr
                    key={i}
                    class={classNames(
                      p.severity === "danger" && "severity-danger",
                      p.severity === "warn" && "severity-warn"
                    )}
                  >
                    <td>
                      <span
                        class={classNames(
                          "state-badge",
                          p.severity === "danger"
                            ? "state-dropped"
                            : "state-pending"
                        )}
                      >
                        {p.severity}
                      </span>
                    </td>
                    <td class="mono">{p.process}</td>
                    <td class="mono">{p.resource}</td>
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
