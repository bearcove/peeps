+++
title = "Timing"
weight = 5
+++

Nodes carry timing fields in their `attrs_json`. These are the key timing-related attributes by node type.

## Common

- `elapsed_ns` — nanoseconds from node creation to completion/removal. Present on most node types.

## Futures

- `pending_count` — how many times polled and returned `Pending`.
- `ready_count` — how many times polled and returned `Ready`.
- `total_pending` — aggregate duration spent in `Pending` state.

A future that has been polled 50 times with `pending_count: 49` and `ready_count: 1` was polled many times before completing. A future with high `total_pending` relative to `elapsed_ns` spent most of its life waiting.

## Channels

- `high_watermark` — peak number of items in the channel buffer. Useful for identifying backpressure and sizing issues.

## RPC

- `response.created_at_ns` — when the response node was created (server side).
- `response.handled_elapsed_ns` — time from creation to handler completion.
- `response.queued_at_ns` — when the response was queued for delivery.

Derived metrics:
- **Queue wait** = `queued_at_ns - created_at_ns` — time spent waiting before the handler ran.
- **Service time** = `handled_elapsed_ns` — how long the handler took.

## Commands

- `elapsed_ns` on spawn, wait, status, output operations.
- `pid`, `exit_code`, `exit_signal` — process metadata.
- `program`, `args` — what was executed.

## Location metadata

Present on all nodes, under the `ctx` key in `attrs_json`:

| Field | Example |
|-------|---------|
| `ctx.location` | `src/main.rs:42` |
| `ctx.module_path` | `my_crate::server::handler` |
| `ctx.file` | `src/main.rs` |
| `ctx.line` | `42` |
| `ctx.crate_name` | `my_crate` |
| `ctx.crate_version` | `0.1.0` |

This lets you trace any node back to the exact source location that created it.
