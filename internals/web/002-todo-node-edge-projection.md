# Node/Edge Projection Spec

Status: todo
Owner: wg-projection
Scope: wrapper emission -> canonical graph rows

## Goal

Project instrumentation output into one graph model:
- canonical nodes
- canonical `needs` edges

This spec must match `/Users/amos/bearcove/peeps/internals/web/GRAPH.md`.

## Node model

Every runtime/resource entity is a node with:
- `id`
- `kind`
- `process` (context/grouping attribute)
- `attrs_json`

Not a node:
- process itself
- thread itself (out of current scope)

## Edge model

Only edge kind:
- `needs`

Meaning:
- source depends on destination for progress.
 - this includes currently-blocked, causal, and structural dependency topology (used for traversal/SCC with node-state filtering).

Required edge fields:
- `src_id`
- `dst_id`
- `kind = needs`

Optional edge fields (future):
- `observed_at_ns`

Not in base model:
- `blocking`
- `duration_ns`
- `count`
- `why` enums
- per-edge severity

## ID conventions (v1)

Define:
- `proc_key = {process}:{pid}` (or stable runtime instance id)
- `connection = conn_{u64}` (sanitized stable token)

IDs:
- task: `task:{proc_key}:{task_id}`
- future: `future:{proc_key}:{future_id}`
- request: `request:{proc_key}:{connection}:{request_id}`
- response: `response:{proc_key}:{connection}:{request_id}`
- lock: `lock:{proc_key}:{name}`
- semaphore: `semaphore:{proc_key}:{name}`
- mpsc endpoints: `mpsc:{proc_key}:{name}:tx|rx`
- oneshot endpoints: `oneshot:{proc_key}:{name}:tx|rx`
- watch endpoints: `watch:{proc_key}:{name}:tx|rx`
- roam channel endpoints: `roam-channel:{proc_key}:{channel_id}:tx|rx`
- oncecell: `oncecell:{proc_key}:{name}`

## Mandatory dependency patterns

- task -> future
- future -> lock/channel/semaphore/oncecell (when explicitly measured)
- channel tx -> channel rx
- request -> response
- request -> downstream request (explicit propagation only)

## Acceptance criteria

1. No edge kinds other than `needs` are persisted.
2. No inferred/derived edges are persisted.
3. Channel and RPC dual-node patterns are represented (tx/rx, request/response).
