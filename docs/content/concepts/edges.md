+++
title = "Edges"
weight = 2
+++

Edges encode relationships between nodes. There are four edge kinds, each with strict semantics.

## `Needs`

Current progress dependency. `src` is blocked waiting on `dst`.

Added when a resource is awaited — channel recv, lock acquire, future poll. Removed when the blocking condition resolves (lock acquired, message received, future ready).

This is the **wait graph**. If you see a `Needs` edge, the source cannot make progress until the destination does something. A chain of `Needs` edges tells you exactly what's blocking what.

## `Touches`

Observed interaction history. `src` interacted with `dst` at least once.

Added automatically by the [causality stack](@/concepts/stack.md): when a parent polls a child, a `Touches` edge is emitted from parent to child. These accumulate and are retained until either endpoint node is removed.

`Touches` edges record "who looked at what." They're useful for understanding dataflow — which futures polled which resources, which handler touched which channels.

## `Spawned`

Lineage. `src` spawned `dst`.

Permanent — retained for the entire lifetime of the child node. Tells you where things came from. A `JoinSet` will have `Spawned` edges to every task it launched.

## `ClosedBy`

Causal closure. `src` was closed because of `dst`.

Records why something ended. For example, a channel `Rx` might have a `ClosedBy` edge pointing to the last `Tx` that was dropped, explaining why the receive side closed.
