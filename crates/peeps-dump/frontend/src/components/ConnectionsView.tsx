import type { ProcessDump, ConnectionSnapshot, RequestSnapshot } from "../types";
import { fmtBytes, fmtDuration } from "../util";
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

type ConnDirection = "outgoing" | "incoming" | "unknown";

function connDirection(name: string): ConnDirection {
  if (name === "client") return "outgoing";
  if (name === "server") return "incoming";
  return "unknown";
}

interface PairedConnection {
  kind: "paired";
  from: FlatConnection;
  to: FlatConnection;
}

interface UnpairedConnection {
  kind: "unpaired";
  conn: FlatConnection;
  dir: ConnDirection;
}

type ConnectionEntry = PairedConnection | UnpairedConnection;

function entryInFlight(e: ConnectionEntry): number {
  if (e.kind === "paired") return e.from.in_flight.length + e.to.in_flight.length;
  return e.conn.in_flight.length;
}

function entryBuffered(e: ConnectionEntry): number {
  if (e.kind === "paired") {
    const abDelta = Math.abs(e.from.transport.bytes_sent - e.to.transport.bytes_received);
    const baDelta = Math.abs(e.to.transport.bytes_sent - e.from.transport.bytes_received);
    return abDelta + baDelta;
  }
  return 0;
}

function buildEntries(conns: FlatConnection[]): ConnectionEntry[] {
  const outgoing: FlatConnection[] = [];
  const incoming: FlatConnection[] = [];
  const other: FlatConnection[] = [];

  for (const c of conns) {
    const dir = connDirection(c.name);
    if (dir === "outgoing") outgoing.push(c);
    else if (dir === "incoming") incoming.push(c);
    else other.push(c);
  }

  const entries: ConnectionEntry[] = [];
  const matchedIncoming = new Set<number>();

  for (const out of outgoing) {
    if (!out.peer_name) {
      entries.push({ kind: "unpaired", conn: out, dir: "outgoing" });
      continue;
    }
    const inIdx = incoming.findIndex(
      (inc, idx) =>
        !matchedIncoming.has(idx) && inc.process === out.peer_name && inc.peer_name === out.process,
    );
    if (inIdx >= 0) {
      matchedIncoming.add(inIdx);
      entries.push({ kind: "paired", from: out, to: incoming[inIdx] });
    } else {
      entries.push({ kind: "unpaired", conn: out, dir: "outgoing" });
    }
  }

  for (let i = 0; i < incoming.length; i++) {
    if (!matchedIncoming.has(i)) {
      entries.push({ kind: "unpaired", conn: incoming[i], dir: "incoming" });
    }
  }

  for (const c of other) {
    entries.push({ kind: "unpaired", conn: c, dir: "unknown" });
  }

  entries.sort((a, b) => {
    const inf = entryInFlight(b) - entryInFlight(a);
    if (inf !== 0) return inf;
    const buf = entryBuffered(b) - entryBuffered(a);
    if (buf !== 0) return buf;
    const compA =
      a.kind === "paired" ? a.from.total_completed + a.to.total_completed : a.conn.total_completed;
    const compB =
      b.kind === "paired" ? b.from.total_completed + b.to.total_completed : b.conn.total_completed;
    return compB - compA;
  });

  return entries;
}

function ProcessBadge({ process, selectedPath }: { process: string; selectedPath: string }) {
  return (
    <ResourceLink
      href={resourceHref({ kind: "process", process })}
      active={isActivePath(selectedPath, resourceHref({ kind: "process", process }))}
      kind="process"
    >
      {process}
    </ResourceLink>
  );
}

function DiffCell({ a, b, fmt }: { a: number; b: number; fmt: (v: number) => string }) {
  if (a === b) return <>{fmt(a)}</>;
  return (
    <>
      <span class="text-red">{fmt(a)}</span>
      {" / "}
      <span class="text-red">{fmt(b)}</span>
    </>
  );
}

function FlowCell({ sent, received }: { sent: number; received: number }) {
  if (sent === received) return <>{fmtBytes(sent)}</>;
  const delta = sent - received;
  return (
    <>
      {fmtBytes(sent)}
      <span
        class="text-red"
        title={`${fmtBytes(Math.abs(delta))} ${delta > 0 ? "buffered" : "extra"}`}
      >
        {" "}
        (+{fmtBytes(Math.abs(delta))})
      </span>
    </>
  );
}

function InFlightList({ requests }: { requests: RequestSnapshot[] }) {
  if (requests.length === 0) return <span class="muted">0</span>;
  const shown = requests.slice(0, 5);
  const remaining = requests.length - shown.length;
  return (
    <div class="in-flight-list">
      <span class="text-amber">{requests.length}</span>
      {shown.map((r, i) => (
        <span key={i} class="in-flight-item" title={`req#${r.request_id} ${r.direction}`}>
          <span class="in-flight-method">{r.method_name ?? `#${r.method_id}`}</span>
          <span class="muted">{fmtDuration(r.elapsed_secs)}</span>
        </span>
      ))}
      {remaining > 0 && <span class="muted">+{remaining} more</span>}
    </div>
  );
}

function lastActivity(c: FlatConnection): string {
  if (c.transport.last_recv_ago_secs != null) {
    return fmtDuration(c.transport.last_recv_ago_secs) + " ago";
  }
  return "\u2014";
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
      (c.peer_name?.toLowerCase().includes(q) ?? false),
  );

  if (conns.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">C</div>
        <p>No connections</p>
      </div>
    );
  }

  const entries = buildEntries(filtered);

  return (
    <div class="card fade-in">
      <table>
        <thead>
          <tr>
            <th>Connection</th>
            <th>In-Flight</th>
            <th>Completed</th>
            <th>Channels</th>
            <th>{"A → B"}</th>
            <th>{"B → A"}</th>
            <th>Last Activity</th>
          </tr>
        </thead>
        <tbody>
          {entries.map((entry, i) => {
            if (entry.kind === "paired") {
              const { from, to } = entry;
              const allInFlight = [...from.in_flight, ...to.in_flight].sort(
                (a, b) => b.elapsed_secs - a.elapsed_secs,
              );
              return (
                <tr key={i}>
                  <td class="mono conn-endpoints">
                    <ProcessBadge process={from.process} selectedPath={selectedPath} />
                    <span class="dir">{"⇄"}</span>
                    <ProcessBadge process={to.process} selectedPath={selectedPath} />
                  </td>
                  <td class="num">
                    <InFlightList requests={allInFlight} />
                  </td>
                  <td class="num">
                    <DiffCell a={from.total_completed} b={to.total_completed} fmt={String} />
                  </td>
                  <td class="num">
                    <DiffCell a={from.channels.length} b={to.channels.length} fmt={String} />
                  </td>
                  <td class="num">
                    <FlowCell
                      sent={from.transport.bytes_sent}
                      received={to.transport.bytes_received}
                    />
                  </td>
                  <td class="num">
                    <FlowCell
                      sent={to.transport.bytes_sent}
                      received={from.transport.bytes_received}
                    />
                  </td>
                  <td class="num">{lastActivity(from)}</td>
                </tr>
              );
            } else {
              const { conn } = entry;
              return (
                <tr key={i}>
                  <td class="mono conn-endpoints">
                    <ProcessBadge process={conn.process} selectedPath={selectedPath} />
                    <span class="dir">{"⇄"}</span>
                    {conn.peer_name ? (
                      <span class="muted">{conn.peer_name}</span>
                    ) : (
                      <span class="muted">{"?"}</span>
                    )}
                  </td>
                  <td class="num">
                    <InFlightList requests={conn.in_flight} />
                  </td>
                  <td class="num">{conn.total_completed}</td>
                  <td class="num">{conn.channels.length}</td>
                  <td class="num">{fmtBytes(conn.transport.bytes_sent)}</td>
                  <td class="num">{fmtBytes(conn.transport.bytes_received)}</td>
                  <td class="num">{lastActivity(conn)}</td>
                </tr>
              );
            }
          })}
        </tbody>
      </table>
    </div>
  );
}
