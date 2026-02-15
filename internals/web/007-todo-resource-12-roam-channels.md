# Resource Track: Roam Channels

Status: todo
Owner: wg-resource-roam-channels
Priority: P1

## Mission

Expose roam channel usage as first-class resources tied to tasks and requests.

## Prerequisites

- Complete `/Users/amos/bearcove/peeps/internals/web/000-todo-crate-split-for-parallelization.md`.
- Use contracts from `/Users/amos/bearcove/peeps/internals/web/006-todo-wrapper-emission-api.md`.

## Current context

- Roam diagnostics include channel details (`channel_id`, `direction`, `queue_depth`, task/request ids when present).
- Channel objects do not carry independent metadata, but channel creation is request-scoped.
- Canonical node/edge mapping is not yet guaranteed for all channel events.

Implementation areas:
- `/Users/amos/bearcove/roam/rust/roam-session/src/diagnostic.rs`
- `/Users/amos/bearcove/peeps/crates/peeps/src/collect.rs`

## Node + edge model

Node IDs:
- `roam-channel:{proc_key}:{channel_id}:tx`
- `roam-channel:{proc_key}:{channel_id}:rx`

Node kinds:
- `roam_channel_tx`
- `roam_channel_rx`

Required attrs_json (both endpoints):
- `channel_id`
- `name`
- `direction`
- `queue_depth`
- `closed`
- `request_id`
- `task_id`

Required `needs` edges:
- `task -> roam-channel:{...}:tx` when task is blocked on send-side progress
- `task -> roam-channel:{...}:rx` when task is blocked on recv-side progress
- `roam-channel:{...}:tx -> roam-channel:{...}:rx` endpoint dependency
- `request -> roam-channel:{...}:tx|rx` when request context is explicitly linked

## Implementation steps

1. Emit tx/rx endpoint nodes for each roam channel.
2. Emit `tx -> rx` `needs` edge for each channel.
3. On receiver-side channel provisioning, derive request provenance from request metadata/context and emit `request -> endpoint` `needs` edge.
4. Emit task->endpoint `needs` edges only when task linkage is explicit.
5. Do not synthesize links from names.

## Consumer changes

- Usually none if roam internals are instrumented centrally.
- Add missing instrumentation at internal roam channel ops if edge coverage is sparse.

## Validation SQL

```sql
SELECT COUNT(*)
FROM nodes
WHERE snapshot_id = ?1 AND kind IN ('roam_channel_tx','roam_channel_rx');
```

```sql
SELECT src_id, dst_id
FROM edges
WHERE snapshot_id = ?1
  AND kind = 'needs'
  AND src_id LIKE 'roam-channel:%:tx'
  AND dst_id LIKE 'roam-channel:%:rx'
LIMIT 100;
```
