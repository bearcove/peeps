import type { ProcessDump, ThreadStackSnapshot } from "../types";
import { firstUsefulFrame, classNames } from "../util";
import { Expandable } from "./Expandable";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

interface FlatThread extends ThreadStackSnapshot {
  process: string;
}

function rowSeverity(t: FlatThread): string {
  if (t.same_location_count >= 10) return "severity-danger";
  if (t.same_location_count >= 5) return "severity-warn";
  return "";
}

export function ThreadsView({ dumps, filter, selectedPath }: Props) {
  const threads: FlatThread[] = [];
  for (const d of dumps) {
    for (const t of d.threads) {
      threads.push({ ...t, process: d.process_name });
    }
  }

  const q = filter.toLowerCase();
  const filtered = threads.filter(
    (t) =>
      !q ||
      t.name.toLowerCase().includes(q) ||
      t.process.toLowerCase().includes(q) ||
      (t.dominant_frame?.toLowerCase().includes(q) ?? false)
  );

  if (threads.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">T</div>
        <p>No thread data</p>
      </div>
    );
  }

  return (
    <div class="card fade-in">
      <table>
        <thead>
          <tr>
            <th>Process</th>
            <th>Thread</th>
            <th>Samples</th>
            <th>Responded</th>
            <th>Stuck</th>
            <th>Top Frame</th>
            <th>Backtrace</th>
          </tr>
        </thead>
        <tbody>
          {filtered.map((t, i) => (
            <tr key={i} class={rowSeverity(t)}>
              <td class="mono">{t.process}</td>
              <td class="mono">
                <ResourceLink
                  href={resourceHref({ kind: "thread", process: t.process, thread: t.name })}
                  active={isActivePath(selectedPath, resourceHref({ kind: "thread", process: t.process, thread: t.name }))}
                >
                  {t.name}
                </ResourceLink>
              </td>
              <td class="num">{t.samples}</td>
              <td class="num">{t.responded}</td>
              <td class={classNames("num", t.same_location_count >= 5 && "text-red")}>
                {t.same_location_count}
              </td>
              <td class="mono" style="max-width:400px;overflow:hidden;text-overflow:ellipsis">
                {t.dominant_frame ?? firstUsefulFrame(t.backtrace) ?? (
                  <span class="muted">{"\u2014"}</span>
                )}
              </td>
              <td>
                <Expandable content={t.backtrace} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
