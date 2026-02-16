+++
title = "Futures and tasks"
weight = 1
+++

## PeepableFuture

`PeepableFuture` wraps any future with diagnostic tracking. Use the `.peepable("label")` extension method (from the `PeepableFutureExt` trait) to wrap a future.

### What it tracks

- `pending_count` — number of polls that returned `Pending`
- `ready_count` — number of polls that returned `Ready`
- `total_pending` — aggregate time spent in `Pending` state
- `elapsed_ns` — total lifetime

### Edge behavior

- While pending, emits `needs` edges to whatever the wrapped future is waiting on.
- On each poll, pushes itself onto the causality stack — nested polls will get `touches` edges.
- On `Drop`, removes itself from the registry and cleans up all edges.

## Task spawning

`peeps::spawn_tracked(name, future)` spawns a Tokio task with peeps tracking. The task gets its own stack scope, so all work done inside is attributed to it.

## JoinSet

Wrapper around `tokio::task::JoinSet`. Tracks spawned tasks. Removed on `Drop`.
