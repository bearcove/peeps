# Resource Track: Futures

Status: todo
Owner: wg-resource-futures
Priority: P0

## Mission

Represent instrumented futures as first-class nodes with explicit `needs` dependencies.

## Prerequisites

- Complete `/Users/amos/bearcove/peeps/internals/web/000-todo-crate-split-for-parallelization.md`.
- Use contracts from `/Users/amos/bearcove/peeps/internals/web/006-todo-wrapper-emission-api.md`.

## Current context

- Future instrumentation is in `/Users/amos/bearcove/peeps/crates/peeps-tasks/src/futures.rs` and `/Users/amos/bearcove/peeps/crates/peeps-tasks/src/snapshot.rs`.
- Current `peepable` API is label-only; metadata-capable API must be added.

## Node + edge model

Node ID:
- `future:{proc_key}:{future_id}`

Node kind:
- `future`

Required attrs_json:
- `future_id`
- `label`
- `pending_count`
- `ready_count`
- `metadata_json` (arbitrary key/value metadata)

Optional attrs_json:
- `created_by_task_id`
- `last_polled_by_task_id`
- `total_pending_ns`

Required `needs` edges:
- `task -> future` (from explicit poll/wait records)
- `future -> resource` only when explicitly measured

Optional `needs` edges:
- `future -> task` only when explicitly measured as dependency

## Implementation steps

1. Add metadata-capable API:
- `peepable_with_meta(future, label, metadata)`
- keep `peepable(label)` as convenience wrapper.
2. Persist metadata on future node attrs.
3. Emit only explicitly recorded `needs` dependencies.
4. Do not invent edge semantics beyond `needs`.

## Consumer changes

Required:
- Add `peepable_with_meta` at important await points in Roam/Vixen:
  - request_id
  - method
  - channel_id
  - path/resource key

## Validation SQL

```sql
SELECT COUNT(*)
FROM nodes
WHERE snapshot_id = ?1 AND kind = 'future';
```

```sql
SELECT COUNT(*)
FROM edges
WHERE snapshot_id = ?1
  AND kind = 'needs'
  AND (
    (src_id LIKE 'task:%' AND dst_id LIKE 'future:%')
    OR (src_id LIKE 'future:%' AND dst_id LIKE 'lock:%')
    OR (src_id LIKE 'future:%' AND dst_id LIKE 'mpsc:%')
    OR (src_id LIKE 'future:%' AND dst_id LIKE 'oneshot:%')
    OR (src_id LIKE 'future:%' AND dst_id LIKE 'watch:%')
    OR (src_id LIKE 'future:%' AND dst_id LIKE 'semaphore:%')
    OR (src_id LIKE 'future:%' AND dst_id LIKE 'oncecell:%')
  );
```
