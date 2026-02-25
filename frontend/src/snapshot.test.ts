import { describe, expect, it } from "vitest";
import type { SnapshotCutResponse } from "./api/types.generated";
import type { EdgeDef } from "./snapshot";
import { buildBacktraceIndex, collapseEdgesThroughHiddenNodes, convertSnapshot } from "./snapshot";

function edge(id: string, source: string, target: string): EdgeDef {
  return {
    id,
    source,
    target,
    kind: "waiting_on",
  };
}

describe("collapseEdgesThroughHiddenNodes", () => {
  it("keeps visible direct edges", () => {
    const edges = [edge("ab", "a", "b"), edge("bc", "b", "c")];
    const visible = new Set(["a", "b"]);

    const collapsed = collapseEdgesThroughHiddenNodes(edges, visible);

    expect(collapsed.map((e) => e.id)).toEqual(["ab"]);
  });

  it("synthesizes across hidden nodes in directed paths", () => {
    const edges = [edge("ah", "a", "h"), edge("hb", "h", "b")];
    const visible = new Set(["a", "b"]);

    const collapsed = collapseEdgesThroughHiddenNodes(edges, visible);

    expect(collapsed).toContainEqual(
      expect.objectContaining({
        id: "collapsed-a-b",
        source: "a",
        target: "b",
        kind: "polls",
      }),
    );
  });

  it("synthesizes when the hidden intermediary only has incoming edges from visible nodes", () => {
    const edges = [edge("ha", "h", "a"), edge("hb", "h", "b")];
    const visible = new Set(["a", "b"]);

    const collapsed = collapseEdgesThroughHiddenNodes(edges, visible);

    expect(collapsed).toContainEqual(
      expect.objectContaining({
        id: "collapsed-a-b",
        source: "a",
        target: "b",
        kind: "polls",
      }),
    );
  });
});

describe("buildBacktraceIndex", () => {
  // f[verify display.backtrace.catalog]
  it("resolves frame_ids through catalog and rejects missing frame references", () => {
    const snapshot: SnapshotCutResponse = {
      snapshot_id: 1,
      captured_at_unix_ms: 0,
      processes: [],
      timed_out_processes: [],
      frames: [
        {
          frame_id: 11,
          frame: {
            unresolved: { module_path: "/bin/app", rel_pc: 16, reason: "symbolication pending" },
          },
        },
      ],
      backtraces: [{ backtrace_id: 101, frame_ids: [11] }],
    };

    const index = buildBacktraceIndex(snapshot);
    expect(index.get(101)?.frame_ids).toEqual([11]);
    expect(index.get(101)?.frames).toHaveLength(1);

    const broken: SnapshotCutResponse = {
      ...snapshot,
      backtraces: [{ backtrace_id: 102, frame_ids: [99] }],
    };
    expect(() => buildBacktraceIndex(broken)).toThrow("references missing frame id 99");
  });
});

function semaphoreSnapshot({
  maxPermits,
  handedOutPermits,
  withWaiter,
}: {
  maxPermits: number;
  handedOutPermits: number;
  withWaiter: boolean;
}): SnapshotCutResponse {
  return {
    snapshot_id: 1,
    captured_at_unix_ms: 1_700_000_000_000,
    timed_out_processes: [],
    frames: [
      {
        frame_id: 11,
        frame: {
          unresolved: {
            module_path: "/bin/demo",
            rel_pc: 16,
            reason: "symbolication pending",
          },
        },
      },
    ],
    backtraces: [{ backtrace_id: 101, frame_ids: [11] }],
    processes: [
      {
        process_id: "p1",
        process_name: "demo",
        pid: 42,
        ptime_now_ms: 100,
        snapshot: {
          entities: [
            {
              id: "sem1",
              birth: 5,
              backtrace: 101,
              name: "demo.api_gate",
              body: {
                semaphore: {
                  max_permits: maxPermits,
                  handed_out_permits: handedOutPermits,
                },
              },
            },
            {
              id: "waiter1",
              birth: 10,
              backtrace: 101,
              name: "run::waiter",
              body: { future: {} },
            },
          ],
          scopes: [],
          edges: withWaiter
            ? [{ src: "waiter1", dst: "sem1", backtrace: 101, kind: "waiting_on" }]
            : [],
          events: [],
        },
        scope_entity_links: [],
      },
    ],
  };
}

describe("convertSnapshot semaphore tone", () => {
  it("marks exhausted semaphores as crit only when there are waiters", () => {
    const { entities } = convertSnapshot(
      semaphoreSnapshot({ maxPermits: 1, handedOutPermits: 1, withWaiter: true }),
    );
    const semaphore = entities.find((entity) => entity.id === "sem1");

    expect(semaphore?.stat).toBe("1/1");
    expect(semaphore?.status.label).toBe("1/1 permits");
    expect(semaphore?.status.tone).toBe("crit");
    expect(semaphore?.statTone).toBe("crit");
  });

  it("keeps exhausted semaphores as warn when nothing is waiting", () => {
    const { entities } = convertSnapshot(
      semaphoreSnapshot({ maxPermits: 1, handedOutPermits: 1, withWaiter: false }),
    );
    const semaphore = entities.find((entity) => entity.id === "sem1");

    expect(semaphore?.stat).toBe("1/1");
    expect(semaphore?.status.tone).toBe("warn");
    expect(semaphore?.statTone).toBe("warn");
  });
});

describe("convertSnapshot task scope selection", () => {
  it("prefers non-main task scopes and otherwise uses the most recent scope", () => {
    const snapshot: SnapshotCutResponse = {
      snapshot_id: 1,
      captured_at_unix_ms: 1_700_000_000_000,
      timed_out_processes: [],
      frames: [
        {
          frame_id: 11,
          frame: {
            unresolved: { module_path: "/bin/demo", rel_pc: 16, reason: "symbolication pending" },
          },
        },
      ],
      backtraces: [{ backtrace_id: 101, frame_ids: [11] }],
      processes: [
        {
          process_id: "p1",
          process_name: "demo",
          pid: 42,
          ptime_now_ms: 100,
          snapshot: {
            entities: [
              {
                id: "ent1",
                birth: 5,
                backtrace: 101,
                name: "demo.api_gate",
                body: { semaphore: { max_permits: 1, handed_out_permits: 0 } },
              },
            ],
            scopes: [
              {
                id: "task-main",
                birth: 30,
                backtrace: 101,
                name: "task.main",
                body: { task: { task_key: "main" } },
              },
              {
                id: "task-new",
                birth: 20,
                backtrace: 101,
                name: "task.spawn#new",
                body: { task: { task_key: "new" } },
              },
              {
                id: "task-old",
                birth: 10,
                backtrace: 101,
                name: "task.spawn#old",
                body: { task: { task_key: "old" } },
              },
            ],
            edges: [],
            events: [],
          },
          scope_entity_links: [
            { entity_id: "ent1", scope_id: "task-main" },
            { entity_id: "ent1", scope_id: "task-new" },
            { entity_id: "ent1", scope_id: "task-old" },
          ],
        },
      ],
    };

    const { entities } = convertSnapshot(snapshot);
    const entity = entities.find((item) => item.id === "ent1");

    expect(entity?.taskScopeId).toBe("task-new");
    expect(entity?.taskScopeName).toBe("task.spawn#new");
    expect(entity?.taskScopeKey).toBe("p1:task-new");
  });
});
