+++
title = "Lifecycles"
weight = 3
+++

Nodes are created, updated, and removed continuously. Exact per-primitive fields can change; lifecycle semantics should not.

## Core lifecycle invariants

- A node appears when its runtime entity becomes observable.
- A node is updated while state changes matter for causality.
- A node disappears when the entity is dropped or completed.
- Edge cleanup follows node cleanup, so stale relationships do not linger.

## What to expect during debugging

- Long-lived nodes usually represent resources.
- Short-lived nodes usually represent operations.
- Spiky `Needs` edges indicate transient contention.
- Persistent `Needs` chains indicate real blocking paths.

## Requests and responses

RPC request/response nodes are often intentionally short-lived in the live graph. They provide causal handoff points; follow downstream resource nodes for long-running investigations.

For exact retention behavior and attributes, consult the source for the specific wrapper.
