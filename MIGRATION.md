# Migration Spec: Consolidate peeps-* Crates into `peeps`

## Goal

Consolidate `peeps-futures`, `peeps-locks`, and `peeps-sync` into the top-level `peeps` crate so consumers can use:

- `use peeps::Mutex;`
- `use peeps::channel;`
- `use peeps::peep;`

while preserving feature-gated diagnostics and the canonical graph emission behavior.

Use the enabled/disabled module pattern everywhere diagnostics are optional:

#[cfg(feature = "diagnostics")]
mod enabled;
#[cfg(not(feature = "diagnostics"))]
mod disabled;

#[cfg(feature = "diagnostics")]
pub use enabled::*;
#[cfg(not(feature = "diagnostics"))]
pub use disabled::*;

**Additional requirement:** create a **single shared diagnostics registry** covering every tracked resource type (futures, locks, channels, oncecell, semaphores, RPC request/response, RPC tx/rx, etc.).

**Hard constraint:** tasks do not exist in the canonical graph model. If task IDs exist anywhere, they are metadata only and must not create task nodes/edges.

## Non-Goals

- Do not add new instrumentation features beyond current behavior.
- Do not reintroduce task tracking (task nodes/edges, task snapshots, wake edges).
- Do not preserve `peeps-futures`, `peeps-locks`, or `peeps-sync` as separate published crates.

## Constraints

- Diagnostics must compile away to zero-cost stubs when the `diagnostics` feature is disabled.
- The spec is the source of truth; avoid ad-hoc manual workarounds.
- The new centralized registry must be the only authoritative registry for diagnostics.

---

## Target API Surface (examples)

Top-level exports from `peeps`:

- Locks
  - `Mutex`, `RwLock`
- Sync primitives
  - `channel`, `unbounded_channel`, `oneshot_channel`, `watch_channel`
  - `Sender`, `Receiver`, `UnboundedSender`, `UnboundedReceiver`
  - `OneshotSender`, `OneshotReceiver`
  - `WatchSender`, `WatchReceiver`
  - `Semaphore`
  - `OnceCell`
- Futures
  - `peep` (macro only)
- Graph collection
  - `collect_graph` uses the single registry to emit all resources

---

## Unified Registry Design

Create a new module `peeps::registry`:

## Canonical Graph Semantics (Stack-Based)

The canonical graph must not contain redundant shortcut paths.

To enforce this, all edge emission is mediated by a **task-local node stack**.

### Invariants

- The canonical graph contains no task nodes or task edges.
- All canonical edges are immediate `needs` edges:
  - `top_of_stack --needs--> resource_endpoint`
  - `tx --needs--> rx` for gateway resources (channels)
  - `request --needs--> response` for RPC
- No transitive shortcuts:
  - If `A --needs--> B` and `B --needs--> C`, do not also emit `A --needs--> C`.

### What “stack” means here

This is not an OS thread stack and not “tokio task” nodes in the graph.

It is a **logical async stack** of *instrumented nodes* representing the current execution chain
within a running async task:

`future -> future -> future -> ...`

Only the **top** is allowed to emit `needs` edges to resources.

### Stack API (required)

Implement a small API in `peeps` (or `peeps-tasks` during migration) gated by `diagnostics`:

- `stack::push(node_id: &str)`
- `stack::pop()`
- `stack::top() -> Option<&str>`
- `stack::with_top(|top| { ... })`

Implementation requirement: task-local storage (e.g. `tokio::task_local!`) so it follows async
execution across threads.

When `diagnostics` is disabled, all stack operations compile away to no-ops.

### How `peepable` integrates with the stack

Each `PeepableFuture` must have a stable canonical node ID for its lifetime (not per poll).

In `PeepableFuture::poll` (diagnostics enabled):

1. `stack::push(self.node_id)`
2. poll inner future
3. `stack::pop()`

This makes nested `peepable(...)` calls create a proper chain without requiring shortcut edges.

### How wrappers emit edges using the stack

Wrappers for locks/channels/semaphores/etc. must emit edges only via the stack.

At the moment a wrapper determines it is *actually waiting* (contended lock, full buffer, empty
recv, no permits, etc.), it must do:

- `stack::with_top(|src| registry::edge(src, resource_endpoint_id))`

Wrappers must not:

- emit `task -> resource` edges
- emit creator edges as `needs` edges
- emit `request -> resource` shortcut edges unless the request node is literally on top of stack

### Gateway resources

Some resources are gateways that intentionally create a “bridge” edge:

- `mpsc_tx --needs--> mpsc_rx` (progress of tx ultimately depends on rx draining)
- `oneshot_tx --needs--> oneshot_rx`
- `watch_tx --needs--> watch_rx`
- `request --needs--> response`

These edges are structural and always allowed because they represent the gateway itself.

### Registry Responsibilities

- Central storage of all live diagnostics objects, keyed as weak references.
- Snapshot extraction per resource type.
- Canonical graph emission per resource type.
- Shared process metadata (process name, proc_key).

### Registry Contents (minimum)

- Futures wait info and composition edges (no task nodes/edges; any task IDs are metadata only)
- Locks (mutex, rwlock)
- Channels:
  - mpsc
  - oneshot
  - watch
- Semaphores
- OnceCell
- RPC request/response and RPC channel endpoints (tx/rx) emitted directly by the RPC/channel wrappers (no projection layer)

### Registry Interface (sketch)

- `registry::init(process_name, proc_key)` — initializes registry and process metadata
- `registry::emit_graph()` — emits canonical nodes/edges for all resources

All resource modules must register themselves into this registry, never maintain private registries.

---

## Migration Steps

### 1) Create `peeps::registry`
- New module under `peeps/src/registry.rs`.
- Aggregates registries currently living in `peeps-sync` and `peeps-locks`.
- Adds future-related registries from `peeps-futures`.
- Add RPC request/response and RPC channel endpoint tracking by instrumenting the RPC/channel wrappers themselves (no “collect session then project”).

### 2) Move `peeps-futures` into `peeps`
- Create `peeps/src/futures/{mod.rs,enabled.rs,disabled.rs}` (shape similar to `peeps-locks`).
- Remove all task tracking references:
  - `TaskId`, `TaskSnapshot`, `TaskState`, `WakeEdgeSnapshot`, `task_name()`, `current_task_id()`.
- Update futures instrumentation to produce only future-related nodes/edges.
- Register all futures diagnostics in the **central registry**.

### 3) Move `peeps-locks` into `peeps`
- Create `peeps/src/locks/{mod.rs,enabled.rs,disabled.rs}`.
- Replace any `current_task_id()` usage with nothing (tasks are not part of the model).
- Register lock info in the **central registry** (no private lock registry).
- Preserve `DiagnosticMutex`, `DiagnosticRwLock` behavior.

### 4) Move `peeps-sync` into `peeps`
- Create `peeps/src/sync/{mod.rs,channels.rs,semaphore.rs,oncecell.rs,enabled.rs,disabled.rs}`.
- Replace `crate::registry` usage with the new `peeps::registry`.
- Remove all `peeps_futures::task_name()` and `current_task_id()` references.

### 5) Update `collect_graph`
- Replace all calls to old crate-specific `emit_graph` functions.
- Use `registry::emit_graph(process_name, proc_key)` to emit all nodes/edges.
- Remove any roam “session snapshot → projection” style collection. RPC/channel wrappers must emit canonical nodes/edges directly into the registry.

### 6) Update `peeps` Public API
- All internal modules (`futures`, `locks`, `sync`, `registry`, `stack`) are `pub(crate)` only.
- Re-export all public types at the crate root so consumers use `peeps::Mutex`, `peeps::channel`, `peeps::Semaphore`, etc.
- Flat API surface similar to tokio — no public submodule paths.

### 7) Remove old crates
- Delete `crates/peeps-futures`, `crates/peeps-locks`, `crates/peeps-sync`.
- Remove dependencies from workspace and `peeps/Cargo.toml`.
- Update any references across the repo.

---

## Diagnostics Feature Flags

- Keep single `diagnostics` feature in `peeps`.
- Internally gate diagnostic codepaths (`#[cfg(feature = "diagnostics")]`).
- No cross-crate feature propagation since subcrates are removed.

---

## Verification Checklist

- `use peeps::Mutex` and `use peeps::channel` compile.
- All diagnostics compile away when `diagnostics` is disabled.
- `collect_graph` returns expected canonical nodes/edges.
- No references to removed task tracking remain.
- Registry is single source of truth for all resource tracking.

---

## Open Decisions (confirm with owner)

- Exact wrapper boundaries for RPC instrumentation (which crate emits what, but still direct-to-registry).
