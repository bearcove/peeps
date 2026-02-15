import { useState } from "preact/hooks";
import type { ProcessDump, RequestSnapshot } from "../types";
import { fmtDuration, classNames } from "../util";
import { Expandable } from "./Expandable";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

const CHAIN_ID_KEY = "peeps.chain_id";
const SPAN_ID_KEY = "peeps.span_id";
const PARENT_SPAN_ID_KEY = "peeps.parent_span_id";

interface FlatRequest extends RequestSnapshot {
  process: string;
  connection: string;
  peer: string;
  chain_id: string | null;
  span_id: string | null;
  parent_span_id: string | null;
}

interface RequestTaskInteraction {
  key: string;
  href: string;
  label: string;
  kind: "lock" | "mpsc" | "oneshot" | "watch" | "once_cell" | "semaphore" | "roam_channel" | "future_wait";
  ageSecs?: number;
  note?: string;
}

function taskKey(process: string, taskId: number): string {
  return `${process}#${taskId}`;
}

function rowSeverity(r: FlatRequest): string {
  if (r.elapsed_secs > 10) return "severity-danger";
  if (r.elapsed_secs > 2) return "severity-warn";
  return "";
}

function meta(r: RequestSnapshot, key: string): string | null {
  return r.metadata?.[key] ?? null;
}

function requestNodeKey(r: FlatRequest): string {
  return `${r.process}::${r.connection}::${r.request_id}`;
}

function RequestLink({
  r,
  selectedPath,
}: {
  r: FlatRequest;
  selectedPath: string;
}) {
  const href = resourceHref({
    kind: "request",
    process: r.process,
    connection: r.connection,
    requestId: r.request_id,
  });
  return (
    <ResourceLink href={href} active={isActivePath(selectedPath, href)} kind="request">
      {r.method_name ?? `method_${r.method_id}`}
    </ResourceLink>
  );
}

function RequestContextTree({
  r,
  interactionsByTask,
  selectedPath,
}: {
  r: FlatRequest;
  interactionsByTask: Map<string, RequestTaskInteraction[]>;
  selectedPath: string;
}) {
  if (r.task_id == null) return <span class="muted">—</span>;
  const key = taskKey(r.process, r.task_id);
  const interactions = [...(interactionsByTask.get(key) ?? [])].sort(
    (a, b) => (b.ageSecs ?? 0) - (a.ageSecs ?? 0),
  );
  const taskHref = resourceHref({ kind: "task", process: r.process, taskId: r.task_id });

  return (
    <details>
      <summary class="mono" style="cursor: pointer">
        {interactions.length} resource interaction(s)
      </summary>
      <div style="padding-top: 6px">
        <div style="margin-bottom: 6px">
          <ResourceLink href={taskHref} active={isActivePath(selectedPath, taskHref)} kind="task">
            {r.task_name ?? "task"} (#{r.task_id})
          </ResourceLink>
        </div>
        {interactions.length > 0 ? (
          <div class="resource-link-list">
            {interactions.map((i) => (
              <ResourceLink key={i.key} href={i.href} active={isActivePath(selectedPath, i.href)} kind={i.kind}>
                {i.label}
              </ResourceLink>
            ))}
          </div>
        ) : (
          <span class="muted">no tracked locks/channels/futures for this task yet</span>
        )}
      </div>
    </details>
  );
}

function RequestContextMini({
  node,
  interactionsByTask,
  selectedPath,
}: {
  node: FlatRequest;
  interactionsByTask: Map<string, RequestTaskInteraction[]>;
  selectedPath: string;
}) {
  if (node.task_id == null) return null;
  const interactions = interactionsByTask.get(taskKey(node.process, node.task_id)) ?? [];
  if (interactions.length === 0) return null;
  return (
    <div style="margin: 4px 0 0 20px">
      <RequestContextTree r={node} interactionsByTask={interactionsByTask} selectedPath={selectedPath} />
    </div>
  );
}

function RequestTreeNode({
  node,
  childrenByParent,
  interactionsByTask,
  selectedPath,
  depth,
  seen,
}: {
  node: FlatRequest;
  childrenByParent: Map<string, FlatRequest[]>;
  interactionsByTask: Map<string, RequestTaskInteraction[]>;
  selectedPath: string;
  depth: number;
  seen: Set<string>;
}) {
  const key = requestNodeKey(node);
  if (seen.has(key)) return null;
  const nextSeen = new Set(seen);
  nextSeen.add(key);
  const kids = [...(childrenByParent.get(key) ?? [])].sort(
    (a, b) => b.elapsed_secs - a.elapsed_secs,
  );

  return (
    <div style={`margin-left: ${depth * 16}px; margin-top: 6px`}>
      <div class={classNames("tree-item", rowSeverity(node))} style="padding: 6px 8px; border-radius: 6px">
        <span
          class={classNames("dir", node.direction === "Outgoing" ? "dir-out" : "dir-in")}
          style="margin-right: 6px"
        >
          {node.direction === "Outgoing" ? "\u2192" : "\u2190"}
        </span>
        <span class="mono">
          <RequestLink r={node} selectedPath={selectedPath} />
        </span>
        <span class="muted" style="margin-left: 8px">
          {node.process} / {node.connection} / {fmtDuration(node.elapsed_secs)}
        </span>
        {node.task_id != null && (
          <span class="mono" style="margin-left: 8px">
            <ResourceLink
              href={resourceHref({ kind: "task", process: node.process, taskId: node.task_id })}
              active={isActivePath(
                selectedPath,
                resourceHref({ kind: "task", process: node.process, taskId: node.task_id }),
              )}
              kind="task"
            >
              #{node.task_id}
            </ResourceLink>
          </span>
        )}
      </div>
      <RequestContextMini node={node} interactionsByTask={interactionsByTask} selectedPath={selectedPath} />
      {kids.map((child) => (
        <RequestTreeNode
          key={requestNodeKey(child)}
          node={child}
          childrenByParent={childrenByParent}
          interactionsByTask={interactionsByTask}
          selectedPath={selectedPath}
          depth={depth + 1}
          seen={nextSeen}
        />
      ))}
    </div>
  );
}

export function RequestsView({ dumps, filter, selectedPath }: Props) {
  const [view, setView] = useState<"table" | "tree">("table");
  const interactionsByTask = new Map<string, RequestTaskInteraction[]>();
  const addInteraction = (process: string, taskId: number | null, interaction: RequestTaskInteraction) => {
    if (taskId == null) return;
    const key = taskKey(process, taskId);
    const list = interactionsByTask.get(key) ?? [];
    if (!list.some((i) => i.key === interaction.key)) {
      list.push(interaction);
      interactionsByTask.set(key, list);
    }
  };

  for (const d of dumps) {
    for (const w of d.future_waits) {
      addInteraction(d.process_name, w.task_id, {
        key: `future:${w.future_id}:${w.task_id}`,
        href: resourceHref({
          kind: "future_wait",
          process: d.process_name,
          taskId: w.task_id,
          resource: w.resource,
        }),
        label: `future ${w.resource}`,
        kind: "future_wait",
        ageSecs: w.total_pending_secs,
        note: `pending ${fmtDuration(w.total_pending_secs)}`,
      });
    }

    if (d.locks) {
      for (const l of d.locks.locks) {
        const lockHref = resourceHref({ kind: "lock", process: d.process_name, lock: l.name });
        for (const h of l.holders) {
          addInteraction(d.process_name, h.task_id, {
            key: `lock:${l.name}:holder:${h.task_id ?? "?"}`,
            href: lockHref,
            label: `lock ${l.name} (holder)`,
            kind: "lock",
            ageSecs: h.held_secs,
          });
        }
        for (const w of l.waiters) {
          addInteraction(d.process_name, w.task_id, {
            key: `lock:${l.name}:waiter:${w.task_id ?? "?"}`,
            href: lockHref,
            label: `lock ${l.name} (waiter)`,
            kind: "lock",
            ageSecs: w.waiting_secs,
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
          ageSecs: ch.age_secs,
        });
      }
      for (const ch of d.sync.oneshot_channels) {
        addInteraction(d.process_name, ch.creator_task_id, {
          key: `oneshot:${ch.name}`,
          href: resourceHref({ kind: "oneshot", process: d.process_name, name: ch.name }),
          label: `oneshot ${ch.name}`,
          kind: "oneshot",
          ageSecs: ch.age_secs,
        });
      }
      for (const ch of d.sync.watch_channels) {
        addInteraction(d.process_name, ch.creator_task_id, {
          key: `watch:${ch.name}`,
          href: resourceHref({ kind: "watch", process: d.process_name, name: ch.name }),
          label: `watch ${ch.name}`,
          kind: "watch",
          ageSecs: ch.age_secs,
        });
      }
      for (const sem of d.sync.semaphores) {
        addInteraction(d.process_name, sem.creator_task_id, {
          key: `sem:${sem.name}`,
          href: resourceHref({ kind: "semaphore", process: d.process_name, name: sem.name }),
          label: `semaphore ${sem.name}`,
          kind: "semaphore",
          ageSecs: sem.oldest_wait_secs,
        });
      }
    }

    if (d.roam) {
      for (const ch of d.roam.channel_details ?? []) {
        addInteraction(d.process_name, ch.task_id, {
          key: `roam:${ch.channel_id}`,
          href: resourceHref({ kind: "roam_channel", process: d.process_name, channelId: ch.channel_id }),
          label: `roam ${ch.name}`,
          kind: "roam_channel",
          ageSecs: ch.age_secs,
        });
      }
    }
  }

  const requests: FlatRequest[] = [];
  for (const d of dumps) {
    if (!d.roam) continue;
    for (const c of d.roam.connections) {
      for (const r of c.in_flight) {
        requests.push({
          ...r,
          process: d.process_name,
          connection: c.name,
          peer: c.peer_name ?? "?",
          chain_id: meta(r, CHAIN_ID_KEY),
          span_id: meta(r, SPAN_ID_KEY),
          parent_span_id: meta(r, PARENT_SPAN_ID_KEY),
        });
      }
    }
  }
  requests.sort((a, b) => b.elapsed_secs - a.elapsed_secs);

  const q = filter.toLowerCase();
  const filtered = requests.filter(
    (r) =>
      !q ||
      r.process.toLowerCase().includes(q) ||
      (r.method_name?.toLowerCase().includes(q) ?? false) ||
      r.connection.toLowerCase().includes(q) ||
      r.peer.toLowerCase().includes(q) ||
      (r.chain_id?.toLowerCase().includes(q) ?? false) ||
      (r.span_id?.toLowerCase().includes(q) ?? false),
  );

  if (requests.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">R</div>
        <p>No in-flight requests</p>
      </div>
    );
  }

  const byChain = new Map<string, FlatRequest[]>();
  for (const r of filtered) {
    const key = r.chain_id ?? `unscoped:${r.process}`;
    const list = byChain.get(key) ?? [];
    list.push(r);
    byChain.set(key, list);
  }
  const chains = [...byChain.entries()].sort((a, b) => {
    const aWorst = Math.max(...a[1].map((r) => r.elapsed_secs), 0);
    const bWorst = Math.max(...b[1].map((r) => r.elapsed_secs), 0);
    return bWorst - aWorst;
  });

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
        <div style="display: flex; flex-direction: column; gap: 10px">
          {filtered.map((r) => (
            <div
              key={requestNodeKey(r)}
              class="sync-instance-card"
              style={
                rowSeverity(r) === "severity-danger"
                  ? "border-color: var(--red)"
                  : rowSeverity(r) === "severity-warn"
                    ? "border-color: var(--amber)"
                    : undefined
              }
            >
              <div class="sync-instance-head">
                <div class="sync-instance-title mono">
                  <RequestLink r={r} selectedPath={selectedPath} />
                  <span class="muted">{"\u2022"}</span>
                  <ResourceLink
                    href={resourceHref({ kind: "process", process: r.process })}
                    active={isActivePath(selectedPath, resourceHref({ kind: "process", process: r.process }))}
                    kind="process"
                  >
                    {r.process}
                  </ResourceLink>
                  <span class="muted">{"\u2192"}</span>
                  <span>{r.peer}</span>
                </div>
                <div class="mono">{fmtDuration(r.elapsed_secs)}</div>
              </div>

              <div class="sync-instance-body" style="margin-top: 10px">
                <div class="sync-kv">
                  <span class="k">Direction</span>
                  <span class="v">{r.direction}</span>
                </div>
                <div class="sync-kv">
                  <span class="k">Connection</span>
                  <span class="v">
                    <ResourceLink
                      href={resourceHref({ kind: "connection", process: r.process, connection: r.connection })}
                      active={isActivePath(selectedPath, resourceHref({ kind: "connection", process: r.process, connection: r.connection }))}
                      kind="connection"
                    >
                      {r.connection}
                    </ResourceLink>
                  </span>
                </div>
              </div>

              <div class="sync-instance-foot" style="display: block">
                <div class="sync-kv" style="margin-bottom: 8px">
                  <span class="k">Task</span>
                  <span class="v">
                    {r.task_id != null ? (
                      <ResourceLink
                        href={resourceHref({ kind: "task", process: r.process, taskId: r.task_id })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "task", process: r.process, taskId: r.task_id }))}
                        kind="task"
                      >
                        {r.task_name ?? "task"} (#{r.task_id})
                      </ResourceLink>
                    ) : (
                      <span class="muted">{"\u2014"}</span>
                    )}
                  </span>
                </div>
                <div style="margin-bottom: 8px">
                  <div class="k muted" style="font-size: 11px; margin-bottom: 4px">Context</div>
                  <RequestContextTree
                    r={r}
                    interactionsByTask={interactionsByTask}
                    selectedPath={selectedPath}
                  />
                </div>
                <div>
                  <div class="k muted" style="font-size: 11px; margin-bottom: 4px">Backtrace</div>
                  <Expandable content={r.backtrace} />
                </div>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div>
          {chains.map(([chainId, chainRequests]) => {
            const spanOwner = new Map<string, FlatRequest>();
            for (const req of chainRequests) {
              if (req.span_id && !spanOwner.has(req.span_id)) {
                spanOwner.set(req.span_id, req);
              }
            }

            const childrenByParent = new Map<string, FlatRequest[]>();
            const roots: FlatRequest[] = [];
            for (const req of chainRequests) {
              const parent =
                req.parent_span_id != null
                  ? spanOwner.get(req.parent_span_id)
                  : undefined;
              if (!parent) {
                roots.push(req);
                continue;
              }
              const parentKey = requestNodeKey(parent);
              const list = childrenByParent.get(parentKey) ?? [];
              list.push(req);
              childrenByParent.set(parentKey, list);
            }
            roots.sort((a, b) => b.elapsed_secs - a.elapsed_secs);

            return (
              <div class="card" key={chainId} style="margin-bottom: 12px">
                <div class="card-head">
                  <span class="mono text-purple">Chain</span>
                  <span class="mono" style="margin-left: 8px">{chainId}</span>
                  <span class="muted" style="margin-left: auto">
                    {chainRequests.length} request(s)
                  </span>
                </div>
                <div style="padding: 8px 10px 12px">
                  {roots.map((root) => (
                    <RequestTreeNode
                      key={requestNodeKey(root)}
                      node={root}
                      childrenByParent={childrenByParent}
                      interactionsByTask={interactionsByTask}
                      selectedPath={selectedPath}
                      depth={0}
                      seen={new Set<string>()}
                    />
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
