+++
title = "Edges"
weight = 2
+++

Edges are the core explanatory mechanism in peeps. They answer "why is this happening?"

## `Needs` (progress dependency)

`src` cannot make progress until `dst` changes state. This is the wait graph and the fastest path to "what is blocking what."

## `Touches` (observed interaction)

`src` interacted with `dst` at least once. These edges preserve context and help reconstruct data/control flow, even when nothing is currently blocked.

## `Spawned` (lineage)

`src` created `dst`. Use this to answer "where did this task/resource come from?"

## `ClosedBy` (termination cause)

`src` ended because `dst` ended or was dropped. This captures causal shutdown and closure chains.

## Reading tip

Start with `Needs` for active stalls, then add `Touches` and `Spawned` for narrative context. `ClosedBy` explains why things stopped.
