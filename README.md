# peeps

Low-overhead instrumentation for production Rust systems.

## Features

**Task Tracking**: Wraps `tokio::spawn` to record task names, poll timing, and execution backtraces. Each poll captures stack information when diagnostics are enabled.

**Thread Sampling**: SIGPROF-based periodic sampling identifies threads stuck in blocking operations or tight loops. Samples are aggregated to show dominant stack frames.

**Lock Instrumentation**: Tracks `parking_lot` mutex/rwlock contention when the `locks` feature is enabled.

**Roam + SHM Diagnostics**: Records connection state and shared-memory diagnostics from registered providers.

**Live Dashboard Push**: Instrumented processes push structured JSON snapshots to a `peeps` dashboard server over TCP. The web UI and API read from this live stream.

## Usage

### Live push mode

```rust
use peeps::tasks;

#[tokio::main]
async fn main() {
    peeps::init_named("my-service");

    tasks::spawn_tracked("connection_handler", async {
        // Task execution is instrumented
    });
}
```

Start the dashboard server:

```bash
cargo run -p peeps-web
```

By default:
- TCP ingest: `127.0.0.1:9119` (`PEEPS_LISTEN`)
- HTTP UI: `127.0.0.1:9120` (`PEEPS_HTTP`)

Protocol debugging:
- `PEEPS_PROTOCOL_TRACE=1` on `peeps-web` and/or instrumented processes logs every dashboard frame send/receive (size + JSON preview).
- On dashboard-protocol decode failures, clients now emit a terminal `client_error` frame with stage/error/last-frame preview before disconnecting.

Enable dashboard push in your app:

```toml
peeps = { git = "https://github.com/bearcove/peeps", branch = "main", features = ["diagnostics"] }
```

Run your app with:

```bash
PEEPS_DASHBOARD=127.0.0.1:9119 <your-binary>
```

`peeps` is push-only: no file dump / SIGUSR1 ingestion mode.

## Examples

Runnable scenarios are available in `examples/`.

Current scenarios:
- `channel-full-stall` — bounded mpsc sender blocks on a full queue
- `roam-rpc-stuck-request` — Rust Roam request stays pending forever
- `semaphore-starvation` — one task holds the only permit forever
- `roam-rust-swift-stuck-request` — Rust host + Swift peer, request intentionally never answered

Run it with:

```bash
scripts/run-example --list
scripts/run-example <example-name>
```

## Architecture

- `peeps`: Main API — futures, locks, sync, live snapshot collection, optional dashboard push client
- `peeps-types`: Shared types (graph nodes, snapshot requests/replies)
- `peeps-web`: SQLite-backed ingest + query server and investigation UI

## Testing

Use `pnpm` for frontend workflows:

```bash
cd crates/peeps-web/frontend
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

Legacy alias keys are rejected at the `peeps-web` persistence boundary and covered by CI tests.

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
