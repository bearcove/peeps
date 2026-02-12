import type { ProcessDump } from "./types";
import { fmtDuration, fmtAge } from "./util";

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
