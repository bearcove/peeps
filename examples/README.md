# Moire examples

Scenarios are implemented in `crates/moire-examples/src/scenarios`.

## Runner

Use the helper launcher to run moire-web (`--dev`) and one scenario in one command:

```bash
just ex channel-full-stall
```

When you stop the runner (`Ctrl+C`), all child processes are stopped too.

## 1) Oneshot sender lost in map (`tokio::sync::oneshot`)

Path: `crates/moire-examples/src/scenarios/oneshot_sender_lost_in_map.rs`

What it does:
- Creates a request/response oneshot pair
- Stores the sender in a pending map under the wrong key
- Receives a response but misses lookup, so the sender stays alive in the map and the receiver waits forever

### Run it

```bash
just ex oneshot-sender-lost-in-map
```

Then open [http://127.0.0.1:9131](http://127.0.0.1:9131) and inspect `demo.request_42.response`, `response_bus.recv`, and `request_42.await_response.blocked`.

## 2) Mutex lock-order inversion (`tokio` tasks + blocking mutex)

Path: `crates/moire-examples/src/scenarios/mutex_lock_order_inversion.rs`

What it does:
- Creates two shared mutexes (`demo.shared.left`, `demo.shared.right`)
- Starts two tracked Tokio tasks that intentionally acquire those mutexes in opposite order
- Uses a Tokio barrier so both tasks hold one lock before attempting the second lock, making the deadlock deterministic
- Exposes async symptoms with tracked observer tasks waiting forever on completion signals

### Run it

```bash
just ex mutex-lock-order-inversion
```

## 3) Channel full stall (`tokio::sync::mpsc` behavior)

Path: `crates/moire-examples/src/scenarios/channel_full_stall.rs`

What it does:
- Creates a bounded channel with capacity `16`
- Sends `16` items to fill it
- Attempts a 17th send, which blocks forever because the receiver is intentionally stalled

### Run it

```bash
just ex channel-full-stall
```

## 4) Roam RPC stuck request

Path: `crates/moire-examples/src/scenarios/roam_rpc_stuck_request.rs`

What it does:
- Starts an in-memory Roam client/server connection
- Sends one RPC request (`sleepy_forever`)
- Handler records a response node and then sleeps forever
- Caller remains blocked waiting for a response that never arrives

### Run it

Single-command runner:

```bash
just ex roam-rpc-stuck-request
```

## 5) Semaphore starvation (`tokio::sync::Semaphore`)

Path: `crates/moire-examples/src/scenarios/semaphore_starvation.rs`

What it does:
- Creates a semaphore with one permit
- `permit_holder` acquires and keeps that permit forever
- `permit_waiter` blocks forever trying to acquire a permit

### Run it

```bash
just ex semaphore-starvation
```

## 6) Roam Rust↔Swift stuck request

Path: `crates/moire-examples/src/scenarios/roam_rust_swift_stuck_request.rs`

What it does:
- Rust host starts a TCP listener and spawns a Swift roam-runtime peer (`swift run`)
- Rust accepts handshake and issues one raw RPC call
- Swift intentionally never answers that request, so Rust stays blocked waiting

### Run it

```bash
just ex roam-rust-swift-stuck-request
```

Requirements:
- Swift toolchain (`swift`) installed locally
- Local `../roam/swift/roam-runtime` checkout available
- Swift package files live in `crates/moire-examples/swift/roam-rust-swift-stuck-request`

## Example Conventions


This is the contract for examples in this repo.

The goal is simple: `just ex` should be enough to reproduce a scenario, inspect it, and stop cleanly without leaving stray processes behind.

## User Flow

The intended loop is:

1. Run `just ex`.
2. Pick an example from the selector.
3. Watch it boot everything required.
4. Inspect in the UI.
5. Stop with `Ctrl+C`.

No second terminal should be required for normal use.

## Runtime Contract

Every example must satisfy this:

- `cargo run --bin moire-examples -- <subcommand>` runs the full scenario.
- If the scenario needs multiple roles (client/server, caller/callee, etc.), the scenario module itself is responsible for launching and coordinating them.
- Example startup should fail fast with a clear error if one required role cannot start.
- Example shutdown should terminate all child work it started.

The runner may set environment (`MOIRE_DASHBOARD`, ports), but it should not contain scenario-specific orchestration logic.

## Process-Group Contract

`moire-examples` (`cargo run --bin moire-examples`, used by `just ex`) is responsible for top-level lifecycle:

- It starts `moire-web` in one process group.
- It runs the chosen scenario in-process.
- On exit or interrupt, it tears down the `moire-web` process group and returns.

This is required to avoid zombie/orphaned children when examples spawn subprocesses.

## Authoring Rules For Multi-Process Examples

When an example needs multiple processes:

- Keep orchestration inside the scenario module.
- Prefer explicit supervised child handles over detached subprocesses.
- Propagate cancellation and wait for child exit paths.
- Treat partial startup as an error; tear down anything already started.

If a scenario cannot be expressed as a single `moire-examples` subcommand, it does not meet this repo's examples contract yet.
