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

### Backtrace Capture

> r[process.frame-pointers]
> Instrumented binaries MUST be compiled with frame pointers enabled: `-C force-frame-pointers=yes` for Rust code, and `-fno-omit-frame-pointer` for any C/C++ dependencies. Without frame pointers, backtrace capture produces incorrect results. This is not detected at runtime and is the caller's responsibility to enforce.

> r[process.frame-pointer-validation]
> At startup, the `moire-trace-capture` crate MUST perform a sanity walk to verify that frame pointers are actually working. It calls a function of known minimum stack depth and walks the frame pointer chain, verifying that the chain reaches at least that depth and that each successive frame pointer is non-null, aligned, and greater than the previous (i.e. the stack is growing in the expected direction). If validation fails, the process MUST panic immediately with an explicit message naming the missing compiler flag (`-C force-frame-pointers=yes`).

> r[process.backtrace-capture]
> At every public instrumented API boundary — every lock acquisition, channel send or receive, spawn, and RPC call — the `moire-trace-capture` crate captures the current call stack. Capture is unconditional: it does not require contention or any other precondition. The captured frames are interned into a `BacktraceRecord` identified by a process-unique `BacktraceId`.

> r[process.backtrace-capture.impl]
> Capture walks the frame pointer chain for the current thread using architecture-specific register conventions — on x86_64, `rbp` points to the saved caller `rbp` at `[rbp]` and the return address at `[rbp+8]`; on aarch64, `x29` points to the saved caller `x29` at `[x29]` and the saved link register at `[x29+8]`. The walk terminates on a null or misaligned frame pointer, when the frame pointer fails to advance, or when the maximum frame count is reached. There is no fallback to DWARF or any other unwinding mechanism. Each collected instruction pointer is resolved to a `(module_path, runtime_base, rel_pc)` triple via `dladdr`, with modules de-duplicated within the capture. The result is a `BacktraceRecord { id, frames: Vec<FrameKey> }` where each `FrameKey` is `{ module_id, rel_pc }`. Capture MUST fail hard — panicking — if any invariant is violated (empty backtrace, missing module info, IP below module base).

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

### Backtraces

> r[model.backtrace]
> A `BacktraceId` is a non-zero integer that refers to an interned `BacktraceRecord`. A `BacktraceRecord` is a non-empty sequence of `FrameKey` values, each identifying a single call frame as a `(ModuleId, rel_pc)` pair. `ModuleId` is a process-local integer that maps to a `ModuleRecord` carrying the module's path, runtime base address, and build identity. Consumers resolve a `BacktraceId` to human-readable symbols via the server-side symbolication pipeline (see [Symbolication]).

> r[model.backtrace.id-layout]
> `BacktraceId` MUST be JavaScript-safe (`<= 2^53 - 1`) and MUST encode process uniqueness explicitly. The required layout is: upper 16 bits = per-process randomized prefix, lower 37 bits = per-process monotonic counter starting at 1. Counter overflow is an invariant violation and MUST panic.

---

### Entity

An entity is a runtime object that exists over time: a future, a lock, a channel endpoint, a network connection leg, an RPC request, etc.

> r[model.entity.fields]
> Every entity has:
> - `id`: opaque `EntityId`
> - `birth`: `PTime` when the entity was first registered
> - `backtrace`: `BacktraceId` captured at the instrumentation call site
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
> - `backtrace`: `BacktraceId` — captured at the instrumentation call site
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
> - `backtrace`: `BacktraceId` captured at the instrumentation call site
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
> - `backtrace`: `BacktraceId` captured at the instrumentation call site
> - `target`: either `entity(EntityId)` or `scope(ScopeId)`
> - `kind`: event kind (see below)

> r[model.event.kinds]
> The following event kinds exist:
> - `state_changed` — the target's observable state has changed (body is inspected via the entity's current `body` field)
> - `channel_sent` — a value was sent on a channel; carries optional `wait_ns` (nanoseconds the send suspended) and `closed` flag
> - `channel_received` — a value was received from a channel; carries optional `wait_ns` and `closed` flag

---

## Wire Protocol

The instrumented process pushes a stream of messages over a persistent TCP connection to `moire-web`. All messages are framed and serialized using the `moire-wire` crate.

### Framing

> r[wire.framing]
> Every message on the wire is length-prefixed: a big-endian `u32` frame length followed by that many bytes of JSON payload. The maximum frame size is 128 MiB. A frame exceeding that limit MUST be rejected. The receiver reads the 4-byte length, reads that many bytes of payload, and deserializes it as JSON.

> r[wire.client-message]
> Messages sent from the instrumented process to the server are variants of the `ClientMessage` type. Each variant serializes as a JSON object with a single key — the snake_case variant name — wrapping the variant payload.

> r[wire.server-message]
> Messages sent from the server to the instrumented process are variants of the `ServerMessage` type. Each variant serializes as a JSON object with a single key — the snake_case variant name — wrapping the variant payload.

### Versioning

> r[wire.magic]
> The first field of every handshake MUST be a protocol magic number — a hardcoded `u32` constant shared between `moire-wire` and `moire-web`. If the magic number received from the client does not match the server's constant, the server MUST reject the connection immediately and close the socket. There is no negotiation: magic number mismatch means the client and server are built from incompatible versions of the protocol.

### Handshake

> r[wire.handshake]
> After the magic number, the client sends a `Handshake` message containing:
> - `process_name`: human-readable name of the instrumented process
> - `pid`: OS process ID
> - `args`: the full command-line argument list (`argv`) of the instrumented process
> - `env`: the complete environment of the instrumented process, as a list of `KEY=VALUE` strings
> - `module_manifest`: a list of `ModuleManifestEntry` values, one per loaded module

> r[wire.handshake.module-manifest]
> Each `ModuleManifestEntry` in the module manifest MUST include:
> - `module_path`: absolute filesystem path to the module binary
> - `runtime_base`: the module's base address in the process's virtual address space at the time of the handshake
> - `identity`: a `ModuleIdentity` — either a `build_id` (ELF) or `debug_id` (Mach-O/PDB) string, non-empty
> - `arch`: target architecture string (e.g. `aarch64`, `x86_64`)

> r[wire.handshake.reject]
> The server MUST reject the connection if any `ModuleManifestEntry` is missing required fields or if the module identity cannot be resolved to debug information. There is no fallback or partial-symbolication mode: all declared modules must be fully resolvable or the connection is refused.

### Message stream

> r[wire.backtrace-record]
> When the instrumented process interns a backtrace it has not previously sent, it emits a `BacktraceRecord` message carrying the `BacktraceId` and the full frame list (`Vec<FrameKey>`). The `BacktraceRecord` message MUST be sent before any entity, edge, scope, or event message that references the same `BacktraceId`. `ModuleId` values in the `FrameKey` list are local to the process and map to entries in the module manifest by position.

---

## Symbolication

> r[symbolicate.server-store]
> The server maintains a mapping from `BacktraceId` to `BacktraceRecord` for every connected process. This mapping persists in SQLite for the lifetime of the session.

> r[symbolicate.stream]
> Symbolication is streamed to clients over `GET /api/snapshot/{snapshot_id}/symbolication/ws`. The initial snapshot response MUST be returned immediately with the backtrace/frame catalog, and then the server sends `SnapshotSymbolicationUpdate` messages carrying changed `frame_id` records plus progress counters (`completed_frames`, `total_frames`, `done`).

> r[symbolicate.stream.stall-completion]
> A symbolication stream MUST NOT remain pending forever. If no frame state changes are observed for the configured stall window, the server MUST force completion by converting remaining `"symbolication pending"` frames into explicit unresolved frames with a concrete reason.

> r[symbolicate.addr-space]
> Address lookup for symbolication MUST account for ASLR. For each frame, the lookup probe passed to the debug resolver is `linked_image_base + rel_pc`, where `linked_image_base` comes from the file-backed object segments of the module debug object.

> r[symbolicate.result]
> A resolved frame includes: demangled function name, crate name, module path within the crate, source file path, and line/column where available. The server caches resolved frames keyed by `(module_identity, rel_pc)` so that repeated requests for the same frame do not re-read debug info.

> r[api.snapshot.frame-catalog]
> Snapshot backtraces are sent as `backtrace_id -> frame_ids`, and frame payloads are sent in a separate deduplicated `frames` catalog keyed by `frame_id`. Clients reconstruct each backtrace by resolving `frame_ids` through that catalog.

> r[api.snapshot.frame-id-stable]
> `frame_id` values in snapshot/stream payloads MUST be deterministic and stable for a given frame identity (`module_identity`, `module_path`, `rel_pc`) so incremental updates can target frames by ID across repeated snapshots and stream updates.

> r[symbolicate.parallel]
> The server MAY resolve multiple `BacktraceId` requests concurrently. Symbolication of one backtrace MUST NOT block symbolication of another.

> r[symbolicate.hard-failure]
> If a frame cannot be resolved — because debug info is missing, corrupt, or does not cover the given `rel_pc` — the server MUST store an explicit unresolved marker for that frame. It MUST NOT silently drop the frame or substitute a placeholder. The unresolved marker MUST include the raw `(module_path, rel_pc)` so the caller can diagnose the gap.
