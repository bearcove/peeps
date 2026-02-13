import { useState } from "preact/hooks";
import type { ProcessDump, TaskSnapshot } from "../types";
import { fmtAge, fmtDuration, classNames } from "../util";
import { Expandable } from "./Expandable";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

interface FlatTask extends TaskSnapshot {
  process: string;
  pid: number;
  interactions: TaskInteraction[];
}

interface TaskInteraction {
  key: string;
  href: string;
  label: string;
  kind: "lock" | "mpsc" | "oneshot" | "watch";
}

function stateClass(state: string): string {
  switch (state) {
    case "Polling":
      return "state-polling";
    case "Completed":
      return "state-completed";
    default:
      return "state-pending";
  }
}

function rowSeverity(t: FlatTask): string {
  if (t.state === "Polling") {
    const lastPoll = t.poll_events[t.poll_events.length - 1];
    if (lastPoll?.duration_secs != null && lastPoll.duration_secs > 1)
      return "severity-danger";
    if (lastPoll?.duration_secs != null && lastPoll.duration_secs > 0.1)
      return "severity-warn";
  }
  return "";
}

function matchesFilter(t: FlatTask, q: string): boolean {
  if (!q) return true;
  const lq = q.toLowerCase();
  return (
    t.name.toLowerCase().includes(lq) ||
    t.process.toLowerCase().includes(lq) ||
    t.state.toLowerCase().includes(lq) ||
    (t.parent_task_name?.toLowerCase().includes(lq) ?? false) ||
    String(t.id).includes(lq)
  );
}

export function TasksView({ dumps, filter, selectedPath }: Props) {
  const [view, setView] = useState<"table" | "tree">("table");
  const interactionsByTask = new Map<string, TaskInteraction[]>();

  const addInteraction = (process: string, taskId: number | null, interaction: TaskInteraction) => {
    if (taskId == null) return;
    const key = `${process}#${taskId}`;
    const list = interactionsByTask.get(key) ?? [];
    if (!list.some((i) => i.key === interaction.key)) {
      list.push(interaction);
      interactionsByTask.set(key, list);
    }
  };

  for (const d of dumps) {
    if (d.locks) {
      for (const l of d.locks.locks) {
        const lockHref = resourceHref({ kind: "lock", process: d.process_name, lock: l.name });
        for (const h of l.holders) {
          addInteraction(d.process_name, h.task_id, {
            key: `lock:${l.name}:holder`,
            href: lockHref,
            label: `lock ${l.name} (holder)`,
            kind: "lock",
          });
        }
        for (const w of l.waiters) {
          addInteraction(d.process_name, w.task_id, {
            key: `lock:${l.name}:waiter`,
            href: lockHref,
            label: `lock ${l.name} (waiter)`,
            kind: "lock",
          });
        }
      }
    }

    if (d.sync) {
      for (const ch of d.sync.mpsc_channels) {
        addInteraction(d.process_name, ch.creator_task_id, {
          key: `mpsc:${ch.name}`,
          href: resourceHref({ kind: "mpsc", process: d.process_name, name: ch.name }),
          label: `mpsc ${ch.name}`,
          kind: "mpsc",
        });
      }
      for (const ch of d.sync.oneshot_channels) {
        addInteraction(d.process_name, ch.creator_task_id, {
          key: `oneshot:${ch.name}`,
          href: resourceHref({ kind: "oneshot", process: d.process_name, name: ch.name }),
          label: `oneshot ${ch.name}`,
          kind: "oneshot",
        });
      }
      for (const ch of d.sync.watch_channels) {
        addInteraction(d.process_name, ch.creator_task_id, {
          key: `watch:${ch.name}`,
          href: resourceHref({ kind: "watch", process: d.process_name, name: ch.name }),
          label: `watch ${ch.name}`,
          kind: "watch",
        });
      }
    }
  }

  const tasks: FlatTask[] = [];
  for (const d of dumps) {
    for (const t of d.tasks) {
      const key = `${d.process_name}#${t.id}`;
      tasks.push({
        ...t,
        process: d.process_name,
        pid: d.pid,
        interactions: interactionsByTask.get(key) ?? [],
      });
    }
  }

  const filtered = tasks.filter((t) => matchesFilter(t, filter));

  if (tasks.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">T</div>
        <p>No tasks tracked</p>
        <p class="sub">Tasks appear when using peeps::tasks::spawn_tracked()</p>
      </div>
    );
  }

  return (
    <div class="fade-in">
      <div style="margin-bottom: 12px; display: flex; gap: 8px">
        <button
          class={classNames("expand-trigger", view === "table" && "active")}
          style="padding: 4px 10px"
          onClick={() => setView("table")}
        >
          Table
        </button>
        <button
          class={classNames("expand-trigger", view === "tree" && "active")}
          style="padding: 4px 10px"
          onClick={() => setView("tree")}
        >
          Tree
        </button>
      </div>
      {view === "table" ? (
        <TaskTable tasks={filtered} selectedPath={selectedPath} />
      ) : (
        <TaskTree tasks={filtered} selectedPath={selectedPath} />
      )}
    </div>
  );
}

function TaskTable({ tasks, selectedPath }: { tasks: FlatTask[]; selectedPath: string }) {
  return (
    <div class="card">
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>Process</th>
            <th>Name</th>
            <th>State</th>
            <th>Age</th>
            <th>Parent</th>
            <th>Interactions</th>
            <th>Polls</th>
            <th>Last Poll</th>
            <th>Backtrace</th>
          </tr>
        </thead>
        <tbody>
          {tasks.map((t) => (
            <tr key={`${t.pid}-${t.id}`} class={rowSeverity(t)}>
              <td class="mono">#{t.id}</td>
              <td class="mono">
                <ResourceLink
                  href={resourceHref({ kind: "process", process: t.process, pid: t.pid })}
                  active={isActivePath(selectedPath, resourceHref({ kind: "process", process: t.process, pid: t.pid }))}
                  kind="process"
                >
                  {t.process}
                </ResourceLink>
              </td>
              <td class="mono">
                <ResourceLink
                  href={resourceHref({ kind: "task", process: t.process, taskId: t.id })}
                  active={isActivePath(selectedPath, resourceHref({ kind: "task", process: t.process, taskId: t.id }))}
                  kind="task"
                >
                  {t.name}
                </ResourceLink>
              </td>
              <td>
                <span class={classNames("state-badge", stateClass(t.state))}>
                  {t.state}
                </span>
              </td>
              <td class="num">{fmtAge(t.age_secs)}</td>
              <td class="mono">
                {t.parent_task_id != null ? (
                  <ResourceLink
                    href={resourceHref({ kind: "task", process: t.process, taskId: t.parent_task_id })}
                    active={isActivePath(selectedPath, resourceHref({ kind: "task", process: t.process, taskId: t.parent_task_id }))}
                    kind="task"
                  >
                    {t.parent_task_name ?? ""} (#{t.parent_task_id})
                  </ResourceLink>
                ) : (
                  <span class="muted">{"\u2014"}</span>
                )}
              </td>
              <td>
                {t.interactions.length === 0 ? (
                  <span class="muted">{"\u2014"}</span>
                ) : (
                  <div class="resource-link-list">
                    {t.interactions.map((i) => (
                      <ResourceLink key={i.key} href={i.href} active={isActivePath(selectedPath, i.href)} kind={i.kind}>
                        {i.label}
                      </ResourceLink>
                    ))}
                  </div>
                )}
              </td>
              <td class="num">{t.poll_events.length}</td>
              <td class="num">
                {t.poll_events.length > 0
                  ? fmtDuration(
                      t.poll_events[t.poll_events.length - 1].duration_secs ?? 0
                    )
                  : "\u2014"}
              </td>
              <td>
                <Expandable content={t.spawn_backtrace || null} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function TaskTree({ tasks, selectedPath }: { tasks: FlatTask[]; selectedPath: string }) {
  const byId = new Map<number, FlatTask>();
  for (const t of tasks) byId.set(t.id, t);

  const roots = tasks.filter((t) => t.parent_task_id == null || !byId.has(t.parent_task_id));
  const children = new Map<number, FlatTask[]>();
  for (const t of tasks) {
    if (t.parent_task_id != null && byId.has(t.parent_task_id)) {
      const list = children.get(t.parent_task_id) ?? [];
      list.push(t);
      children.set(t.parent_task_id, list);
    }
  }

  return (
    <div>
      {roots.map((t) => (
        <TreeNode key={t.id} task={t} children={children} depth={0} selectedPath={selectedPath} />
      ))}
    </div>
  );
}

function TreeNode({
  task: t,
  children,
  depth,
  selectedPath,
}: {
  task: FlatTask;
  children: Map<number, FlatTask[]>;
  depth: number;
  selectedPath: string;
}) {
  const kids = children.get(t.id) ?? [];
  return (
    <div class={depth === 0 ? "tree-node-root" : "tree-node"}>
      <div class="tree-item">
        <span class={classNames("state-badge", stateClass(t.state))} style="margin-right: 6px">
          {t.state}
        </span>
        <span class="mono">
          <ResourceLink
            href={resourceHref({ kind: "task", process: t.process, taskId: t.id })}
            active={isActivePath(selectedPath, resourceHref({ kind: "task", process: t.process, taskId: t.id }))}
            kind="task"
          >
            #{t.id} {t.name}
          </ResourceLink>
        </span>
        <span class="muted" style="margin-left: 8px">
          {t.process} &middot; {fmtAge(t.age_secs)}
        </span>
      </div>
      {kids.map((k) => (
        <TreeNode
          key={k.id}
          task={k}
          children={children}
          depth={depth + 1}
          selectedPath={selectedPath}
        />
      ))}
    </div>
  );
}
