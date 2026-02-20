# Resources vs Await Graph

This document defines the split between:
- the **causal await graph** (what can block what), and
- the **resource context model** (where work runs and what it is attached to).

The goal is to keep deadlock/hang reasoning readable while still exposing rich operational context.

## Problem

If we render every implementation detail as first-class graph structure, the graph becomes unreadable.

Example: each RPC request can create multiple internal channel nodes (`oneshot_tx`, `oneshot_rx`, internal mpsc hops). Those are often useful for debugging internals, but they are not always useful for understanding the primary blocking path.

At the same time, users need resource-level answers:
- Which connection is unhealthy?
- Which process is overloaded?
- Which thread/task is stalled?
- What queue depth is rising?

## Core Model: Two Planes

### 1) Causal Plane (Await Graph)

This is the default graph.

Use it for:
- `needs` / blocker relationships
- deadlock suspects
- wait chains

Typical node kinds:
- `request`, `response`
- `future`
- `lock`
- selected channel/sync nodes that are direct blockers

Rule: show what is needed for causal reasoning. Avoid incidental transport mechanics by default.

### 2) Resource Plane (Context)

This is first-class in the data model, but secondary in default rendering.

Use it for:
- topology and ownership
- health and lifecycle
- throughput and queue metrics

Typical resource kinds:
- `process`
- `task` / `thread`
- `connection`
- optionally service/runtime resources

These are related via contextual edges (typically `touches`) and viewed in dedicated panels/tabs.

## Terminology

Everything is still a node in storage. The distinction is semantic:
- **Causal nodes**: participate directly in wait analysis.
- **Resource nodes**: provide context and diagnostics, not primary blocker flow.

## Visibility Strategy

Default UI behavior:
- Graph view prioritizes causal nodes and causal edges.
- Resource nodes are hidden or de-emphasized unless explicitly enabled.

Opt-in behavior:
- Toggle `Show resources` overlays process/task/connection context.
- Inspector always exposes resource attachments for selected causal nodes.

This keeps the graph legible while preserving drill-down power.

## Instrumentation Levels (`info`, `debug`, `trace`)

Levels should apply to both planes, but with different defaults.

Suggested policy:
- `info`: human-scale causal graph. Minimal internals.
- `debug`: include important internal blockers and queue resources.
- `trace`: full fidelity internals, including transport/channel implementation details.

Example:
- `request -> connection` can be visible at `info` or `debug`.
- `request -> oneshot_rx` and transport-internal hops can remain `trace` by default.

## Connection as a Resource

A `connection` resource should capture high-value state without forcing transport internals into the main graph.

Suggested connection attributes:
- identity: `connection_id`, `proc_key`, `peer_proc_key`, transport
- lifecycle: `state`, `opened_at_ns`, `closed_at_ns`, close reason
- liveness: `last_frame_sent_at_ns`, `last_frame_recv_at_ns`
- pressure: `driver_queue_len`, `pending_requests`, `pending_responses`
- errors: protocol violations, timeout counters, ring-full/slot pressure counters

Suggested relationships:
- `request --touches--> connection`
- `response --touches--> connection`
- optional `connection --touches--> transport resources` (doorbell, internal queues)

## Encapsulation Principle

Internal mechanisms (oneshots, per-call channels, scheduler glue) are implementation details.

Expose them when needed (`debug`/`trace`), but do not force them into the default narrative.

The default narrative should answer:
- what is stuck,
- what it waits on,
- which resource owns or routes that wait.

## UI Guidance

Recommended interaction model:
- **Graph tab**: causal reasoning first.
- **Resources tab**: process/task/connection inventory + health.
- **Inspector**: selected node shows related resources and key metrics.

When a node is selected, show “Related resources” immediately:
- process
- connection
- task/thread
- queue stats

This provides context without adding graph noise.

## Practical Rule of Thumb

When adding a new node/edge:
- If it is required to explain a wait chain, it belongs in the causal plane.
- If it explains ownership, topology, or health, it belongs in the resource plane.
- If uncertain, prefer resource-plane visibility first, then promote to causal only if required by diagnosis quality.

## Near-term Implementation Direction

1. Keep the current causal graph as the primary view.
2. Add first-class `connection` resources and attach requests/responses to them.
3. Keep low-level channel internals mostly at `trace`.
4. Use inspector/resources panels to surface queue depth, liveness, and error context.

This gives us both readability and depth: clean blocking paths plus actionable operational context.
