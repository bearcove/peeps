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

function RequestTreeNode({
  node,
  childrenByParent,
  selectedPath,
  depth,
  seen,
}: {
  node: FlatRequest;
  childrenByParent: Map<string, FlatRequest[]>;
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
      {kids.map((child) => (
        <RequestTreeNode
          key={requestNodeKey(child)}
          node={child}
          childrenByParent={childrenByParent}
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
        <div class="card">
          <table>
            <thead>
              <tr>
                <th>Dir</th>
                <th>Process</th>
                <th>Method</th>
                <th>Task</th>
                <th>Elapsed</th>
                <th>Connection</th>
                <th>Chain</th>
                <th>Span</th>
                <th>Peer</th>
                <th>Request ID</th>
                <th>Backtrace</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((r) => (
                <tr key={requestNodeKey(r)} class={rowSeverity(r)}>
                  <td>
                    <span
                      class={classNames("dir", r.direction === "Outgoing" ? "dir-out" : "dir-in")}
                    >
                      {r.direction === "Outgoing" ? "\u2192" : "\u2190"}
                    </span>
                  </td>
                  <td class="mono">
                    <ResourceLink
                      href={resourceHref({ kind: "process", process: r.process })}
                      active={isActivePath(selectedPath, resourceHref({ kind: "process", process: r.process }))}
                      kind="process"
                    >
                      {r.process}
                    </ResourceLink>
                  </td>
                  <td class="mono">
                    <RequestLink r={r} selectedPath={selectedPath} />
                  </td>
                  <td class="mono">
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
                  </td>
                  <td class="num">{fmtDuration(r.elapsed_secs)}</td>
                  <td class="mono">
                    <ResourceLink
                      href={resourceHref({ kind: "connection", process: r.process, connection: r.connection })}
                      active={isActivePath(selectedPath, resourceHref({ kind: "connection", process: r.process, connection: r.connection }))}
                      kind="connection"
                    >
                      {r.connection}
                    </ResourceLink>
                  </td>
                  <td class="mono">{r.chain_id ?? "—"}</td>
                  <td class="mono">{r.span_id ?? "—"}</td>
                  <td class="mono">{r.peer}</td>
                  <td class="num">{r.request_id}</td>
                  <td>
                    <Expandable content={r.backtrace} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
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
