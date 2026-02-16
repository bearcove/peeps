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

Enable dashboard push in your app:

```toml
peeps = { git = "https://github.com/bearcove/peeps", branch = "main", features = ["dashboard"] }
```

Run your app with:

```bash
PEEPS_DASHBOARD=127.0.0.1:9119 <your-binary>
```

`peeps` is push-only: no file dump / SIGUSR1 ingestion mode.

## Examples

Runnable scenarios are available in `examples/`.

First scenario: `examples/channel-full-stall`

- Uses a bounded channel with capacity `16`
- Sends `16` items and then blocks on the next send (queue is full)
- Useful for exploring backpressure and stuck-send diagnostics in the UI

Run it with:

```bash
scripts/run-example channel-full-stall
```

## Architecture

- `peeps`: Main API — futures, locks, sync, live snapshot collection, optional dashboard push client
- `peeps-types`: Shared types (graph nodes, snapshot requests/replies)
- `peeps-web`: SQLite-backed ingest + query server and investigation UI
