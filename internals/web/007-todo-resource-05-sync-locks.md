# Resource Track: Sync Locks

Status: todo
Owner: wg-resource-sync-locks
Priority: P1

## Mission

Handle lock primitives as one coherent track because lock semantics are the same:
- ownership
- waiting
- contention

This track covers sync lock wrappers in `peeps-locks` only.
Async mutex/rwlock wrappers are out of scope (banned).

## Prerequisites

- Complete `/Users/amos/bearcove/peeps/internals/web/000-todo-crate-split-for-parallelization.md`.
- Use contracts from `/Users/amos/bearcove/peeps/internals/web/006-todo-wrapper-emission-api.md`.

## Current context

- Lock wrappers are in `/Users/amos/bearcove/peeps/crates/peeps-locks/src/sync_locks.rs` and `/Users/amos/bearcove/peeps/crates/peeps-locks/src/snapshot.rs`.
- Existing diagnostics track holders/waiters with `peeps_task_id`.
- Internal `holder_id`/waiter token IDs are local bookkeeping only and must not become graph identities.

## Node + edge model

Node ID:
- `lock:{proc_key}:{name}`

Node kind:
- `lock`

Required attrs_json:
- `name`
- `lock_kind` (`mutex|rwlock_read|rwlock_write`)
- `acquires`
- `releases`
- `holder_count`
- `waiter_count`

Required `needs` edges:
- `task -> lock` when task is waiting
- `lock -> task` when lock is currently held (dependency anchor)

## Implementation steps

1. Emit one lock node per tracked lock.
2. Emit wait edges from explicit waiter records only.
3. Emit holder edges from explicit holder records only.
4. Use `peeps_task_id` namespace for task endpoint identity.
5. Never use internal `holder_id`/waiter token IDs outside wrapper bookkeeping.

## Consumer changes

- Transparent where wrapper lock types are used.
- Required migration where raw lock types bypass wrappers.

## Validation SQL

```sql
SELECT json_extract(attrs_json, '$.lock_kind') AS lock_kind, COUNT(*)
FROM nodes
WHERE snapshot_id = ?1 AND kind = 'lock'
GROUP BY json_extract(attrs_json, '$.lock_kind');
```

```sql
SELECT COUNT(*)
FROM edges
WHERE snapshot_id = ?1
  AND kind = 'needs'
  AND (
    (src_id LIKE 'task:%' AND dst_id LIKE 'lock:%')
    OR (src_id LIKE 'lock:%' AND dst_id LIKE 'task:%')
  );
```
