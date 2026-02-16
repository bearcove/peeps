+++
title = "Lifecycles"
weight = 3
+++

Nodes are created, updated, and removed as your program runs. Each node kind has its own lifecycle.

## Futures

Removed from the registry on `Drop`, along with all their edges. While alive, they emit `Needs` edges for whatever they're awaiting and `Touches` edges for stack interactions (see [Causality Stack](@/concepts/stack.md)).

## Locks

Exist as long as the lock wrapper is alive. Track holders and waiters. `Needs` edges are emitted while waiting to acquire and removed once acquired.

## Channels

`Tx` and `Rx` nodes exist independently. They track sent/received counts, high watermark, sender/receiver counts, closed state, and waiter counts. A `ClosedBy` edge may be emitted when all senders or receivers are dropped.

## Timers

`Sleep`, `Interval`, `Timeout` — exist for their duration. `Interval` tracks tick count and elapsed time.

## JoinSet

Removed on `Drop`. Has `Spawned` edges to every task it launched. Tracks the number of spawned tasks.

## Commands

Registered on spawn, updated on exit, removed on `Drop`. Track `pid`, `exit_code`, `exit_signal`, `program`, `args`, and `elapsed_ns`.

## File and network operations

`FileOp`, `NetConnect`, `NetAccept`, `NetReadable`, `NetWritable` — exist for the duration of the operation. Track bytes transferred, elapsed time, and result.

## RPC Requests

Removed on call completion by default. Set `PEEPS_KEEP_COMPLETED_RPC=1` to retain them after completion.

## RPC Responses

Persist with final state (unlike requests). This asymmetry is intentional — responses represent the server's view and are useful for understanding processing time and handler behavior.

## Edge cleanup

| Edge kind | Removed when... |
|-----------|----------------|
| `Needs` | The blocking condition resolves (lock acquired, channel received, etc.) |
| `Touches` | Either endpoint node is removed. |
| `Spawned` | The child node is removed. |
| `ClosedBy` | Either endpoint node is removed. |
