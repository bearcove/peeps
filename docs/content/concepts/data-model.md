+++
title = "Data Model"
weight = 1
+++

peeps models runtime behavior as a directed graph plus a timeline: **nodes** are runtime entities, **edges** are causal relationships, and **events** are timestamped facts about lifecycle and activity.

## Node contract

A node is "something that exists at runtime and can participate in causality" (task, resource, RPC unit, system operation).

Stable guarantees:

- Every node has a globally unique ID.
- Every node has a kind so the UI can group similar entities.
- Every node has a mandatory `created_at` timestamp in `attrs_json` (`i64`, Unix epoch nanoseconds).
- Inspector-facing node attrs use canonical keys only:
  - `created_at` (required)
  - `source` (required)
  - `method` (optional)
  - `correlation` (optional)
- Legacy alias keys are not part of the contract and are rejected at ingest.
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
