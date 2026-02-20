# moire (mwah-ray)

Runtime graph instrumentation for Tokio-based Rust systems.

Moiré replaces Tokio's primitives with named, instrumented wrappers. At every API boundary — every lock acquisition, channel send/receive, spawn, and RPC call — it captures the current call stack via frame-pointer walking. The resulting graph of entities (tasks, locks, channels, RPC calls) connected by typed edges (polls, waiting_on, paired_with, holds) is pushed as a live stream to `moire-web` for investigation.

The dashboard shows you which tasks are stuck, what they are waiting on, and exactly where in your code they got there.

## What is instrumented

- **Tasks**: `moire::spawn`, `moire::spawn_blocking`, `moire::JoinSet`
- **Channels**: mpsc, unbounded, broadcast, oneshot, watch
- **Synchronization**: `Mutex`, `RwLock`, `Semaphore`, `Notify`, `OnceCell`
- **Processes**: `moire::Command`
- **RPC**: request/response tracking (used by [Roam](https://github.com/bearcove/roam))

## Usage

### Live push mode

```rust
#[tokio::main]
async fn main() {
    moire::init!();

    moire::spawn_tracked!("connection_handler", async {
        // Task execution is instrumented
    });
}
```

Start the dashboard server:

```bash
cargo run -p moire-web
```

By default:
- TCP ingest: `127.0.0.1:9119` (`MOIRE_LISTEN`)
- HTTP UI: `127.0.0.1:9120` (`MOIRE_HTTP`)

Protocol debugging:
- `MOIRE_PROTOCOL_TRACE=1` on `moire-web` and/or instrumented processes logs every dashboard frame send/receive (size + JSON preview).
- On dashboard-protocol decode failures, clients now emit a terminal `client_error` frame with stage/error/last-frame preview before disconnecting.

Enable dashboard push in your app:

```toml
moire = { git = "https://github.com/bearcove/moire", branch = "main", features = ["diagnostics"] }
```

Run your app with:

```bash
MOIRE_DASHBOARD=127.0.0.1:9119 <your-binary>
```

`moire` is push-only: no file dump / SIGUSR1 ingestion mode.

## Examples

Runnable scenarios are implemented as subcommands in `crates/moire-examples`.

Current scenarios:
- `channel-full-stall` — bounded mpsc sender blocks on a full queue
- `mutex-lock-order-inversion` — two tasks deadlock by acquiring mutexes in opposite order
- `oneshot-sender-lost-in-map` — sender stored under wrong key, receiver waits forever
- `roam-rpc-stuck-request` — Rust Roam request stays pending forever
- `semaphore-starvation` — one task holds the only permit forever
- `roam-rust-swift-stuck-request` — Rust host + Swift peer, request intentionally never answered

Run it with:

```bash
just ex <example-name>
```

## Architecture

- `moire`: Main API — futures, locks, sync, live snapshot collection, optional dashboard push client
- `moire-types`: Shared types (graph nodes, snapshot requests/replies)
- `moire-web`: SQLite-backed ingest + query server and investigation UI

## Testing

Use `pnpm` for frontend workflows:

```bash
cd crates/moire-web/frontend
pnpm install
pnpm test
pnpm build
```

Run Rust checks from repo root:

```bash
cargo check --workspace --all-features
cargo nextest run --workspace --all-features
cargo clippy --workspace --all-features --all-targets
```

### Canonical Inspector Contract

Inspector path node attrs must use canonical keys only:

- `created_at` (required, epoch ns i64)
- `source` (required, non-empty string)
- `method` (optional)
- `correlation` (optional)

Legacy alias keys are rejected at the `moire-web` persistence boundary and covered by CI tests.

## Breaking Change: Canonical Attrs Only

Legacy alias keys were removed from emitted node attrs and inspector/timeline serializers.

- Removed aliases:
  - `request.*` / `response.*` field variants for method/status/timing/correlation
  - `ctx.location`
  - `correlation_key`, `request_id`, `trace_id`, `correlation_id`
  - `created_at_ns` as a canonical timestamp alias
- Required canonical fields for node attrs:
  - `created_at` (`i64`, Unix epoch ns)
  - `source` (non-empty string)
- Optional canonical cross-node fields:
  - `method`
  - `correlation`

Migration for downstream consumers:

1. Read `method` instead of `request.method`/`response.method`.
2. Read `correlation` instead of `correlation_key`/`request.id`/`request_id`.
3. Read `source` instead of `ctx.location`.
4. Treat `created_at` as the only canonical creation timestamp.

Example payloads:

```json
{"id":"request:01J...","kind":"request","process":"api","attrs":{"created_at":1700000000000000000,"source":"/srv/api/request.rs:42","method":"GetUser","correlation":"01J...","status":"in_flight"}}
```

```json
{"id":"tx:01J...","kind":"tx","process":"worker","attrs":{"created_at":1700000001000000000,"source":"/srv/queue.rs:88","channel_kind":"mpsc","age_ns":512000000,"queue_len":3}}
```

```json
{"id":"lock:01J...","kind":"lock","process":"worker","attrs":{"created_at":1700000002000000000,"source":"/srv/state.rs:17","lock_kind":"mutex","holder_count":1,"waiter_count":2}}
```
