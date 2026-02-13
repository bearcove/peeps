import type { ProcessDump } from "../types";
import { fmtBytes } from "../util";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

export function ProcessesView({ dumps, filter, selectedPath }: Props) {
  const q = filter.toLowerCase();
  const filtered = dumps.filter(
    (d) => !q || d.process_name.toLowerCase().includes(q)
  );

  if (dumps.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">P</div>
        <p>No processes</p>
        <p class="sub">
          Waiting for instrumented processes to connect...
        </p>
      </div>
    );
  }

  return (
    <div class="card-grid fade-in">
      {filtered.map((d) => (
        <ProcessCard key={`${d.process_name}-${d.pid}`} dump={d} selectedPath={selectedPath} />
      ))}
    </div>
  );
}

function ProcessCard({ dump: d, selectedPath }: { dump: ProcessDump; selectedPath: string }) {
  const connCount = d.roam?.connections.length ?? 0;
  const inFlight = d.roam?.connections.reduce(
    (s, c) => s + c.in_flight.length,
    0
  ) ?? 0;
  const totalCompleted = d.roam?.connections.reduce(
    (s, c) => s + c.total_completed,
    0
  ) ?? 0;
  const txBytes = d.roam?.connections.reduce(
    (s, c) => s + c.transport.bytes_sent,
    0
  ) ?? 0;
  const rxBytes = d.roam?.connections.reduce(
    (s, c) => s + c.transport.bytes_received,
    0
  ) ?? 0;

  return (
    <div class="proc-card">
      <div class="proc-card-head">
        <div class="dot" style="background: var(--purple)" />
        <div class="name">
          <ResourceLink
            href={resourceHref({ kind: "process", process: d.process_name, pid: d.pid })}
            active={isActivePath(selectedPath, resourceHref({ kind: "process", process: d.process_name, pid: d.pid }))}
          >
            {d.process_name}
          </ResourceLink>
        </div>
        <div class="pid">pid {d.pid}</div>
      </div>
      <div class="proc-card-body">
        <div class="proc-kv">
          <div class="proc-kv-item">
            <span class="k">Tasks</span>
            <span class="v">{d.tasks.length}</span>
          </div>
          <div class="proc-kv-item">
            <span class="k">Threads</span>
            <span class="v">{d.threads.length}</span>
          </div>
          {connCount > 0 && (
            <>
              <div class="proc-kv-item">
                <span class="k">Connections</span>
                <span class="v">{connCount}</span>
              </div>
              <div class="proc-kv-item">
                <span class="k">In-flight</span>
                <span class="v">{inFlight}</span>
              </div>
              <div class="proc-kv-item">
                <span class="k">Completed</span>
                <span class="v">{totalCompleted}</span>
              </div>
              <div class="proc-kv-item">
                <span class="k">TX / RX</span>
                <span class="v">
                  {fmtBytes(txBytes)} / {fmtBytes(rxBytes)}
                </span>
              </div>
            </>
          )}
          {d.locks && (
            <div class="proc-kv-item">
              <span class="k">Contended locks</span>
              <span class="v">{d.locks.locks.length}</span>
            </div>
          )}
        </div>
        <div class="proc-section" style="font-size: 11px; color: var(--fg-muted)">
          Timestamp: {d.timestamp}
        </div>
      </div>
    </div>
  );
}
