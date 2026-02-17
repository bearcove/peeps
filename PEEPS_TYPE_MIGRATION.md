# peeps migration guide (single source of truth)

When your async service freezes, the question is not "did we preserve every old field?".
The question is "can we explain who is waiting on whom, and why this wait is not resolving?".

This guide adopts `peeps-types` as the source of truth and keeps only information that helps deadlock diagnosis.

## Model-first stance

This is a model shift, not a compatibility exercise.

- `Entity` holds long-lived runtime objects.
- `Edge` holds relationships.
- `Event` holds transitions/outcomes.
- `PTime` (`birth`, event `at`) is canonical time.

Legacy field names that contain `node` are legacy naming only. In the new model they map to entity identity and relationships.

## Canonical decisions

1. `created_at_ns` is conceptually replaced by `birth`.
- Use `Entity.birth` (`PTime`) as canonical creation time.
- Do not preserve absolute Unix ns by default.

2. Timer/sleep instrumentation is intentionally out of scope.
- `sleep` / `interval` / `interval_at` telemetry is dropped.
- Reason: it did not help deadlock detection.

3. Keep only deadlock-relevant diagnostics.
- If a field does not improve deadlock explanation, drop it.
- If uncertain, keep briefly in `meta`, then remove if unused.

## Classification buckets

Every legacy detail should land in exactly one bucket:

1. `Top-level field`
2. `Body field`
3. `meta`
4. `Event`
5. `Edge`
6. `Likely oversight/blindspot`

## Cross-cutting mapping rules

| Legacy detail | Bucket | Target |
| --- | --- | --- |
| endpoint id (`*_node_id`) | `Top-level field` | `Entity.id` |
| location/callsite | `Top-level field` | `Entity.source` |
| creation time (`created_at_ns`) | `Top-level field` | `Entity.birth` |
| label/name | `Top-level field` | `Entity.name` |
| runtime typed state | `Body field` | `EntityBody::*` |
| send/recv/close/wait transitions | `Event` | typed sync events |
| tx-rx pairing | `Edge` | `EdgeKind::ChannelLink` |
| request-response pairing | `Edge` | `EdgeKind::RpcLink` |
| active blocking dependency | `Edge` | `EdgeKind::Needs` |
| closure causality | `Edge` or `Event` | `ClosedBy` and/or close event |
| high-watermark, utilization, counters | `meta` | optional diagnostics only |
| freeform close-cause strings | `meta` | optional compatibility/debug context |
| waiter bookkeeping internals (`active_waiter_starts`, waiter lists) | `Edge` and `Event` | do not persist raw lists; emit `Needs` edge lifecycle + wait start/end events |

## Deadlock-focused wrapper/entity mapping

| Legacy wrapper/entity | Keep in top-level/body | Keep as events/edges | Optional `meta` | Drop |
| --- | --- | --- | --- | --- |
| `sync::channel` / `unbounded_channel` | ids, source, name, lifecycle, capacity | sent/received/wait/closed events, `ChannelLink`, `Needs` | high-watermark, waiter/sender counters, utilization | legacy string parity |
| `sync::oneshot_channel` | ids, source, name, oneshot state | send/recv/closed events, `ChannelLink`, `Needs` | freeform drop reasons if needed | anything not improving closure/wait reasoning |
| `sync::watch_channel` | ids, source, name, watch details | send/changed/wait events, `ChannelLink`, `Needs` | receiver counts, change counters | noisy legacy mirrors |
| `sync::Semaphore` | ids, source, name, permits state | acquire outcomes, wait events, `Needs` | wait aggregates and watermarks | raw per-waiter scratch lists |
| `sync::Notify` | ids, source, name, waiter count | notify/wait transitions, `Needs` | wakeup/notify counters and aggregates | raw waiter-start lists |
| `sync::OnceCell` | ids, source, name, state | init attempt/completion events, `Needs` | init duration/retry counters | raw internal waiter lists |
| `locks::{Mutex,RwLock,AsyncMutex,AsyncRwLock}` | ids, source, name, `EntityBody::Lock(kind)` | lock wait/acquire/release events (usually `StateChanged`) + `Needs` during contention | holder/waiter/acquire/release counts | internal lock token bookkeeping |
| `futures::peepable` / tracked futures | ids, source, name, `EntityBody::Future` | `Needs` while parent awaits child | poll/pending/idle stats if they help diagnosis | verbose poll telemetry by default |
| `futures::spawn_tracked` / `spawn_blocking_tracked` | future entities | lineage if represented (see blindspots) | spawn context info | legacy spawn/touch parity if no model slot |
| `command::{Command,Child}` | ids, source, name, `EntityBody::Command(program,args,env)` | `Needs` while waiting for child/status/output, lifecycle events | pid, exit code/signal, elapsed, error | transient formatting artifacts |
| `fs::*` wrappers | ids, source, name, `EntityBody::FileOp(op,path)` | optionally `Needs` for blocking waits | byte counts, op-specific attrs | operation spam without causal value |
| `net::{connect,accept,readable,writable}` | ids, source, name, `EntityBody::Net*` + address | `Needs` while readiness/connect waits are blocked | transport, elapsed | duplicate per-poll timing |
| `rpc::{record_request,record_response}` | request/response as `EntityBody::Request/Response`; rpc connection as `Scope` | `RpcLink` request->response, `Needs` only when truly blocked, scope-targeted lifecycle/state events | correlation keys, method args preview | legacy standalone connection entity |
| `JoinSet` wrapper | model joinset as `Scope` (not entity) | scope-targeted lifecycle/state events; optional lineage edges if added | cancelled/close-cause if useful | legacy standalone joinset entity |
| timers (`sleep`, `interval`, `interval_at`) | none | none | none | intentionally excluded |

## Typed baseline to emit

Minimum stream that keeps deadlock analysis useful:

- Edges:
  - `Needs`
  - `ChannelLink`
  - `RpcLink`
  - `ClosedBy`
- Events:
  - `ChannelSent`
  - `ChannelReceived`
  - `ChannelClosed`
  - `ChannelWaitStarted`
  - `ChannelWaitEnded`
  - `StateChanged` for non-channel lifecycle transitions (locks, command lifecycle, request/response state)

## Blindspots and resolved decisions

These are real model gaps or intentional choices, not failures to preserve legacy bytes:

1. Legacy lineage edges (`spawned`, `touches`) have no direct `EdgeKind` in current `peeps-types`.
- Decision needed: ignore, encode in `meta`, or add explicit edge kinds.

2. `JoinSet` modeling is resolved: represent joinsets as scopes.
- Use `Scope` as the long-lived container for joinset-owned work.
- Emit scope-targeted events for lifecycle transitions (`abort_all`, close-cause, cancellation).

3. RPC connection modeling is resolved: represent connections as scopes.
- Use a connection scope as the parent execution context for related request/response entities.
- Keep request/response pairing via `RpcLink`; keep connection identity in scope fields/meta.

4. Lock-specific typed wait events do not exist beyond generic `StateChanged`.
- Decision needed: keep lock transitions under `StateChanged` meta or add lock event kinds.

5. There is no first-class edge shape from `Entity` to `Scope` in the current edge model.
- Decision needed: represent membership/context via event targeting + metadata for now, or extend the model with explicit entity-scope linkage.

## What disappears on purpose

- Absolute timestamp compatibility (`created_at_ns` as canonical field).
- Timer/sleep/interval entity and tick telemetry.
- Legacy string-event parity as a migration objective.
- Counter dumps that do not explain blocked dependency chains.

## Implementation order

1. Build entities with correct `EntityBody` and canonical top-level fields.
2. Emit structural edges (`ChannelLink`, `RpcLink`) and wait edges (`Needs`).
3. Replace legacy string events with typed events.
4. Add only justified `meta` keys.
5. Resolve remaining blindspots explicitly with product intent, not legacy parity pressure.

That is the migration contract: preserve deadlock signal, not legacy shape.
