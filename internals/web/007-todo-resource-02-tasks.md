# Resource Track: Tasks

Status: todo
Owner: wg-resource-tasks
Priority: P0

## Mission

Emit task lifecycle as canonical graph nodes and explicit dependencies only.

## Prerequisites

- Complete `/Users/amos/bearcove/peeps/internals/web/000-todo-crate-split-for-parallelization.md`.
- Use contracts from `/Users/amos/bearcove/peeps/internals/web/006-todo-wrapper-emission-api.md`.

## Current context

- Task tracking lives in `/Users/amos/bearcove/peeps/crates/peeps-tasks/src/tasks.rs` and `/Users/amos/bearcove/peeps/crates/peeps-tasks/src/snapshot.rs`.
- Existing snapshots include task records and wake/spawn metadata.
- `spawn_tracked` exists, but not all consumer callsites necessarily use it.

## Node + edge model

Node ID:
- `task:{proc_key}:{task_id}`

Node kind:
- `task`

Required attrs_json:
- `task_id`
- `name`
- `state` (`pending|polling|completed`)
- `spawned_at_ns`

Optional attrs_json:
- `parent_task_id`
- `spawn_backtrace`
- `last_wake_from_task_id`

Required `needs` edges:
- `task -> future` when task progress depends on that future (from explicit scheduler instrumentation)

Optional `needs` edges (only if explicitly measured as dependency):
- `task -> task` for explicit wake/join dependency

Do not emit:
- synthetic parent/wake edges from guessed relationships

## Implementation steps

1. Emit canonical task nodes in `peeps-tasks` graph builder.
2. Emit `task -> future` only from explicit records.
3. Emit `task -> task` only when a true dependency event is explicitly captured.
4. Keep spawn lineage in attrs when it is causal history but not a direct dependency.

## Consumer changes

Required where missing instrumentation:
- Replace `tokio::spawn(...)` with `peeps_tasks::spawn_tracked(...)` in critical paths.
- Start with Roam + Vixen entry points where deadlocks are investigated.

## Validation SQL

```sql
SELECT COUNT(*)
FROM nodes
WHERE snapshot_id = ?1
  AND kind = 'task';
```

```sql
SELECT COUNT(*)
FROM edges
WHERE snapshot_id = ?1
  AND kind = 'needs'
  AND src_id LIKE 'task:%'
  AND dst_id LIKE 'future:%';
```
