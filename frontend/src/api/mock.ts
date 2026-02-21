import type { ApiClient } from "./client";
import type {
  ConnectionsResponse,
  CutStatusResponse,
  FrameSummary,
  RecordCurrentResponse,
  RecordingSessionInfo,
  SqlResponse,
  SnapshotCutResponse,
  TriggerCutResponse,
} from "./types.generated";

const sampleConnections: ConnectionsResponse = {
  connected_processes: 2,
  processes: [
    {
      conn_id: 101,
      process_id: "PROC#101",
      process_name: "lab-server",
      pid: 4242,
    },
    {
      conn_id: 202,
      process_id: "PROC#202",
      process_name: "lab-loader",
      pid: 1313,
    },
  ],
};

// Process 1: "lab-server" — 1 hour old, has the deadlock cycle
const LAB_SERVER_PTIME_NOW = 3_600_000; // 1h in ms
const LAB_SERVER_CAPTURED_AT = 1_739_794_800_000; // fixed unix ms anchor

// Process 2: "lab-loader" — 30 minutes old, I/O and sync primitives
const LAB_LOADER_PTIME_NOW = 1_800_000; // 30min in ms
const _LAB_LOADER_CAPTURED_AT = 1_739_794_800_000;

// Helper: convert intended age-in-ms to a PTime birth value
function birth(ptimeNow: number, ageMs: number): number {
  return Math.max(0, ptimeNow - ageMs);
}

const MOCK_SNAPSHOT: SnapshotCutResponse = {
  snapshot_id: 1,
  captured_at_unix_ms: LAB_SERVER_CAPTURED_AT,
  timed_out_processes: [],
  backtraces: [
    { backtrace_id: 101001, frame_ids: [1] },
    { backtrace_id: 101002, frame_ids: [2] },
    { backtrace_id: 101003, frame_ids: [3] },
    { backtrace_id: 101004, frame_ids: [4] },
    { backtrace_id: 101005, frame_ids: [5] },
    { backtrace_id: 101006, frame_ids: [6] },
    { backtrace_id: 101007, frame_ids: [7] },
    { backtrace_id: 101008, frame_ids: [8] },
    { backtrace_id: 202001, frame_ids: [9] },
    { backtrace_id: 202002, frame_ids: [10] },
    { backtrace_id: 202003, frame_ids: [11] },
    { backtrace_id: 202004, frame_ids: [12] },
    { backtrace_id: 202005, frame_ids: [13] },
    { backtrace_id: 202006, frame_ids: [14] },
  ],
  frames: [
    { frame_id: 1, frame: { resolved: { module_path: "lab_server::rpc::demo", function_name: "handle_sleepy_forever", source_file: "/workspace/lab-server/src/rpc/demo.rs", line: 42 } } },
    { frame_id: 2, frame: { resolved: { module_path: "lab_server::rpc::demo", function_name: "finish_sleepy_forever", source_file: "/workspace/lab-server/src/rpc/demo.rs", line: 45 } } },
    { frame_id: 3, frame: { resolved: { module_path: "lab_server::rpc::demo", function_name: "handle_ping", source_file: "/workspace/lab-server/src/rpc/demo.rs", line: 18 } } },
    { frame_id: 4, frame: { resolved: { module_path: "lab_server::rpc::demo", function_name: "finish_ping", source_file: "/workspace/lab-server/src/rpc/demo.rs", line: 20 } } },
    { frame_id: 5, frame: { resolved: { module_path: "lab_server::state", function_name: "lock_state", source_file: "/workspace/lab-server/src/state.rs", line: 12 } } },
    { frame_id: 6, frame: { resolved: { module_path: "lab_server::dispatch", function_name: "send_dispatch", source_file: "/workspace/lab-server/src/dispatch.rs", line: 67 } } },
    { frame_id: 7, frame: { resolved: { module_path: "lab_server::dispatch", function_name: "recv_dispatch", source_file: "/workspace/lab-server/src/dispatch.rs", line: 68 } } },
    { frame_id: 8, frame: { resolved: { module_path: "lab_server::store", function_name: "poll_store_incoming", source_file: "/workspace/lab-server/src/store.rs", line: 104 } } },
    { frame_id: 9, frame: { resolved: { module_path: "lab_loader::server::limits", function_name: "acquire_connection_permit", source_file: "/workspace/lab-loader/src/server/limits.rs", line: 28 } } },
    { frame_id: 10, frame: { resolved: { module_path: "lab_loader::lifecycle", function_name: "wait_for_shutdown", source_file: "/workspace/lab-loader/src/lifecycle.rs", line: 15 } } },
    { frame_id: 11, frame: { resolved: { module_path: "lab_loader::config", function_name: "init_config_cell", source_file: "/workspace/lab-loader/src/config.rs", line: 8 } } },
    { frame_id: 12, frame: { resolved: { module_path: "lab_loader::bootstrap", function_name: "spawn_migration", source_file: "/workspace/lab-loader/src/bootstrap.rs", line: 55 } } },
    { frame_id: 13, frame: { resolved: { module_path: "lab_loader::config", function_name: "read_config_file", source_file: "/workspace/lab-loader/src/config.rs", line: 22 } } },
    { frame_id: 14, frame: { resolved: { module_path: "lab_loader::net::peer", function_name: "connect_peer", source_file: "/workspace/lab-loader/src/net/peer.rs", line: 31 } } },
  ],
  processes: [
    {
      process_id: "PROC#101",
      process_name: "lab-server",
      pid: 4242,
      ptime_now_ms: LAB_SERVER_PTIME_NOW,
      scope_entity_links: [],
      snapshot: {
        scopes: [],
        events: [],
        entities: [
          {
            id: "req_sleepy",
            birth: birth(LAB_SERVER_PTIME_NOW, 1_245_000),
            backtrace: 101001,
            name: "DemoRpc.sleepy_forever",
            body: { request: { service_name: "DemoRpc", method_name: "sleepy_forever", args_json: "(no args)" } },
          },
          {
            id: "resp_sleepy",
            birth: birth(LAB_SERVER_PTIME_NOW, 1_244_800),
            backtrace: 101002,
            name: "DemoRpc.sleepy_forever",
            body: { response: { service_name: "DemoRpc", method_name: "sleepy_forever", status: { error: { internal: "deadline exceeded" } } } },
          },
          {
            id: "req_ping",
            birth: birth(LAB_SERVER_PTIME_NOW, 820_000),
            backtrace: 101003,
            name: "DemoRpc.ping",
            body: { request: { service_name: "DemoRpc", method_name: "ping", args_json: "{ ttl: 30 }" } },
          },
          {
            id: "resp_ping",
            birth: birth(LAB_SERVER_PTIME_NOW, 819_500),
            backtrace: 101004,
            name: "DemoRpc.ping",
            body: { response: { service_name: "DemoRpc", method_name: "ping", status: { ok: "{}" } } },
          },
          {
            id: "lock_state",
            birth: birth(LAB_SERVER_PTIME_NOW, 3_600_000),
            backtrace: 101005,
            name: "Mutex<GlobalState>",
            body: { lock: { kind: "mutex" } },
          },
          {
            id: "ch_tx",
            birth: birth(LAB_SERVER_PTIME_NOW, 3_590_000),
            backtrace: 101006,
            name: "mpsc.send",
            body: { mpsc_tx: { queue_len: 0, capacity: 128 } },
          },
          {
            id: "ch_rx",
            birth: birth(LAB_SERVER_PTIME_NOW, 3_590_000),
            backtrace: 101007,
            name: "mpsc.recv",
            body: { mpsc_rx: {} },
          },
          {
            id: "future_store",
            birth: birth(LAB_SERVER_PTIME_NOW, 2_100_000),
            backtrace: 101008,
            name: "store.incoming.recv",
            body: { future: {} },
          },
        ],
        edges: [
          { src: "resp_sleepy", dst: "lock_state", backtrace: 101002, kind: "waiting_on" },
          { src: "lock_state", dst: "ch_rx", backtrace: 101005, kind: "waiting_on" },
          { src: "ch_rx", dst: "resp_sleepy", backtrace: 101007, kind: "waiting_on" },
          { src: "ch_tx", dst: "ch_rx", backtrace: 101006, kind: "paired_with" },
          { src: "req_sleepy", dst: "resp_sleepy", backtrace: 101001, kind: "paired_with" },
          { src: "req_ping", dst: "resp_ping", backtrace: 101003, kind: "paired_with" },
          { src: "req_ping", dst: "lock_state", backtrace: 101003, kind: "polls" },
          { src: "future_store", dst: "ch_rx", backtrace: 101008, kind: "polls" },
        ],
      },
    },
    {
      process_id: "PROC#202",
      process_name: "lab-loader",
      pid: 1313,
      ptime_now_ms: LAB_LOADER_PTIME_NOW,
      scope_entity_links: [],
      snapshot: {
        scopes: [],
        events: [],
        entities: [
          {
            id: "sem_conns",
            birth: birth(LAB_LOADER_PTIME_NOW, 1_780_000),
            backtrace: 202001,
            name: "conn.rate_limit",
            body: { semaphore: { max_permits: 5, handed_out_permits: 2 } },
          },
          {
            id: "notify_shutdown",
            birth: birth(LAB_LOADER_PTIME_NOW, 1_800_000),
            backtrace: 202002,
            name: "shutdown.signal",
            body: { notify: { waiter_count: 2 } },
          },
          {
            id: "oncecell_config",
            birth: birth(LAB_LOADER_PTIME_NOW, 1_200_000),
            backtrace: 202003,
            name: "AppConfig",
            body: { once_cell: { waiter_count: 1, state: "initializing" } },
          },
          {
            id: "cmd_migrate",
            birth: birth(LAB_LOADER_PTIME_NOW, 45_000),
            backtrace: 202004,
            name: "db-migrate",
            body: { command: { program: "db-migrate", args: ["--up", "--env=staging"], env: [] } },
          },
          {
            id: "file_config",
            birth: birth(LAB_LOADER_PTIME_NOW, 1_199_500),
            backtrace: 202005,
            name: "config.toml",
            body: { file_op: { op: "read", path: "/etc/app/config.toml" } },
          },
          {
            id: "net_peer",
            birth: birth(LAB_LOADER_PTIME_NOW, 920_000),
            backtrace: 202006,
            name: "peer:10.0.0.5:8080",
            body: { net_connect: { addr: "10.0.0.5:8080" } },
          },
        ],
        edges: [
          { src: "oncecell_config", dst: "file_config", backtrace: 202003, kind: "waiting_on" },
          { src: "cmd_migrate", dst: "oncecell_config", backtrace: 202004, kind: "polls" },
          { src: "net_peer", dst: "sem_conns", backtrace: 202006, kind: "polls" },
          { src: "sem_conns", dst: "notify_shutdown", backtrace: 202001, kind: "polls" },
        ],
      },
    },
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
    fetchSql: (_sql: string) => {
      const response: SqlResponse = {
        columns: [
          "process_id",
          "process_name",
          "pid",
          "stream_id",
          "scope_id",
          "scope_name",
          "scope_kind",
          "member_count",
          "scope_json",
        ],
        rows: [
          [
            101,
            "lab-server",
            4242,
            "runtime",
            "scope_conn_1",
            "incoming_connections",
            "connection",
            4,
            "{\"id\":\"scope_conn_1\",\"name\":\"incoming_connections\",\"body\":\"connection\",\"meta\":{\"in_flight\":4,\"capacity\":64}}",
          ],
          [
            101,
            "lab-server",
            4242,
            "runtime",
            "scope_tasks_1",
            "rpc handlers",
            "task",
            3,
            "{\"id\":\"scope_tasks_1\",\"name\":\"rpc handlers\",\"body\":\"task\",\"meta\":{\"executor\":\"tokio\"}}",
          ],
          [
            202,
            "lab-loader",
            1313,
            "runtime",
            "scope_thread_1",
            "tokio worker",
            "thread",
            2,
            "{\"id\":\"scope_thread_1\",\"name\":\"tokio worker\",\"body\":\"thread\",\"meta\":{\"worker\":1}}",
          ],
        ],
        row_count: 3,
      };
      return delay(response);
    },
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
    fetchExistingSnapshot: () => delay(MOCK_SNAPSHOT, 150),
    fetchSnapshot: () => delay(MOCK_SNAPSHOT, 300),
    streamSnapshotSymbolication: (_snapshotId, onUpdate) => {
      const timer = window.setTimeout(() => {
        onUpdate({
          snapshot_id: MOCK_SNAPSHOT.snapshot_id,
          total_frames: MOCK_SNAPSHOT.frames.length,
          completed_frames: MOCK_SNAPSHOT.frames.length,
          done: true,
          updated_frames: [],
        });
      }, 300);
      return () => window.clearTimeout(timer);
    },
    startRecording: (req) => {
      const session: RecordingSessionInfo = {
        session_id: "mock-session-001",
        status: "recording",
        interval_ms: req?.interval_ms ?? 1000,
        started_at_unix_ms: Date.now(),
        stopped_at_unix_ms: undefined,
        frame_count: 0,
        max_frames: req?.max_frames ?? 100,
        max_memory_bytes: req?.max_memory_bytes ?? 256 * 1024 * 1024,
        overflowed: false,
        approx_memory_bytes: 0,
        avg_capture_ms: 0,
        max_capture_ms: 0,
        total_capture_ms: 0,
        frames: [],
      };
      return delay(session);
    },
    stopRecording: () => {
      const now = Date.now();
      const frames: FrameSummary[] = [
        { frame_index: 0, captured_at_unix_ms: now - 3000, process_count: 2, capture_duration_ms: 12 },
        { frame_index: 1, captured_at_unix_ms: now - 2000, process_count: 2, capture_duration_ms: 10 },
        { frame_index: 2, captured_at_unix_ms: now - 1000, process_count: 2, capture_duration_ms: 11 },
      ];
      const session: RecordingSessionInfo = {
        session_id: "mock-session-001",
        status: "stopped",
        interval_ms: 1000,
        started_at_unix_ms: now - 3000,
        stopped_at_unix_ms: now,
        frame_count: frames.length,
        max_frames: 100,
        max_memory_bytes: 256 * 1024 * 1024,
        overflowed: false,
        approx_memory_bytes: 0,
        avg_capture_ms: 11,
        max_capture_ms: 12,
        total_capture_ms: 33,
        frames,
      };
      return delay(session);
    },
    fetchRecordingCurrent: () => {
      const response: RecordCurrentResponse = {};
      return delay(response);
    },
    fetchRecordingFrame: (_frameIndex) => delay(MOCK_SNAPSHOT, 300),
    exportRecording: () => Promise.resolve(new Blob(["{}"], { type: "application/json" })),
    importRecording: () => Promise.reject(new Error("import not supported in mock")),
  };
}
