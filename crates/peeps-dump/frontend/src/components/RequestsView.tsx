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

interface FlatRequest extends RequestSnapshot {
  process: string;
  connection: string;
  peer: string;
}

function rowSeverity(r: FlatRequest): string {
  if (r.elapsed_secs > 10) return "severity-danger";
  if (r.elapsed_secs > 2) return "severity-warn";
  return "";
}

export function RequestsView({ dumps, filter, selectedPath }: Props) {
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
      r.peer.toLowerCase().includes(q)
  );

  if (requests.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">R</div>
        <p>No in-flight requests</p>
      </div>
    );
  }

  return (
    <div class="card fade-in">
      <table>
        <thead>
          <tr>
            <th>Dir</th>
            <th>Process</th>
            <th>Method</th>
            <th>Elapsed</th>
            <th>Connection</th>
            <th>Peer</th>
            <th>Request ID</th>
            <th>Backtrace</th>
          </tr>
        </thead>
        <tbody>
          {filtered.map((r, i) => (
            <tr key={i} class={rowSeverity(r)}>
              <td>
                <span
                  class={classNames(
                    "dir",
                    r.direction === "Outgoing" ? "dir-out" : "dir-in"
                  )}
                >
                  {r.direction === "Outgoing" ? "\u2192" : "\u2190"}
                </span>
              </td>
              <td class="mono">{r.process}</td>
              <td class="mono">
                <ResourceLink
                  href={resourceHref({
                    kind: "request",
                    process: r.process,
                    connection: r.connection,
                    requestId: r.request_id,
                  })}
                  active={isActivePath(
                    selectedPath,
                    resourceHref({
                      kind: "request",
                      process: r.process,
                      connection: r.connection,
                      requestId: r.request_id,
                    }),
                  )}
                >
                  {r.method_name ?? `method_${r.method_id}`}
                </ResourceLink>
              </td>
              <td class="num">{fmtDuration(r.elapsed_secs)}</td>
              <td class="mono">
                <ResourceLink
                  href={resourceHref({ kind: "connection", process: r.process, connection: r.connection })}
                  active={isActivePath(selectedPath, resourceHref({ kind: "connection", process: r.process, connection: r.connection }))}
                >
                  {r.connection}
                </ResourceLink>
              </td>
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
  );
}
