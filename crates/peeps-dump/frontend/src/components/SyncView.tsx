import type { ProcessDump } from "../types";
import { fmtAge, classNames } from "../util";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

function stateClass(state: string): string {
  switch (state) {
    case "Pending":
      return "state-pending";
    case "Sent":
    case "Received":
    case "Initialized":
      return "state-completed";
    case "Initializing":
      return "state-initializing";
    case "SenderDropped":
    case "ReceiverDropped":
      return "state-dropped";
    case "Empty":
      return "state-empty";
    default:
      return "";
  }
}

function taskRef(process: string, id: number | null, name: string | null, selectedPath: string) {
  if (id == null) return <span class="muted">{"\u2014"}</span>;
  const href = resourceHref({ kind: "task", process, taskId: id });
  return (
    <ResourceLink href={href} active={isActivePath(selectedPath, href)}>
      {name ?? ""} (#{id})
    </ResourceLink>
  );
}

export function SyncView({ dumps, filter, selectedPath }: Props) {
  const q = filter.toLowerCase();

  const mpsc: { process: string; ch: (typeof dumps)[0]["sync"] extends infer S ? S extends { mpsc_channels: infer C } ? C extends (infer T)[] ? T : never : never : never }[] = [];
  const oneshot: { process: string; ch: (typeof dumps)[0]["sync"] extends infer S ? S extends { oneshot_channels: infer C } ? C extends (infer T)[] ? T : never : never : never }[] = [];
  const watch: { process: string; ch: (typeof dumps)[0]["sync"] extends infer S ? S extends { watch_channels: infer C } ? C extends (infer T)[] ? T : never : never : never }[] = [];
  const once: { process: string; ch: (typeof dumps)[0]["sync"] extends infer S ? S extends { once_cells: infer C } ? C extends (infer T)[] ? T : never : never : never }[] = [];

  for (const d of dumps) {
    if (!d.sync) continue;
    for (const ch of d.sync.mpsc_channels)
      mpsc.push({ process: d.process_name, ch });
    for (const ch of d.sync.oneshot_channels)
      oneshot.push({ process: d.process_name, ch });
    for (const ch of d.sync.watch_channels)
      watch.push({ process: d.process_name, ch });
    for (const ch of d.sync.once_cells)
      once.push({ process: d.process_name, ch });
  }

  const filterMatch = (process: string, name: string) =>
    !q || process.toLowerCase().includes(q) || name.toLowerCase().includes(q);

  return (
    <div class="fade-in">
      {mpsc.length > 0 && (
        <div class="card" style="margin-bottom: 16px">
          <div class="card-head">MPSC Channels</div>
          <table>
            <thead>
              <tr>
                <th>Process</th>
                <th>Name</th>
                <th>Type</th>
                <th>Sent</th>
                <th>Recv</th>
                <th>Senders</th>
                <th>Waiters</th>
                <th>Age</th>
                <th>Creator</th>
              </tr>
            </thead>
            <tbody>
              {mpsc
                .filter((m) => filterMatch(m.process, m.ch.name))
                .map((m, i) => (
                  <tr
                    key={i}
                    class={classNames(
                      m.ch.send_waiters > 0 && "severity-warn",
                      (m.ch.sender_closed || m.ch.receiver_closed) && "severity-danger"
                    )}
                  >
                    <td class="mono">{m.process}</td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "mpsc", process: m.process, name: m.ch.name })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "mpsc", process: m.process, name: m.ch.name }))}
                      >
                        {m.ch.name}
                      </ResourceLink>
                    </td>
                    <td class="mono">
                      {m.ch.bounded ? `bounded(${m.ch.capacity})` : "unbounded"}
                    </td>
                    <td class="num">{m.ch.sent}</td>
                    <td class="num">{m.ch.received}</td>
                    <td class="num">{m.ch.sender_count}</td>
                    <td class={classNames("num", m.ch.send_waiters > 0 && "text-amber")}>
                      {m.ch.send_waiters}
                    </td>
                    <td class="num">{fmtAge(m.ch.age_secs)}</td>
                    <td>{taskRef(m.process, m.ch.creator_task_id, m.ch.creator_task_name, selectedPath)}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}

      {oneshot.length > 0 && (
        <div class="card" style="margin-bottom: 16px">
          <div class="card-head">Oneshot Channels</div>
          <table>
            <thead>
              <tr>
                <th>Process</th>
                <th>Name</th>
                <th>State</th>
                <th>Age</th>
                <th>Creator</th>
              </tr>
            </thead>
            <tbody>
              {oneshot
                .filter((o) => filterMatch(o.process, o.ch.name))
                .map((o, i) => (
                  <tr key={i}>
                    <td class="mono">{o.process}</td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "oneshot", process: o.process, name: o.ch.name })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "oneshot", process: o.process, name: o.ch.name }))}
                      >
                        {o.ch.name}
                      </ResourceLink>
                    </td>
                    <td>
                      <span class={classNames("state-badge", stateClass(o.ch.state))}>
                        {o.ch.state}
                      </span>
                    </td>
                    <td class="num">{fmtAge(o.ch.age_secs)}</td>
                    <td>{taskRef(o.process, o.ch.creator_task_id, o.ch.creator_task_name, selectedPath)}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}

      {watch.length > 0 && (
        <div class="card" style="margin-bottom: 16px">
          <div class="card-head">Watch Channels</div>
          <table>
            <thead>
              <tr>
                <th>Process</th>
                <th>Name</th>
                <th>Changes</th>
                <th>Receivers</th>
                <th>Age</th>
                <th>Creator</th>
              </tr>
            </thead>
            <tbody>
              {watch
                .filter((w) => filterMatch(w.process, w.ch.name))
                .map((w, i) => (
                  <tr key={i}>
                    <td class="mono">{w.process}</td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "watch", process: w.process, name: w.ch.name })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "watch", process: w.process, name: w.ch.name }))}
                      >
                        {w.ch.name}
                      </ResourceLink>
                    </td>
                    <td class="num">{w.ch.changes}</td>
                    <td class="num">{w.ch.receiver_count}</td>
                    <td class="num">{fmtAge(w.ch.age_secs)}</td>
                    <td>{taskRef(w.process, w.ch.creator_task_id, w.ch.creator_task_name, selectedPath)}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}

      {once.length > 0 && (
        <div class="card">
          <div class="card-head">OnceCells</div>
          <table>
            <thead>
              <tr>
                <th>Process</th>
                <th>Name</th>
                <th>State</th>
                <th>Age</th>
                <th>Init Duration</th>
              </tr>
            </thead>
            <tbody>
              {once
                .filter((o) => filterMatch(o.process, o.ch.name))
                .map((o, i) => (
                  <tr key={i}>
                    <td class="mono">{o.process}</td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "once_cell", process: o.process, name: o.ch.name })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "once_cell", process: o.process, name: o.ch.name }))}
                      >
                        {o.ch.name}
                      </ResourceLink>
                    </td>
                    <td>
                      <span class={classNames("state-badge", stateClass(o.ch.state))}>
                        {o.ch.state}
                      </span>
                    </td>
                    <td class="num">{fmtAge(o.ch.age_secs)}</td>
                    <td class="num">
                      {o.ch.init_duration_secs != null
                        ? (o.ch.init_duration_secs * 1000).toFixed(0) + "ms"
                        : "\u2014"}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
