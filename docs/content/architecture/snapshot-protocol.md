+++
title = "Snapshot Protocol"
weight = 1
+++

## Pull snapshot protocol

peeps uses a pull model: the server asks every connected process for its current graph, then assembles one point-in-time snapshot.

### Why pull, not push

A push stream is great for telemetry, but weak for consistent cross-process causality. Pull gives "world at time T" semantics, which is what debugging needs.

## Protocol flow

On snapshot trigger:

1. Allocate a new `snapshot_id`
2. Broadcast request to all connected clients
3. Collect replies until timeout
4. Record non-responders/disconnects explicitly
5. Persist all received graph data under that snapshot ID

The result is complete or explicitly partial data, never silent omission.

## Operational knobs

- `PEEPS_DASHBOARD`: where clients send data.
- `PEEPS_LISTEN`: snapshot TCP listener.
- `PEEPS_HTTP`: HTTP API listener.

Exact wire framing and timeout constants are implementation details and may evolve.
