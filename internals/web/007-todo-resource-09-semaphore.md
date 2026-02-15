# Resource Track: Semaphore

Status: todo
Owner: wg-resource-semaphore
Priority: P1

## Mission

Make semaphore contention explicit in node state + `needs` dependencies.

## Prerequisites

- Complete `/Users/amos/bearcove/peeps/internals/web/000-todo-crate-split-for-parallelization.md`.
- Use contracts from `/Users/amos/bearcove/peeps/internals/web/006-todo-wrapper-emission-api.md`.

## Current context

- Wrapper is `/Users/amos/bearcove/peeps/crates/peeps-sync/src/semaphore.rs` and `/Users/amos/bearcove/peeps/crates/peeps-sync/src/snapshot.rs`.
- Current snapshot has waiters/acquires/avg/max but edge-level task waits must be explicit.

## Node + edge model

Node ID:
- `semaphore:{proc_key}:{name}`

Node kind:
- `semaphore`

Required attrs_json:
- `name`
- `permits_total`
- `permits_available`
- `waiters`
- `acquires`
- `oldest_wait_ns`
- `high_waiters_watermark`
- `creator_task_id`

Required `needs` edges:
- `task -> semaphore` when task progress depends on permit availability

## Implementation steps

1. Instrument all acquire paths (borrowed + owned + try variants).
2. Emit `task -> semaphore` `needs` edges from explicitly measured dependency paths.
3. Keep try-acquire failures as explicit attrs/counters, not fake wait edges.
4. Track watermark metrics in wrapper state.

## Consumer changes

- Transparent where `DiagnosticSemaphore` is used.
- Migrate raw `tokio::sync::Semaphore` where present.

## Validation SQL

```sql
SELECT COUNT(*)
FROM edges
WHERE snapshot_id = ?1
  AND kind = 'needs'
  AND src_id LIKE 'task:%'
  AND dst_id LIKE 'semaphore:%';
```
