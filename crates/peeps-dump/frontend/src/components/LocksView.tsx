import type { ProcessDump, LockInfoSnapshot } from "../types";
import { fmtDuration } from "../util";
import { Expandable } from "./Expandable";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

interface FlatLock extends LockInfoSnapshot {
  process: string;
}

export function LocksView({ dumps, filter, selectedPath }: Props) {
  const locks: FlatLock[] = [];
  for (const d of dumps) {
    if (!d.locks) continue;
    for (const l of d.locks.locks) {
      locks.push({ ...l, process: d.process_name });
    }
  }

  const q = filter.toLowerCase();
  const filtered = locks.filter(
    (l) =>
      !q ||
      l.name.toLowerCase().includes(q) ||
      l.process.toLowerCase().includes(q)
  );

  if (locks.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">L</div>
        <p>No contended locks</p>
      </div>
    );
  }

  return (
    <div class="fade-in">
      {filtered.map((l, i) => (
        <div key={i} class="card">
          <div class="card-head">
            <span class="mono">
              <ResourceLink
                href={resourceHref({ kind: "process", process: l.process })}
                active={isActivePath(selectedPath, resourceHref({ kind: "process", process: l.process }))}
                kind="process"
              >
                {l.process}
              </ResourceLink>
            </span>
            <span class="muted">/</span>
            <span class="mono">
              <ResourceLink
                href={resourceHref({ kind: "lock", process: l.process, lock: l.name })}
                active={isActivePath(selectedPath, resourceHref({ kind: "lock", process: l.process, lock: l.name }))}
                kind="lock"
              >
                {l.name}
              </ResourceLink>
            </span>
            <span class="muted" style="margin-left: auto">
              {l.acquires} acquires, {l.releases} releases
            </span>
          </div>
          {l.holders.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th>Holder</th>
                  <th>Kind</th>
                  <th>Held</th>
                  <th>Task</th>
                  <th>Backtrace</th>
                </tr>
              </thead>
              <tbody>
                {l.holders.map((h, hi) => (
                  <tr key={hi}>
                    <td class="mono">holder #{hi}</td>
                    <td class="mono">{h.kind}</td>
                    <td class="num">{fmtDuration(h.held_secs)}</td>
                    <td class="mono">
                      {h.task_id != null ? (
                        <ResourceLink
                          href={resourceHref({ kind: "task", process: l.process, taskId: h.task_id })}
                          active={isActivePath(selectedPath, resourceHref({ kind: "task", process: l.process, taskId: h.task_id }))}
                          kind="task"
                        >
                          {h.task_name ?? ""} (#{h.task_id})
                        </ResourceLink>
                      ) : (
                        <span class="muted">{"\u2014"}</span>
                      )}
                    </td>
                    <td>
                      <Expandable content={h.backtrace} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {l.waiters.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th>Waiter</th>
                  <th>Kind</th>
                  <th>Waiting</th>
                  <th>Task</th>
                  <th>Backtrace</th>
                </tr>
              </thead>
              <tbody>
                {l.waiters.map((w, wi) => (
                  <tr key={wi} class={w.waiting_secs > 1 ? "severity-warn" : ""}>
                    <td class="mono">waiter #{wi}</td>
                    <td class="mono">{w.kind}</td>
                    <td class="num">{fmtDuration(w.waiting_secs)}</td>
                    <td class="mono">
                      {w.task_id != null ? (
                        <ResourceLink
                          href={resourceHref({ kind: "task", process: l.process, taskId: w.task_id })}
                          active={isActivePath(selectedPath, resourceHref({ kind: "task", process: l.process, taskId: w.task_id }))}
                          kind="task"
                        >
                          {w.task_name ?? ""} (#{w.task_id})
                        </ResourceLink>
                      ) : (
                        <span class="muted">{"\u2014"}</span>
                      )}
                    </td>
                    <td>
                      <Expandable content={w.backtrace} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      ))}
    </div>
  );
}
