# Wrapper Emission API Spec

Status: todo
Owner: wg-wrapper-api
Scope: `peeps-types` + wrapper crates (`peeps-tasks`, `peeps-locks`, `peeps-sync`) + roam diagnostics bridge

## Goal

All wrappers emit canonical graph rows through one API shape:
- nodes
- `needs` edges

No inferred/derived/heuristic edges.

## Canonical API (source of truth)

Defined in `peeps-types`:
- `GraphNodeSnapshot`
- `GraphEdgeSnapshot`
- `GraphSnapshot`
- `GraphSnapshotBuilder`

`ProcessDump.graph` is the migration bridge; `graph` becomes source-of-truth for `peeps-web`.

## Required contract

1. Every instrumented runtime/resource entity is emitted as a node.
2. Every dependency is emitted as a `needs` edge.
3. If dependency cannot be measured explicitly, do not emit an edge.
4. Process is context (`node.process`), not a node.
5. Threads are out of current graph scope.

## Canonical IDs (v1)

Define:
- `proc_key = {process}:{pid}` (or stable runtime instance id when pid reuse is a concern)
- `connection` must be a sanitized stable token: `conn_{u64}` (not raw socket string)

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

## Edge model

Only edge kind:
- `needs`

Required fields:
- `src_id`
- `dst_id`
- `kind = needs`

Optional future field:
- `observed_at_ns`

Not part of base edge model:
- `blocking`
- `duration_ns`
- `count`
- `why`

## Wrapper responsibilities

### peeps-tasks

Emit nodes:
- task
- future

Emit `needs` edges:
- task -> future
- future -> task/resource only when explicitly measured

Consumer impact:
- migrate critical callsites to `spawn_tracked*`
- add metadata-capable `peepable_with_meta(...)` for high-value futures

### peeps-locks

Emit nodes:
- lock

Emit `needs` edges:
- task -> lock (waiter)
- lock -> task (holder)

Namespace rule:
- only `peeps_task_id` is valid for task identity.
- local holder/waiter token IDs never leave wrapper internals.

### peeps-sync (tokio channels + semaphore + oncecell)

Emit nodes:
- channel endpoints (tx/rx)
- semaphore
- oncecell

Emit `needs` edges:
- task -> endpoint (send/recv-side dependencies)
- endpoint tx -> endpoint rx
- task -> semaphore
- task -> oncecell (wait/init dependencies where explicit)

MPSC required node attrs include:
- `queue_len`, `capacity`, `high_watermark`, `sender_count`, `send_waiters`, closed flags, totals

### roam diagnostics bridge

Emit nodes:
- request + response
- roam-channel endpoints

Emit `needs` edges:
- request -> response (receiver emits this edge)
- request -> downstream request (explicit propagated context only)
- task/request -> roam-channel endpoint when explicitly linked
- roam-channel tx -> roam-channel rx

Request attrs requirement:
- include Roam-style `args_preview` (readable scalars + middle-elided large binaries)

## Hard validation rules

At ingest, reject/quarantine rows when:
- edge kind != `needs`
- `src_id` or `dst_id` missing in same snapshot
- empty/unknown node kind

## Acceptance criteria

1. Wrapper crates emit canonical nodes + `needs` edges only.
2. No inferred/derived edges are persisted.
3. `peeps-web` request-centered graph traversal works from canonical rows only.
