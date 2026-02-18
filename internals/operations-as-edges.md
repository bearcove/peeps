# Operations As Edges

This file is the single source of truth for moving primitive operations from wrapper-future nodes to operation edges.

## Problem

Right now, we can end up graphing wrapper internals instead of the program's intent.

Example of noisy shape:

- `permit_waiter -> gate.acquire.blocked -> semaphore.acquire_owned -> demo.api_gate`

What we actually care about:

- which actor is blocked
- on which resource
- for how long
- and who currently holds that resource (if applicable)

## Outcome We Want

A graph that reads like execution intent:

- actor nodes are intentional (`peeps!` futures/scopes we choose to keep)
- resource nodes are stateful runtime resources (channel, semaphore, lock, connection leg)
- primitive operations are edges (`send`, `recv`, `acquire`, `lock`, etc.)

This keeps causality visible without flooding the graph with wrapper implementation details.

## Hard Invariants

- [ ] Edges remain `Entity -> Entity` only.
- [ ] Scopes are never edge endpoints.
- [ ] Scopes are context/membership only (`entity_scope_links` + scope inspector/table).
- [ ] Primitive wrappers do not create wrapper-internal future nodes.
- [ ] RPC `request`/`response` remain entities (not reduced to one edge).

## Canonical Model Rules

### 1) Actor vs resource split

- Actor entities: named futures/tasks/scopes we intentionally instrument.
- Resource entities: channel endpoints, semaphore, lock, connection leg, etc.

### 2) Primitive operations become edges

Primitive operations are represented as edges from actor to resource:

- `actor --send--> channel_tx`
- `actor --recv--> channel_rx`
- `actor --acquire--> semaphore`
- `actor --lock--> mutex`

### 3) Blocking is edge state, not a separate node

A blocked wait is expressed as edge metadata (`pending` + timestamps), not a wrapper node like `semaphore.acquire_owned`.

### 4) RPC stays lifecycle-first

Request/response are entities because they:

- span time
- cross processes
- can trigger nested calls
- have independent state and attributes

## Edge Metadata Contract (Operation Edges)

Operation edges should carry this metadata in `edge.meta`:

- `op_kind`: `send | recv | acquire | lock | notify_wait | oncecell_wait | ...`
- `state`: `active | pending | done | failed | cancelled`
- `pending_since_ptime_ms`: `u64 | null`
- `last_change_ptime_ms`: `u64`
- `source`: absolute file path + line
- `krate`: optional crate name
- `poll_count`: optional counter for churn diagnostics
- `details`: optional typed payload (capacity/occupancy, permits requested, etc.)

Notes:

- [ ] `age_ms` should be derived in UI/backend from `ptime_now_ms - pending_since_ptime_ms`, not stored redundantly.
- [ ] `state=pending` is the canonical "blocked" signal for graph styling.

## RPC + Cross-Process Rules

- [ ] Keep `Request` and `Response` as entities.
- [ ] Keep `EdgeKind::RpcLink` for request/response pairing.
- [ ] Use scope membership to associate request/response with connection scope.
- [ ] Do not add edges to scope nodes.

Cross-process correlation should come from metadata, not UI merge tricks:

- `connection.correlation_id`
- `request_id`
- direction/role metadata as needed

## Pairing And Merging Rules (UI)

Pairing is model-level. Merging is view-level.

- [ ] Keep paired entities separate in storage.
- [ ] Pairing source of truth includes `ChannelLink` (`tx -> rx`).
- [ ] Pairing source of truth includes `RpcLink` (`request -> response`).
- [ ] Merge paired cards only when both endpoints belong to the same process.
- [ ] For cross-process pairs, do not merge; render both with a visible pair link.
- [ ] Node color always follows owning process, even when paired.

## Clean Break Plan

No dual-write period. No compatibility mode. Land the new model and delete legacy behavior in the same change set.

### Cutover A - Model and runtime

- [ ] Define operation-edge metadata schema in `peeps-types` docs/comments.
- [ ] Add helper APIs in `peeps` registry for upsert/update/remove operation edges.
- [ ] Channel wrappers emit operation edges (`send`, `recv`, etc.) and stop emitting wrapper-internal future nodes.
- [ ] Lock/semaphore wrappers emit operation edges (`lock`, `acquire`) and stop emitting wrapper-internal future nodes.
- [ ] Notify/oncecell wrappers emit operation edges for waits and stop emitting wrapper-internal future nodes.
- [ ] Keep only intentional actor futures + resource entities + RPC lifecycle entities.

### Cutover B - Frontend and queries

- [ ] Frontend blocked styling keys off operation-edge pending state.
- [ ] Inspector shows operation-edge details (state, pending duration, source, crate).
- [ ] Graph layout/ranking assumes no primitive wrapper-internal future nodes.
- [ ] Query packs and any graph helpers stop relying on legacy wrapper-node shapes.

### Cutover C - Legacy removal and validation

- [ ] Remove dead code paths and helpers that only supported wrapper-internal primitive nodes.
- [ ] Remove stale docs/examples that describe the old primitive-wrapper-node model.
- [ ] Validation targets:
- [ ] `channel-full-stall`: clear `actor --send(pending)--> channel_tx`.
- [ ] `semaphore-starvation`: clear `waiter --acquire(pending)--> semaphore`, plus holder relation.
- [ ] `oneshot-sender-lost-in-map`: wait edge remains pending with useful metadata.
- [ ] `roam-rust-swift-stuck-request`: request/response remain first-class entities across processes.

## Open Questions

- [ ] Do we keep one generic `EdgeKind::Needs` + `op_kind` in meta, or add explicit operation edge kinds?
- [ ] Should completed operation edges be removed immediately or kept for a short TTL for debuggability?
- [ ] Which operation details belong in edge meta vs events?
- [ ] Do we need edge compaction rules for very high-frequency operations?

## Non-Goals

- Not redesigning scope semantics; scopes remain membership/context.
- Not collapsing RPC lifecycle into a single arrow.
- Not inferring crate name from source path.
