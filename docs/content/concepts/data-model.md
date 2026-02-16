+++
title = "Data Model"
weight = 1
+++

peeps models runtime behavior as a directed graph with two parts: **nodes** and **edges**.

## Node contract

A node is "something that exists at runtime and can participate in causality" (task, resource, RPC unit, system operation).

Stable guarantees:

- Every node has a globally unique ID.
- Every node has a kind so the UI can group similar entities.
- Optional metadata can be attached for debugging context.
- Metadata shape is intentionally flexible and may evolve.

Treat IDs as stable identity and metadata as observational detail.

## Edge contract

An edge says "this node has a causal relationship to that node."

Stable guarantees:

- Edges are directional.
- Edge kind communicates meaning (`Needs`, `Touches`, `Spawned`, `ClosedBy`).
- Edges appear and disappear with runtime state transitions.

Semantics of each edge kind are described in [Edges](@/concepts/edges.md).
