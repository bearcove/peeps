# Resource Track: RPC Requests

Status: todo
Owner: wg-resource-rpc-requests
Priority: P0

## Mission

Make request causality across tasks/processes explicit and queryable.

## Prerequisites

- Complete `/Users/amos/bearcove/peeps/internals/web/000-todo-crate-split-for-parallelization.md`.
- Use contracts from `/Users/amos/bearcove/peeps/internals/web/006-todo-wrapper-emission-api.md`.

## Current context

- Roam session diagnostics surface in-flight requests and metadata.
- `peeps` currently extracts some request-parent data from metadata.
- Cross-process causality quality depends on robust metadata propagation.

Implementation areas:
- `/Users/amos/bearcove/roam/rust/roam-session/src/diagnostic.rs`
- `/Users/amos/bearcove/peeps/crates/peeps/src/collect.rs`

## Node + edge model

Node ID:
- `request:{proc_key}:{connection}:{request_id}`
- `response:{proc_key}:{connection}:{request_id}`

Node kinds:
- `request`
- `response`

Required attrs_json (request):
- `request_id`
- `method`
- `method_id`
- `direction` (`incoming|outgoing`)
- `elapsed_ns`
- `connection` (`conn_{u64}`)
- `peer`
- `task_id` (nullable)
- `metadata_json`
- `correlation_key` (`{connection}:{request_id}`)
- `args_preview` (Roam-formatted argument rendering)

`args_preview` requirement:
- must use the same formatting style as Roam diagnostics
- keep scalar readability (numbers/strings/bools)
- for large binary payloads, middle-elide content while preserving prefix/suffix context
- never drop argument visibility entirely just because payload is large

Required attrs_json (response):
- `request_id`
- `method`
- `status` (`ok|error|cancelled|timeout|in_flight`)
- `elapsed_ns`
- `connection` (`conn_{u64}`)
- `peer`
- `server_task_id`
- `correlation_key` (`{connection}:{request_id}`)

Required `needs` edges:
- `request -> response` (request depends on response completion)
- `request -> request` only for explicit downstream RPC dependencies
- `request -> task` only when request progress explicitly depends on handler task

## Implementation steps

1. Caller side emits request node.
2. Receiver side emits response node.
3. Receiver side emits `request -> response` `needs` edge.
4. Emit cross-request `needs` only from explicit propagated context metadata.
5. Keep chain/span identifiers as attrs only; no inferred parent linking.

## Consumer changes

Required:
- Ensure all outbound RPC call sites propagate parent request context metadata.
- Ensure server-side handlers keep task/request association fields populated.

## Validation SQL

```sql
SELECT src_id, dst_id
FROM edges
WHERE snapshot_id = ?1
  AND kind = 'needs'
  AND src_id LIKE 'request:%'
  AND dst_id LIKE 'response:%'
LIMIT 200;
```

```sql
SELECT COUNT(*)
FROM nodes r
LEFT JOIN nodes s
  ON s.snapshot_id = r.snapshot_id
 AND s.kind = 'response'
 AND json_extract(s.attrs_json, '$.correlation_key') = json_extract(r.attrs_json, '$.correlation_key')
WHERE r.snapshot_id = ?1
  AND r.kind = 'request'
  AND s.id IS NULL;
```
