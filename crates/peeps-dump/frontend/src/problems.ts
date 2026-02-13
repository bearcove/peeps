import type { ProcessDump } from "./types";
import {
  firstUsefulFrame,
  fmtDuration,
  fmtAge,
  isLikelyIdleBacktrace,
  isLikelyIdleFrameName,
} from "./util";

export type Severity = "danger" | "warn";
export type ProblemCategory =
  | "Tasks"
  | "Threads"
  | "Locks"
  | "Channels"
  | "RPC"
  | "SHM";

export interface Problem {
  severity: Severity;
  category: ProblemCategory;
  process: string;
  resource: string;
  description: string;
  timing: number;
  timingLabel: string;
  backtrace: string | null;
}

export interface RelationshipIssue {
  severity: Severity;
  category: "Locks" | "Channels" | "RPC";
  process: string;
  blocked: string;
  waitsOn: string;
  owner: string | null;
  description: string;
  timing: number;
  timingLabel: string;
  count: number;
  backtrace: string | null;
}

export interface RootCauseSummary {
  severity: Severity;
  owner: string;
  blockedCount: number;
  edgeCount: number;
  worstTiming: number;
  worstTimingLabel: string;
}

export function hasDanger(problems: Problem[]): boolean {
  return problems.some((p) => p.severity === "danger");
}

export function detectProblems(dumps: ProcessDump[]): Problem[] {
  const problems: Problem[] = [];

  for (const dump of dumps) {
    const proc = dump.process_name;

    detectTasks(proc, dump, problems);
    detectThreads(proc, dump, problems);
    detectLocks(proc, dump, problems);
    detectChannels(proc, dump, problems);
    detectRoam(proc, dump, problems);
    detectShm(proc, dump, problems);
  }

  problems.sort((a, b) => {
    if (a.severity !== b.severity) {
      return a.severity === "danger" ? -1 : 1;
    }
    return b.timing - a.timing;
  });

  return problems;
}

function detectTasks(
  proc: string,
  dump: ProcessDump,
  out: Problem[]
): void {
  for (const task of dump.tasks) {
    if (task.state === "Polling" && task.poll_events.length > 0) {
      const last = task.poll_events[task.poll_events.length - 1];
      if (last.duration_secs != null && last.duration_secs > 1) {
        out.push({
          severity: "danger",
          category: "Tasks",
          process: proc,
          resource: task.name,
          description: `Polling for ${fmtDuration(last.duration_secs)}`,
          timing: last.duration_secs,
          timingLabel: fmtDuration(last.duration_secs),
          backtrace: last.backtrace ?? task.spawn_backtrace,
        });
      } else if (last.duration_secs != null && last.duration_secs > 0.1) {
        out.push({
          severity: "warn",
          category: "Tasks",
          process: proc,
          resource: task.name,
          description: `Polling for ${fmtDuration(last.duration_secs)}`,
          timing: last.duration_secs,
          timingLabel: fmtDuration(last.duration_secs),
          backtrace: last.backtrace ?? task.spawn_backtrace,
        });
      }
    } else if (
      task.state === "Pending" &&
      task.age_secs > 30 &&
      task.poll_events.length === 0
    ) {
      out.push({
        severity: "warn",
        category: "Tasks",
        process: proc,
        resource: task.name,
        description: `Pending for ${fmtAge(task.age_secs)}, never polled`,
        timing: task.age_secs,
        timingLabel: fmtAge(task.age_secs),
        backtrace: task.spawn_backtrace,
      });
    }
  }
}

function detectThreads(
  proc: string,
  dump: ProcessDump,
  out: Problem[]
): void {
  for (const thread of dump.threads) {
    const frame = thread.dominant_frame ?? firstUsefulFrame(thread.backtrace);
    if (isLikelyIdleFrameName(frame) || isLikelyIdleBacktrace(thread.backtrace)) continue;

    if (thread.same_location_count >= 10) {
      out.push({
        severity: "danger",
        category: "Threads",
        process: proc,
        resource: thread.name,
        description: `Stuck at same location for ${thread.same_location_count} samples`,
        timing: thread.same_location_count,
        timingLabel: `${thread.same_location_count} samples stuck`,
        backtrace: thread.backtrace,
      });
    } else if (thread.same_location_count >= 5) {
      out.push({
        severity: "warn",
        category: "Threads",
        process: proc,
        resource: thread.name,
        description: `Same location for ${thread.same_location_count} samples`,
        timing: thread.same_location_count,
        timingLabel: `${thread.same_location_count} samples stuck`,
        backtrace: thread.backtrace,
      });
    }
  }
}

export function detectRelationshipIssues(dumps: ProcessDump[]): RelationshipIssue[] {
  const byKey = new Map<string, RelationshipIssue>();

  const upsert = (issue: Omit<RelationshipIssue, "count">) => {
    const key = [
      issue.severity,
      issue.category,
      issue.process,
      issue.blocked,
      issue.waitsOn,
      issue.owner ?? "",
    ].join("|");
    const existing = byKey.get(key);
    if (existing) {
      existing.count += 1;
      if (issue.timing > existing.timing) {
        existing.timing = issue.timing;
        existing.timingLabel = issue.timingLabel;
      }
      if (!existing.backtrace && issue.backtrace) {
        existing.backtrace = issue.backtrace;
      }
    } else {
      byKey.set(key, { ...issue, count: 1 });
    }
  };

  for (const dump of dumps) {
    const proc = dump.process_name;

    if (dump.locks) {
      for (const lock of dump.locks.locks) {
        const holderNames = lock.holders
          .map((h) => h.task_name)
          .filter((v): v is string => !!v && v.trim().length > 0);
        const owner = holderNames.length > 0 ? holderNames[0] : null;

        for (const waiter of lock.waiters) {
          if (waiter.waiting_secs <= 1) continue;
          upsert({
            severity: waiter.waiting_secs > 5 ? "danger" : "warn",
            category: "Locks",
            process: proc,
            blocked: waiter.task_name ?? `${proc}:${lock.name}:waiter`,
            waitsOn: `lock:${lock.name}`,
            owner,
            description: `Lock waiter blocked for ${fmtDuration(waiter.waiting_secs)}`,
            timing: waiter.waiting_secs,
            timingLabel: fmtDuration(waiter.waiting_secs),
            backtrace: waiter.backtrace,
          });
        }
      }
    }

    if (dump.sync) {
      for (const ch of dump.sync.mpsc_channels) {
        if (ch.send_waiters <= 0) continue;
        upsert({
          severity: ch.send_waiters > 3 ? "danger" : "warn",
          category: "Channels",
          process: proc,
          blocked: `${ch.send_waiters} sender(s)`,
          waitsOn: `channel:${ch.name}`,
          owner: ch.creator_task_name ?? null,
          description: `Backpressure on channel (${ch.send_waiters} blocked sender(s))`,
          timing: ch.age_secs,
          timingLabel: fmtAge(ch.age_secs),
          backtrace: null,
        });
      }
    }

    if (dump.roam) {
      for (const conn of dump.roam.connections) {
        for (const req of conn.in_flight) {
          if (req.elapsed_secs <= 2) continue;
          const method = req.method_name ?? `method#${req.method_id}`;
          upsert({
            severity: req.elapsed_secs > 10 ? "danger" : "warn",
            category: "RPC",
            process: proc,
            blocked: `${conn.name} (${method})`,
            waitsOn: `rpc:${conn.peer_name ?? "unknown-peer"}`,
            owner: conn.peer_name ?? null,
            description: `RPC in-flight for ${fmtDuration(req.elapsed_secs)}`,
            timing: req.elapsed_secs,
            timingLabel: fmtDuration(req.elapsed_secs),
            backtrace: req.backtrace,
          });
        }
      }
    }
  }

  const out = [...byKey.values()];
  out.sort((a, b) => {
    if (a.severity !== b.severity) return a.severity === "danger" ? -1 : 1;
    if (b.count !== a.count) return b.count - a.count;
    return b.timing - a.timing;
  });
  return out;
}

export function summarizeRootCauses(issues: RelationshipIssue[]): RootCauseSummary[] {
  const byOwner = new Map<string, RootCauseSummary>();

  for (const issue of issues) {
    const owner = issue.owner ?? issue.waitsOn;
    const existing = byOwner.get(owner);
    if (existing) {
      existing.edgeCount += issue.count;
      existing.blockedCount += 1;
      if (issue.severity === "danger") existing.severity = "danger";
      if (issue.timing > existing.worstTiming) {
        existing.worstTiming = issue.timing;
        existing.worstTimingLabel = issue.timingLabel;
      }
    } else {
      byOwner.set(owner, {
        severity: issue.severity,
        owner,
        blockedCount: 1,
        edgeCount: issue.count,
        worstTiming: issue.timing,
        worstTimingLabel: issue.timingLabel,
      });
    }
  }

  const out = [...byOwner.values()];
  out.sort((a, b) => {
    if (a.severity !== b.severity) return a.severity === "danger" ? -1 : 1;
    if (b.blockedCount !== a.blockedCount) return b.blockedCount - a.blockedCount;
    if (b.edgeCount !== a.edgeCount) return b.edgeCount - a.edgeCount;
    return b.worstTiming - a.worstTiming;
  });
  return out;
}

function detectLocks(
  proc: string,
  dump: ProcessDump,
  out: Problem[]
): void {
  if (!dump.locks) return;

  for (const lock of dump.locks.locks) {
    for (const holder of lock.holders) {
      if (holder.held_secs > 5) {
        out.push({
          severity: "danger",
          category: "Locks",
          process: proc,
          resource: lock.name,
          description: `Held for ${fmtDuration(holder.held_secs)} (${holder.kind})`,
          timing: holder.held_secs,
          timingLabel: fmtDuration(holder.held_secs),
          backtrace: holder.backtrace,
        });
      } else if (holder.held_secs > 1) {
        out.push({
          severity: "warn",
          category: "Locks",
          process: proc,
          resource: lock.name,
          description: `Held for ${fmtDuration(holder.held_secs)} (${holder.kind})`,
          timing: holder.held_secs,
          timingLabel: fmtDuration(holder.held_secs),
          backtrace: holder.backtrace,
        });
      }
    }

    for (const waiter of lock.waiters) {
      if (waiter.waiting_secs > 5) {
        out.push({
          severity: "danger",
          category: "Locks",
          process: proc,
          resource: lock.name,
          description: `Waiting for ${fmtDuration(waiter.waiting_secs)} (${waiter.kind})`,
          timing: waiter.waiting_secs,
          timingLabel: fmtDuration(waiter.waiting_secs),
          backtrace: waiter.backtrace,
        });
      } else if (waiter.waiting_secs > 1) {
        out.push({
          severity: "warn",
          category: "Locks",
          process: proc,
          resource: lock.name,
          description: `Waiting for ${fmtDuration(waiter.waiting_secs)} (${waiter.kind})`,
          timing: waiter.waiting_secs,
          timingLabel: fmtDuration(waiter.waiting_secs),
          backtrace: waiter.backtrace,
        });
      }
    }
  }
}

function detectChannels(
  proc: string,
  dump: ProcessDump,
  out: Problem[]
): void {
  if (!dump.sync) return;

  for (const ch of dump.sync.mpsc_channels) {
    if (ch.send_waiters > 0 && ch.receiver_closed) {
      out.push({
        severity: "danger",
        category: "Channels",
        process: proc,
        resource: ch.name,
        description: `${ch.send_waiters} blocked sender(s), receiver closed`,
        timing: ch.age_secs,
        timingLabel: fmtAge(ch.age_secs),
        backtrace: null,
      });
    } else if (ch.sender_closed || ch.receiver_closed) {
      const side = ch.sender_closed ? "sender" : "receiver";
      out.push({
        severity: "danger",
        category: "Channels",
        process: proc,
        resource: ch.name,
        description: `Broken pipe: ${side} closed`,
        timing: ch.age_secs,
        timingLabel: fmtAge(ch.age_secs),
        backtrace: null,
      });
    } else if (ch.send_waiters > 0) {
      out.push({
        severity: "warn",
        category: "Channels",
        process: proc,
        resource: ch.name,
        description: `${ch.send_waiters} sender(s) blocked (backpressure)`,
        timing: ch.age_secs,
        timingLabel: fmtAge(ch.age_secs),
        backtrace: null,
      });
    }
  }

  for (const ch of dump.sync.oneshot_channels) {
    if (ch.state === "SenderDropped" || ch.state === "ReceiverDropped") {
      out.push({
        severity: "danger",
        category: "Channels",
        process: proc,
        resource: ch.name,
        description: `Oneshot: ${ch.state}`,
        timing: ch.age_secs,
        timingLabel: fmtAge(ch.age_secs),
        backtrace: null,
      });
    } else if (ch.state === "Pending" && ch.age_secs > 10) {
      out.push({
        severity: "warn",
        category: "Channels",
        process: proc,
        resource: ch.name,
        description: `Oneshot pending for ${fmtAge(ch.age_secs)}`,
        timing: ch.age_secs,
        timingLabel: fmtAge(ch.age_secs),
        backtrace: null,
      });
    }
  }

  for (const cell of dump.sync.once_cells) {
    if (cell.state === "Initializing" && cell.age_secs > 5) {
      out.push({
        severity: "warn",
        category: "Channels",
        process: proc,
        resource: cell.name,
        description: `OnceCell initializing for ${fmtAge(cell.age_secs)}`,
        timing: cell.age_secs,
        timingLabel: fmtAge(cell.age_secs),
        backtrace: null,
      });
    }
  }
}

function detectRoam(
  proc: string,
  dump: ProcessDump,
  out: Problem[]
): void {
  if (!dump.roam) return;

  for (const conn of dump.roam.connections) {
    for (const req of conn.in_flight) {
      const name = req.method_name ?? `method#${req.method_id}`;
      if (req.elapsed_secs > 10) {
        out.push({
          severity: "danger",
          category: "RPC",
          process: proc,
          resource: `${conn.name}: ${name}`,
          description: `In-flight for ${fmtDuration(req.elapsed_secs)}`,
          timing: req.elapsed_secs,
          timingLabel: fmtDuration(req.elapsed_secs),
          backtrace: req.backtrace,
        });
      } else if (req.elapsed_secs > 2) {
        out.push({
          severity: "warn",
          category: "RPC",
          process: proc,
          resource: `${conn.name}: ${name}`,
          description: `In-flight for ${fmtDuration(req.elapsed_secs)}`,
          timing: req.elapsed_secs,
          timingLabel: fmtDuration(req.elapsed_secs),
          backtrace: req.backtrace,
        });
      }
    }
  }
}

function detectShm(
  proc: string,
  dump: ProcessDump,
  out: Problem[]
): void {
  if (!dump.shm) return;

  for (const seg of dump.shm.segments) {
    for (const peer of seg.peers) {
      if (peer.state !== "Attached") continue;
      const name = peer.name ?? `peer#${peer.peer_id}`;
      if (
        peer.time_since_heartbeat_ms != null &&
        peer.time_since_heartbeat_ms > 5000
      ) {
        out.push({
          severity: "danger",
          category: "SHM",
          process: proc,
          resource: name,
          description: `No heartbeat for ${(peer.time_since_heartbeat_ms / 1000).toFixed(1)}s`,
          timing: peer.time_since_heartbeat_ms / 1000,
          timingLabel: fmtDuration(peer.time_since_heartbeat_ms / 1000),
          backtrace: null,
        });
      } else if (
        peer.time_since_heartbeat_ms != null &&
        peer.time_since_heartbeat_ms > 2000
      ) {
        out.push({
          severity: "warn",
          category: "SHM",
          process: proc,
          resource: name,
          description: `No heartbeat for ${(peer.time_since_heartbeat_ms / 1000).toFixed(1)}s`,
          timing: peer.time_since_heartbeat_ms / 1000,
          timingLabel: fmtDuration(peer.time_since_heartbeat_ms / 1000),
          backtrace: null,
        });
      }
    }
  }

  for (const q of dump.shm.channels) {
    if (q.len === q.capacity && q.capacity > 0) {
      out.push({
        severity: "warn",
        category: "SHM",
        process: proc,
        resource: q.name,
        description: `Queue full (${q.len}/${q.capacity})`,
        timing: q.capacity,
        timingLabel: `${q.len}/${q.capacity}`,
        backtrace: null,
      });
    }
  }
}
