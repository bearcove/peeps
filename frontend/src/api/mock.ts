import type { ApiClient } from "./client";
import type {
  ConnectionsResponse,
  CutStatusResponse,
  SnapshotResponse,
  TriggerCutResponse,
} from "./types";

const sampleConnections: ConnectionsResponse = {
  connected_processes: 2,
  processes: [
    { conn_id: 101, process_name: "lab-server", pid: 4242 },
    { conn_id: 202, process_name: "lab-loader", pid: 1313 },
  ],
};

const MOCK_SNAPSHOT: SnapshotResponse = {
  entities: [
    {
      id: "req_sleepy", birth_ms: 1245000, source: "src/rpc/demo.rs:42",
      name: "DemoRpc.sleepy_forever",
      body: { request: { method: "DemoRpc.sleepy_forever", args_preview: "(no args)" } },
      meta: { level: "info", rpc_service: "DemoRpc", transport: "roam-tcp" },
    },
    {
      id: "resp_sleepy", birth_ms: 1244800, source: "src/rpc/demo.rs:45",
      name: "DemoRpc.sleepy_forever",
      body: { response: { method: "DemoRpc.sleepy_forever", status: "error" } },
      meta: { level: "info", status_detail: "deadline exceeded" },
    },
    {
      id: "req_ping", birth_ms: 820000, source: "src/rpc/demo.rs:18",
      name: "DemoRpc.ping",
      body: { request: { method: "DemoRpc.ping", args_preview: "{ ttl: 30 }" } },
      meta: { level: "info", rpc_service: "DemoRpc", transport: "roam-tcp" },
    },
    {
      id: "resp_ping", birth_ms: 819500, source: "src/rpc/demo.rs:20",
      name: "DemoRpc.ping",
      body: { response: { method: "DemoRpc.ping", status: "ok" } },
      meta: { level: "info" },
    },
    {
      id: "lock_state", birth_ms: 3600000, source: "src/state.rs:12",
      name: "Mutex<GlobalState>",
      body: { lock: { kind: "mutex" } },
      meta: { level: "debug" },
    },
    {
      id: "ch_tx", birth_ms: 3590000, source: "src/dispatch.rs:67",
      name: "mpsc.send",
      body: { channel_tx: { lifecycle: "open", details: { mpsc: { buffer: { occupancy: 0, capacity: 128 } } } } },
      meta: { level: "debug" },
    },
    {
      id: "ch_rx", birth_ms: 3590000, source: "src/dispatch.rs:68",
      name: "mpsc.recv",
      body: { channel_rx: { lifecycle: "open", details: { mpsc: { buffer: { occupancy: 0, capacity: 128 } } } } },
      meta: { level: "debug" },
    },
    {
      id: "future_store", birth_ms: 2100000, source: "src/store.rs:104",
      name: "store.incoming.recv",
      body: "future",
      meta: { level: "trace", poll_count: 847 },
    },
    {
      id: "sem_conns", birth_ms: 3580000, source: "src/server/limits.rs:28",
      name: "conn.rate_limit",
      body: { semaphore: { max_permits: 5, handed_out_permits: 2 } },
      meta: { level: "debug", scope: "rate_limiter" },
    },
    {
      id: "notify_shutdown", birth_ms: 3600000, source: "src/lifecycle.rs:15",
      name: "shutdown.signal",
      body: { notify: { waiter_count: 2 } },
      meta: { level: "info" },
    },
    {
      id: "oncecell_config", birth_ms: 1800000, source: "src/config.rs:8",
      name: "AppConfig",
      body: { once_cell: { waiter_count: 1, state: "initializing" } },
      meta: { level: "info", config_path: "/etc/app/config.toml" },
    },
    {
      id: "cmd_migrate", birth_ms: 45000, source: "src/bootstrap.rs:55",
      name: "db-migrate",
      body: { command: { program: "db-migrate", args: ["--up", "--env=staging"], env: [] } },
      meta: { level: "info" },
    },
    {
      id: "file_config", birth_ms: 1799500, source: "src/config.rs:22",
      name: "config.toml",
      body: { file_op: { op: "read", path: "/etc/app/config.toml" } },
      meta: { level: "debug", bytes: 4096 },
    },
    {
      id: "net_peer", birth_ms: 920000, source: "src/net/peer.rs:31",
      name: "peer:10.0.0.5:8080",
      body: { net_connect: { addr: "10.0.0.5:8080" } },
      meta: { level: "info", tls: true },
    },
  ],
  edges: [
    { src_id: "resp_sleepy", dst_id: "lock_state", kind: "needs" },
    { src_id: "lock_state", dst_id: "ch_rx", kind: "needs" },
    { src_id: "ch_rx", dst_id: "resp_sleepy", kind: "needs" },
    { src_id: "ch_tx", dst_id: "ch_rx", kind: "channel_link" },
    { src_id: "req_sleepy", dst_id: "resp_sleepy", kind: "rpc_link" },
    { src_id: "req_ping", dst_id: "resp_ping", kind: "rpc_link" },
    { src_id: "req_ping", dst_id: "lock_state", kind: "polls" },
    { src_id: "future_store", dst_id: "ch_rx", kind: "polls" },
    { src_id: "oncecell_config", dst_id: "file_config", kind: "needs" },
    { src_id: "cmd_migrate", dst_id: "oncecell_config", kind: "polls" },
    { src_id: "net_peer", dst_id: "sem_conns", kind: "polls" },
    { src_id: "notify_shutdown", dst_id: "ch_tx", kind: "closed_by" },
  ],
};

const retryDelay = 120;

function delay<T>(payload: T, ms = retryDelay): Promise<T> {
  return new Promise((resolve) => {
    window.setTimeout(() => resolve(payload), ms);
  });
}

function buildPendingIds(count: number): number[] {
  return sampleConnections.processes.slice(0, count).map((proc) => proc.conn_id);
}

export function createMockApiClient(): ApiClient {
  let nextCutId = 1;
  let activeCut: CutStatusResponse | null = null;
  return {
    fetchConnections: () => delay(sampleConnections),
    triggerCut: () => {
      const cut: CutStatusResponse = {
        cut_id: `lab-mock-${String(nextCutId).padStart(3, "0")}`,
        requested_at_ns: Date.now() * 1_000_000,
        pending_connections: sampleConnections.processes.length,
        acked_connections: 0,
        pending_conn_ids: buildPendingIds(sampleConnections.processes.length),
      };
      nextCutId += 1;
      activeCut = cut;
      const trigger: TriggerCutResponse = {
        cut_id: cut.cut_id,
        requested_at_ns: cut.requested_at_ns,
        requested_connections: sampleConnections.processes.length,
      };
      return delay(trigger);
    },
    fetchCutStatus: (cutId: string) => {
      if (!activeCut || activeCut.cut_id !== cutId) {
        return delay({
          cut_id: cutId,
          requested_at_ns: Date.now() * 1_000_000,
          pending_connections: 0,
          acked_connections: 0,
          pending_conn_ids: [],
        });
      }
      const pending = Math.max(activeCut.pending_connections - 1, 0);
      const acked = activeCut.acked_connections + (activeCut.pending_connections > 0 ? 1 : 0);
      activeCut = {
        ...activeCut,
        pending_connections: pending,
        acked_connections: acked,
        pending_conn_ids: buildPendingIds(pending),
      };
      return delay(activeCut);
    },
    fetchSnapshot: () => delay(MOCK_SNAPSHOT, 300),
  };
}
