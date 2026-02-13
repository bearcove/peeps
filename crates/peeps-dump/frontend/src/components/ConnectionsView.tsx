import type { ProcessDump, ConnectionSnapshot } from "../types";
import { fmtAge, fmtBytes, fmtDuration } from "../util";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

interface FlatConnection extends ConnectionSnapshot {
  process: string;
}

export function ConnectionsView({ dumps, filter, selectedPath }: Props) {
  const conns: FlatConnection[] = [];
  for (const d of dumps) {
    if (!d.roam) continue;
    for (const c of d.roam.connections) {
      conns.push({ ...c, process: d.process_name });
    }
  }

  const q = filter.toLowerCase();
  const filtered = conns.filter(
    (c) =>
      !q ||
      c.process.toLowerCase().includes(q) ||
      c.name.toLowerCase().includes(q) ||
      (c.peer_name?.toLowerCase().includes(q) ?? false)
  );

  if (conns.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">C</div>
        <p>No connections</p>
      </div>
    );
  }

  return (
    <div class="card fade-in">
      <table>
        <thead>
          <tr>
            <th>Process</th>
            <th>Name</th>
            <th>Peer</th>
            <th>Age</th>
            <th>In-Flight</th>
            <th>Completed</th>
            <th>Channels</th>
            <th>TX</th>
            <th>RX</th>
            <th>Last Activity</th>
          </tr>
        </thead>
        <tbody>
          {filtered.map((c, i) => (
            <tr key={i}>
              <td class="mono">{c.process}</td>
              <td class="mono">
                <ResourceLink
                  href={resourceHref({ kind: "connection", process: c.process, connection: c.name })}
                  active={isActivePath(selectedPath, resourceHref({ kind: "connection", process: c.process, connection: c.name }))}
                >
                  {c.name}
                </ResourceLink>
              </td>
              <td class="mono">{c.peer_name ?? <span class="muted">?</span>}</td>
              <td class="num">{fmtAge(c.age_secs)}</td>
              <td class="num">{c.in_flight.length}</td>
              <td class="num">{c.total_completed}</td>
              <td class="num">{c.channels.length}</td>
              <td class="num">
                {fmtBytes(c.transport.bytes_sent)}
                <span class="muted"> ({c.transport.frames_sent}f)</span>
              </td>
              <td class="num">
                {fmtBytes(c.transport.bytes_received)}
                <span class="muted"> ({c.transport.frames_received}f)</span>
              </td>
              <td class="num">
                {c.transport.last_recv_ago_secs != null
                  ? fmtDuration(c.transport.last_recv_ago_secs) + " ago"
                  : "\u2014"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
