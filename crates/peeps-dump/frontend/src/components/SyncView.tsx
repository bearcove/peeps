import type { ProcessDump, MpscChannelSnapshot, OneshotChannelSnapshot, WatchChannelSnapshot, OnceCellSnapshot } from "../types";
import { fmtAge, classNames } from "../util";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

type Severity = "danger" | "warn" | "idle";

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

function severityRank(s: Severity): number {
  if (s === "danger") return 2;
  if (s === "warn") return 1;
  return 0;
}

function severityBadge(s: Severity) {
  if (s === "danger") return <span class="state-badge state-dropped">danger</span>;
  if (s === "warn") return <span class="state-badge state-pending">warn</span>;
  return <span class="state-badge state-empty">idle</span>;
}

function rowClass(s: Severity): string {
  if (s === "danger") return "severity-danger";
  if (s === "warn") return "severity-warn";
  return "";
}

function taskRef(process: string, id: number | null, name: string | null, selectedPath: string) {
  if (id == null) return <span class="muted">{"\u2014"}</span>;
  const href = resourceHref({ kind: "task", process, taskId: id });
  return (
    <ResourceLink href={href} active={isActivePath(selectedPath, href)} kind="task">
      {name ?? ""} (#{id})
    </ResourceLink>
  );
}

function processRef(process: string, selectedPath: string) {
  const href = resourceHref({ kind: "process", process });
  return (
    <ResourceLink href={href} active={isActivePath(selectedPath, href)} kind="process">
      {process}
    </ResourceLink>
  );
}

function mpscSeverity(ch: MpscChannelSnapshot): Severity {
  if (ch.sender_closed || ch.receiver_closed) return "danger";
  if (ch.send_waiters > 0) return "warn";
  return "idle";
}

function oneshotSeverity(ch: OneshotChannelSnapshot): Severity {
  if (ch.state === "SenderDropped" || ch.state === "ReceiverDropped") return "danger";
  if (ch.state === "Pending" && ch.age_secs > 10) return "warn";
  return "idle";
}

function watchSeverity(ch: WatchChannelSnapshot): Severity {
  if (ch.receiver_count === 0 && ch.age_secs > 30) return "warn";
  return "idle";
}

function onceSeverity(ch: OnceCellSnapshot): Severity {
  if (ch.state === "Initializing" && ch.age_secs > 5) return "warn";
  return "idle";
}

export function SyncView({ dumps, filter, selectedPath }: Props) {
  const q = filter.toLowerCase();

  const mpsc: { process: string; ch: MpscChannelSnapshot; severity: Severity }[] = [];
  const oneshot: { process: string; ch: OneshotChannelSnapshot; severity: Severity }[] = [];
  const watch: { process: string; ch: WatchChannelSnapshot; severity: Severity }[] = [];
  const once: { process: string; ch: OnceCellSnapshot; severity: Severity }[] = [];

  for (const d of dumps) {
    if (!d.sync) continue;
    for (const ch of d.sync.mpsc_channels) mpsc.push({ process: d.process_name, ch, severity: mpscSeverity(ch) });
    for (const ch of d.sync.oneshot_channels) oneshot.push({ process: d.process_name, ch, severity: oneshotSeverity(ch) });
    for (const ch of d.sync.watch_channels) watch.push({ process: d.process_name, ch, severity: watchSeverity(ch) });
    for (const ch of d.sync.once_cells) once.push({ process: d.process_name, ch, severity: onceSeverity(ch) });
  }

  const filterMatch = (process: string, name: string) =>
    !q || process.toLowerCase().includes(q) || name.toLowerCase().includes(q);

  const mpscFiltered = mpsc
    .filter((m) => filterMatch(m.process, m.ch.name))
    .sort((a, b) => {
      if (a.severity !== b.severity) return severityRank(b.severity) - severityRank(a.severity);
      if (a.ch.send_waiters !== b.ch.send_waiters) return b.ch.send_waiters - a.ch.send_waiters;
      if (a.ch.sender_count !== b.ch.sender_count) return b.ch.sender_count - a.ch.sender_count;
      return b.ch.age_secs - a.ch.age_secs;
    });

  const oneshotFiltered = oneshot
    .filter((o) => filterMatch(o.process, o.ch.name))
    .sort((a, b) => {
      if (a.severity !== b.severity) return severityRank(b.severity) - severityRank(a.severity);
      return b.ch.age_secs - a.ch.age_secs;
    });

  const watchFiltered = watch
    .filter((w) => filterMatch(w.process, w.ch.name))
    .sort((a, b) => {
      if (a.severity !== b.severity) return severityRank(b.severity) - severityRank(a.severity);
      if (a.ch.receiver_count !== b.ch.receiver_count) return a.ch.receiver_count - b.ch.receiver_count;
      return b.ch.age_secs - a.ch.age_secs;
    });

  const onceFiltered = once
    .filter((o) => filterMatch(o.process, o.ch.name))
    .sort((a, b) => {
      if (a.severity !== b.severity) return severityRank(b.severity) - severityRank(a.severity);
      return b.ch.age_secs - a.ch.age_secs;
    });

  const mpscHot = mpscFiltered.filter((m) => m.severity !== "idle");
  const mpscIdle = mpscFiltered.filter((m) => m.severity === "idle");

  const oneshotHot = oneshotFiltered.filter((o) => o.severity !== "idle");
  const oneshotIdle = oneshotFiltered.filter((o) => o.severity === "idle");

  const watchHot = watchFiltered.filter((w) => w.severity !== "idle");
  const watchIdle = watchFiltered.filter((w) => w.severity === "idle");

  const onceHot = onceFiltered.filter((o) => o.severity !== "idle");
  const onceIdle = onceFiltered.filter((o) => o.severity === "idle");

  return (
    <div class="fade-in">
      {mpsc.length > 0 && (
        <div class="card" style="margin-bottom: 16px">
          <div class="card-head">
            MPSC Channels
            <span class="muted" style="margin-left: auto">sender count alone is usually not a problem; waiters/closed ends are</span>
          </div>
          {mpscHot.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th>Severity</th>
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
                {mpscHot.map((m, i) => (
                  <tr key={i} class={rowClass(m.severity)}>
                    <td>{severityBadge(m.severity)}</td>
                    <td class="mono">{processRef(m.process, selectedPath)}</td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "mpsc", process: m.process, name: m.ch.name })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "mpsc", process: m.process, name: m.ch.name }))}
                        kind="mpsc"
                      >
                        {m.ch.name}
                      </ResourceLink>
                    </td>
                    <td class="mono">{m.ch.bounded ? `bounded(${m.ch.capacity})` : "unbounded"}</td>
                    <td class="num">{m.ch.sent}</td>
                    <td class="num">{m.ch.received}</td>
                    <td class="num">{m.ch.sender_count}</td>
                    <td class={classNames("num", m.ch.send_waiters > 0 && "text-amber")}>{m.ch.send_waiters}</td>
                    <td class="num">{fmtAge(m.ch.age_secs)}</td>
                    <td>{taskRef(m.process, m.ch.creator_task_id, m.ch.creator_task_name, selectedPath)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}

          {mpscIdle.length > 0 && (
            <details style="padding: 8px 12px 12px">
              <summary class="muted" style="cursor: pointer">Idle channels ({mpscIdle.length})</summary>
              <table style="margin-top: 8px">
                <thead>
                  <tr>
                    <th>Severity</th>
                    <th>Process</th>
                    <th>Name</th>
                    <th>Type</th>
                    <th>Senders</th>
                    <th>Waiters</th>
                    <th>Age</th>
                  </tr>
                </thead>
                <tbody>
                  {mpscIdle.map((m, i) => (
                    <tr key={i}>
                      <td>{severityBadge(m.severity)}</td>
                      <td class="mono">{processRef(m.process, selectedPath)}</td>
                      <td class="mono">
                        <ResourceLink
                          href={resourceHref({ kind: "mpsc", process: m.process, name: m.ch.name })}
                          active={isActivePath(selectedPath, resourceHref({ kind: "mpsc", process: m.process, name: m.ch.name }))}
                          kind="mpsc"
                        >
                          {m.ch.name}
                        </ResourceLink>
                      </td>
                      <td class="mono">{m.ch.bounded ? `bounded(${m.ch.capacity})` : "unbounded"}</td>
                      <td class="num">{m.ch.sender_count}</td>
                      <td class="num">{m.ch.send_waiters}</td>
                      <td class="num">{fmtAge(m.ch.age_secs)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </details>
          )}
        </div>
      )}

      {oneshot.length > 0 && (
        <div class="card" style="margin-bottom: 16px">
          <div class="card-head">Oneshot Channels</div>
          {oneshotHot.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th>Severity</th>
                  <th>Process</th>
                  <th>Name</th>
                  <th>State</th>
                  <th>Age</th>
                  <th>Creator</th>
                </tr>
              </thead>
              <tbody>
                {oneshotHot.map((o, i) => (
                  <tr key={i} class={rowClass(o.severity)}>
                    <td>{severityBadge(o.severity)}</td>
                    <td class="mono">{processRef(o.process, selectedPath)}</td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "oneshot", process: o.process, name: o.ch.name })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "oneshot", process: o.process, name: o.ch.name }))}
                        kind="oneshot"
                      >
                        {o.ch.name}
                      </ResourceLink>
                    </td>
                    <td><span class={classNames("state-badge", stateClass(o.ch.state))}>{o.ch.state}</span></td>
                    <td class="num">{fmtAge(o.ch.age_secs)}</td>
                    <td>{taskRef(o.process, o.ch.creator_task_id, o.ch.creator_task_name, selectedPath)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {oneshotIdle.length > 0 && (
            <details style="padding: 8px 12px 12px">
              <summary class="muted" style="cursor: pointer">Idle channels ({oneshotIdle.length})</summary>
            </details>
          )}
        </div>
      )}

      {watch.length > 0 && (
        <div class="card" style="margin-bottom: 16px">
          <div class="card-head">Watch Channels</div>
          {watchHot.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th>Severity</th>
                  <th>Process</th>
                  <th>Name</th>
                  <th>Changes</th>
                  <th>Receivers</th>
                  <th>Age</th>
                  <th>Creator</th>
                </tr>
              </thead>
              <tbody>
                {watchHot.map((w, i) => (
                  <tr key={i} class={rowClass(w.severity)}>
                    <td>{severityBadge(w.severity)}</td>
                    <td class="mono">{processRef(w.process, selectedPath)}</td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "watch", process: w.process, name: w.ch.name })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "watch", process: w.process, name: w.ch.name }))}
                        kind="watch"
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
          )}
          {watchIdle.length > 0 && (
            <details style="padding: 8px 12px 12px">
              <summary class="muted" style="cursor: pointer">Idle channels ({watchIdle.length})</summary>
            </details>
          )}
        </div>
      )}

      {once.length > 0 && (
        <div class="card">
          <div class="card-head">OnceCells</div>
          {onceHot.length > 0 && (
            <table>
              <thead>
                <tr>
                  <th>Severity</th>
                  <th>Process</th>
                  <th>Name</th>
                  <th>State</th>
                  <th>Age</th>
                  <th>Init Duration</th>
                </tr>
              </thead>
              <tbody>
                {onceHot.map((o, i) => (
                  <tr key={i} class={rowClass(o.severity)}>
                    <td>{severityBadge(o.severity)}</td>
                    <td class="mono">{processRef(o.process, selectedPath)}</td>
                    <td class="mono">
                      <ResourceLink
                        href={resourceHref({ kind: "once_cell", process: o.process, name: o.ch.name })}
                        active={isActivePath(selectedPath, resourceHref({ kind: "once_cell", process: o.process, name: o.ch.name }))}
                        kind="once_cell"
                      >
                        {o.ch.name}
                      </ResourceLink>
                    </td>
                    <td><span class={classNames("state-badge", stateClass(o.ch.state))}>{o.ch.state}</span></td>
                    <td class="num">{fmtAge(o.ch.age_secs)}</td>
                    <td class="num">{o.ch.init_duration_secs != null ? (o.ch.init_duration_secs * 1000).toFixed(0) + "ms" : "\u2014"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {onceIdle.length > 0 && (
            <details style="padding: 8px 12px 12px">
              <summary class="muted" style="cursor: pointer">Idle cells ({onceIdle.length})</summary>
            </details>
          )}
        </div>
      )}
    </div>
  );
}
