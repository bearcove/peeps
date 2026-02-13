import type { ProcessDump } from "../types";
import { fmtBytes } from "../util";
import { ResourceLink } from "./ResourceLink";
import { isActivePath, resourceHref } from "../routes";

interface Props {
  dumps: ProcessDump[];
  filter: string;
  selectedPath: string;
}

export function ShmView({ dumps, filter, selectedPath }: Props) {
  const q = filter.toLowerCase();

  const segments: { process: string; seg: (typeof dumps)[0]["shm"] extends infer S ? S extends { segments: infer C } ? C extends (infer T)[] ? T : never : never : never }[] = [];
  for (const d of dumps) {
    if (!d.shm) continue;
    for (const seg of d.shm.segments) {
      segments.push({ process: d.process_name, seg });
    }
  }

  const filtered = segments.filter(
    (s) =>
      !q ||
      s.process.toLowerCase().includes(q) ||
      (s.seg.segment_path?.toLowerCase().includes(q) ?? false)
  );

  if (segments.length === 0) {
    return (
      <div class="empty-state fade-in">
        <div class="icon">S</div>
        <p>No SHM segments</p>
      </div>
    );
  }

  return (
    <div class="fade-in">
      {filtered.map((s, i) => (
        <div key={i} class="card" style="margin-bottom: 16px">
          <div class="card-head">
            <span class="mono">
              <ResourceLink
                href={resourceHref({ kind: "process", process: s.process })}
                active={isActivePath(selectedPath, resourceHref({ kind: "process", process: s.process }))}
                kind="process"
              >
                {s.process}
              </ResourceLink>
            </span>
            <span class="muted">/</span>
            <span class="mono">
              <ResourceLink
                href={resourceHref({
                  kind: "shm_segment",
                  process: s.process,
                  segment: s.seg.segment_path ?? "anonymous",
                })}
                active={isActivePath(
                  selectedPath,
                  resourceHref({
                    kind: "shm_segment",
                    process: s.process,
                    segment: s.seg.segment_path ?? "anonymous",
                  }),
                )}
                kind="shm_segment"
              >
                {s.seg.segment_path ?? "anonymous"}
              </ResourceLink>
            </span>
            <span class="muted" style="margin-left: auto">
              {fmtBytes(s.seg.current_size)} / {fmtBytes(s.seg.total_size)}
            </span>
          </div>
          <table>
            <thead>
              <tr>
                <th>Peer</th>
                <th>Name</th>
                <th>State</th>
                <th>Buf Cap</th>
                <th>TX</th>
                <th>RX</th>
                <th>Calls TX</th>
                <th>Calls RX</th>
                <th>Heartbeat</th>
              </tr>
            </thead>
            <tbody>
              {s.seg.peers.map((p) => (
                <tr key={p.peer_id}>
                  <td class="num">
                    <ResourceLink
                      href={resourceHref({
                        kind: "shm_peer",
                        process: s.process,
                        segment: s.seg.segment_path ?? "anonymous",
                        peerId: p.peer_id,
                      })}
                      active={isActivePath(
                        selectedPath,
                        resourceHref({
                          kind: "shm_peer",
                          process: s.process,
                          segment: s.seg.segment_path ?? "anonymous",
                          peerId: p.peer_id,
                        }),
                      )}
                      kind="shm_peer"
                    >
                      {p.peer_id}
                    </ResourceLink>
                  </td>
                  <td class="mono">{p.name ?? <span class="muted">{"\u2014"}</span>}</td>
                  <td class="mono">{p.state}</td>
                  <td class="num">{fmtBytes(p.bipbuf_capacity)}</td>
                  <td class="num">{fmtBytes(p.bytes_sent)}</td>
                  <td class="num">{fmtBytes(p.bytes_received)}</td>
                  <td class="num">{p.calls_sent}</td>
                  <td class="num">{p.calls_received}</td>
                  <td class="num">
                    {p.time_since_heartbeat_ms != null
                      ? p.time_since_heartbeat_ms + "ms"
                      : "\u2014"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
    </div>
  );
}
