# Peeps examples

These examples live in-repo but outside the workspace crates so they are easy to run and tweak.

## Runner

Use the helper launcher to run dashboard backend + frontend + any example in one command:

```bash
scripts/run-example --list
scripts/run-example channel-full-stall
```

When you stop the runner (`Ctrl+C`), all child processes are stopped too.

## 1) Channel full stall (`tokio::sync::mpsc` behavior)

Path: `examples/channel-full-stall`

What it does:
- Creates a bounded channel with capacity `16`
- Sends `16` items to fill it
- Attempts a 17th send, which blocks forever because the receiver is intentionally stalled

### Run it

Terminal 1:

```bash
cargo run -p peeps-web
```

Terminal 2:

```bash
pnpm --dir crates/peeps-web/frontend dev
```

Terminal 3:

```bash
PEEPS_DASHBOARD=127.0.0.1:9119 \
  cargo run --manifest-path examples/channel-full-stall/Cargo.toml
```

Then open [http://127.0.0.1:9131](http://127.0.0.1:9131) and inspect the `demo.work_queue` channel nodes plus the `queue.send.blocked` task.

## 2) Roam RPC stuck request

Path: `examples/roam-rpc-stuck-request`

What it does:
- Starts an in-memory Roam client/server connection
- Sends one RPC request (`sleepy_forever`)
- Handler records a response node and then sleeps forever
- Caller remains blocked waiting for a response that never arrives

### Run it

Single-command runner:

```bash
scripts/run-example roam-rpc-stuck-request
```

Manual mode:

Terminal 1:

```bash
cargo run -p peeps-web
```

Terminal 2:

```bash
pnpm --dir crates/peeps-web/frontend dev
```

Terminal 3:

```bash
PEEPS_DASHBOARD=127.0.0.1:9119 \
  cargo run --manifest-path examples/roam-rpc-stuck-request/Cargo.toml
```

## 3) Semaphore starvation (`tokio::sync::Semaphore`)

Path: `examples/semaphore-starvation`

What it does:
- Creates a semaphore with one permit
- `permit_holder` acquires and keeps that permit forever
- `permit_waiter` blocks forever trying to acquire a permit

### Run it

```bash
scripts/run-example semaphore-starvation
```

## 4) Roam Rust↔Swift stuck request

Path: `examples/roam-rust-swift-stuck-request`

What it does:
- Rust host starts a TCP listener and spawns a Swift roam-runtime peer (`swift run`)
- Rust accepts handshake and issues one raw RPC call
- Swift intentionally never answers that request, so Rust stays blocked waiting

### Run it

```bash
scripts/run-example roam-rust-swift-stuck-request
```

Requirements:
- Swift toolchain (`swift`) installed locally
- Local `../roam/swift/roam-runtime` checkout available (used as a path dependency by the peer package)
