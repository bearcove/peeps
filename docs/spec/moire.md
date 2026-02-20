# Moire Specification

Moire is an instrumentation library for Rust systems built on Tokio. It captures stack traces, tracks async tasks, locks, channels, and RPC calls, and pushes structured data to a dashboard server.

The dashboard presents the runtime state as a graph of entities connected by edges, grouped and colored by scope. Nodes can be inspected individually, and a filter bar allows grouping, coloring, and filtering the graph interactively.

---

## Instrumented Processes

> r[process.dependency]
> To be instrumented, a process MUST depend on the `moire` crate and use its wrappers in place of the underlying primitives — for example `moire::Mutex` instead of `parking_lot::Mutex`, `moire::mpsc` instead of `tokio::sync::mpsc`, and `moire::spawn` instead of `tokio::spawn`.

> r[process.feature-gate]
> Instrumentation is only active when the `diagnostics` feature of the `moire` crate is enabled. Without it, all instrumentation APIs compile to no-ops.

> r[process.auto-init]
> When the `diagnostics` feature is enabled, the `moire` crate MUST use the `ctor` crate to automatically initialize the runtime and start the dashboard push loop at program startup, with no user code required.

---

## Configuration

### Instrumented process

An instrumented process is any binary that depends on `moire` with the `diagnostics` cargo feature enabled.

> r[config.dashboard-addr]
> The instrumented process reads `MOIRE_DASHBOARD` at startup. If set to a non-empty `<host>:<port>` string, it initiates a persistent TCP push connection to that address.

> r[config.dashboard-feature-gate]
> If `MOIRE_DASHBOARD` is set but the `diagnostics` feature is not enabled, the process MUST emit a warning to stderr and MUST NOT attempt to connect.

> r[config.dashboard-reconnect]
> If the connection to the dashboard is lost, the process MUST attempt to reconnect after a delay. It MUST NOT crash or log an unrecoverable error on connection failure.

### moire-web server

`moire-web` is the dashboard server. It accepts TCP pushes from instrumented processes and serves an HTTP investigation UI.

> r[config.web.tcp-listen]
> `moire-web` reads `MOIRE_LISTEN` for the TCP ingest address. Default: `127.0.0.1:9119`.

> r[config.web.http-listen]
> `moire-web` reads `MOIRE_HTTP` for the HTTP UI address. Default: `127.0.0.1:9130`.

> r[config.web.db-path]
> `moire-web` reads `MOIRE_DB` for the SQLite database file path. Default: `moire-web.sqlite`.

> r[config.web.vite-addr]
> In dev mode, `moire-web` reads `MOIRE_VITE_ADDR` for the Vite dev server proxy address.

---

## Public API

The `moire` crate re-exports the appropriate backend based on target:

> r[api.backend.native]
> On native targets, `moire` re-exports `moire-tokio`. When the `diagnostics` feature is enabled, all wrappers are instrumented. When it is not, they compile to zero-overhead pass-throughs.

> r[api.backend.wasm]
> On `wasm32` targets, `moire` re-exports `moire-wasm`. Instrumentation is always a no-op on WASM, but the API surface is identical to the native surface so that code compiles for both targets without `#[cfg]` attributes.

### Tasks

> r[api.spawn]
> `moire::spawn(name, future)` wraps `tokio::spawn`. It spawns a named async task registered as a `future` entity with its execution tracked.

> r[api.spawn-blocking]
> `moire::spawn_blocking(name, f)` wraps `tokio::task::spawn_blocking`. It spawns a named blocking task on the blocking thread pool, registered as a `future` entity.

> r[api.joinset]
> `moire::JoinSet` wraps `tokio::task::JoinSet`. `JoinSet::named(name)` creates a named join set. Tasks added via `JoinSet::spawn(label, future)` are individually tracked. Awaiting `JoinSet::join_next()` is instrumented.

### Channels

> r[api.mpsc]
> `moire::channel(name, capacity)` and `moire::unbounded_channel(name)` wrap `tokio::sync::mpsc`. Sends and receives are recorded as `channel_sent` and `channel_received` events, including wait duration and close status.

> r[api.broadcast]
> `moire::broadcast(name, capacity)` wraps `tokio::sync::broadcast`. Sender lag is tracked on the `broadcast_rx` entity.

> r[api.oneshot]
> `moire::oneshot(name)` wraps `tokio::sync::oneshot`. The sender's `sent` flag is tracked.

> r[api.watch]
> `moire::watch(name, initial)` wraps `tokio::sync::watch`. The sender's `last_update_at` timestamp is tracked.

### Synchronization

> r[api.mutex]
> `moire::Mutex::new(name, value)` wraps `parking_lot::Mutex`. Locking is synchronous and blocking, not async. Contention is tracked on the `lock` entity with kind `mutex`.

> r[api.rwlock]
> `moire::RwLock::new(name, value)` wraps `parking_lot::RwLock`. Locking is synchronous and blocking, not async. Contention is tracked on the `lock` entity with kind `rwlock`.

> r[api.semaphore]
> `moire::Semaphore::new(name, permits)` wraps `tokio::sync::Semaphore`. `max_permits` and `handed_out_permits` are tracked.

> r[api.notify]
> `moire::Notify::new(name)` wraps `tokio::sync::Notify`. `waiter_count` is tracked.

> r[api.once-cell]
> `moire::OnceCell::new(name)` wraps `tokio::sync::OnceCell`. `waiter_count` and initialization state are tracked.

### Processes

> r[api.command]
> `moire::Command::new(program)` wraps `tokio::process::Command`. Program, arguments, and environment are recorded on the `command` entity. `spawn()`, `status()`, `output()`, and `wait()` are individually instrumented.

### RPC

The RPC instrumentation exists to support [Roam](https://github.com/bearcove/roam), Moire's companion RPC framework. Roam calls into these APIs directly to register requests and responses as they cross process boundaries.

> r[api.rpc-request]
> `moire::rpc_request(method, args_json)` registers an RPC request entity. The method string is split on the last `.` into `service_name` and `method_name`.

> r[api.rpc-response]
> `moire::rpc_response_for(method, request)` registers a response entity paired with its request via a `paired_with` edge. The response status starts as `pending` and is updated as the call completes.

---

## Data Model

The runtime graph consists of four kinds of objects: **entities**, **edges**, **scopes**, and **events**.

> r[model.summary]
> Events happen to entities. Entities are connected by edges. Entities and edges are connected to scopes. An entity may be associated with multiple scopes over its lifetime — for example, a future can be polled from different tasks and threads.

### Identifiers

Every object has an opaque string identifier. IDs are generated by the instrumented process and must be treated as opaque by consumers.

> r[model.id.format]
> IDs are 16-character strings using a custom hex alphabet where `a–f` are remapped to `p,e,s,P,E,S`. The upper 16 bits encode a process-randomized prefix; the lower 48 bits are a per-process atomic counter starting at 1.

> r[model.id.uniqueness]
> IDs MUST be unique within a single process lifetime. Across processes, uniqueness is probabilistic due to the randomized prefix.

### Process time

> r[model.ptime]
> `PTime` is a process-relative timestamp in milliseconds. Zero corresponds to the first call to `PTime::now()` in that process — effectively process birth. It is monotonic and does not correspond to wall-clock time.

### Source locations

> r[model.source]
> A `SourceId` is an opaque integer that refers to an interned `(file_path, line, crate_name)` triple. It is JSON-safe (fits in a U53). Consumers can resolve a `SourceId` to a human-readable location string.
>
> Note: source locations are being replaced by captured backtraces. A separate spec section will cover the backtrace model once that work is complete.

---

### Entity

An entity is a runtime object that exists over time: a future, a lock, a channel endpoint, a network connection leg, an RPC request, etc.

> r[model.entity.fields]
> Every entity has:
> - `id`: opaque `EntityId`
> - `birth`: `PTime` when the entity was first registered
> - `source`: `SourceId` pointing to the instrumentation call site
> - `name`: human-facing string label
> - `body`: kind-specific data (see below)

> r[model.entity.kinds]
> The following entity kinds exist:
>
> **Async / Tokio primitives:**
> - `future` — a spawned task or instrumented future
> - `lock` — a `parking_lot` mutex or rwlock, with `kind` (`mutex` | `rwlock` | `other`)
> - `mpsc_tx` — mpsc channel sender, with `queue_len` and optional `capacity`
> - `mpsc_rx` — mpsc channel receiver
> - `broadcast_tx` — broadcast sender, with `capacity`
> - `broadcast_rx` — broadcast receiver, with `lag`
> - `watch_tx` — watch sender, with optional `last_update_at`
> - `watch_rx` — watch receiver
> - `oneshot_tx` — oneshot sender, with `sent` flag
> - `oneshot_rx` — oneshot receiver
> - `semaphore` — semaphore, with `max_permits` and `handed_out_permits`
> - `notify` — `Notify`, with `waiter_count`
> - `once_cell` — `OnceCell`, with `waiter_count` and `state` (`empty` | `initializing` | `initialized`)
>
> **System / I/O:**
> - `command` — a spawned child process, with `program`, `args`, and `env` (as `KEY=VALUE` strings)
> - `file_op` — a file operation, with `op` (`open` | `read` | `write` | `sync` | `metadata` | `remove` | `rename` | `other`) and `path`
>
> **Network:**
> - `net_connect` — outbound connection attempt, with `addr`
> - `net_accept` — inbound accepted connection, with `addr`
> - `net_read` — network read operation, with `addr`
> - `net_write` — network write operation, with `addr`
>
> **RPC:**
> - `request` — an outbound or inbound RPC call, with `service_name`, `method_name`, and `args_json`
> - `response` — the reply to a request, with `service_name`, `method_name`, and `status` (`pending` | `ok(json)` | `error(internal(string) | user_json(json))` | `cancelled`)

---

### Edge

An edge is a directed relationship between two entities.

> r[model.edge.fields]
> Every edge has:
> - `src`: `EntityId` — source of the relationship
> - `dst`: `EntityId` — destination of the relationship
> - `source`: `SourceId` — instrumentation call site
> - `kind`: edge kind (see below)

> r[model.edge.kinds]
> The following edge kinds exist:
> - `polls` — the source entity is actively polling the destination (e.g. a task polling a future)
> - `waiting_on` — the source is blocked waiting for the destination (e.g. a receiver awaiting a channel)
> - `paired_with` — the two entities are endpoints of the same logical primitive (e.g. tx/rx pair)
> - `holds` — the source resource is currently held by the destination (e.g. semaphore → permit holder)

---

### Scope

A scope is an execution container that groups entities over time.

> r[model.scope.fields]
> Every scope has:
> - `id`: opaque `ScopeId`
> - `birth`: `PTime` when the scope was first registered
> - `source`: `SourceId` pointing to the instrumentation call site
> - `name`: human-facing string label
> - `body`: kind-specific data (see below)

> r[model.scope.kinds]
> The following scope kinds exist:
> - `process` — OS process, with `pid`
> - `thread` — OS thread, with optional `thread_name`
> - `task` — a Tokio task, with `task_key` (Tokio's internal task ID as a string)
> - `connection` — a logical connection, with optional `local_addr` and `peer_addr`

---

### Event

An event is a point-in-time occurrence attached to an entity or scope.

> r[model.event.fields]
> Every event has:
> - `id`: opaque `EventId`
> - `at`: `PTime` timestamp
> - `source`: `SourceId` pointing to the instrumentation call site
> - `target`: either `entity(EntityId)` or `scope(ScopeId)`
> - `kind`: event kind (see below)

> r[model.event.kinds]
> The following event kinds exist:
> - `state_changed` — the target's observable state has changed (body is inspected via the entity's current `body` field)
> - `channel_sent` — a value was sent on a channel; carries optional `wait_ns` (nanoseconds the send suspended) and `closed` flag
> - `channel_received` — a value was received from a channel; carries optional `wait_ns` and `closed` flag
