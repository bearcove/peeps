+++
title = "Timing"
weight = 5
+++

Timing data exists to separate "busy" from "waiting" and to localize latency sources.

## Principles

- Prefer relative interpretation over absolute precision.
- Compare siblings (same kind, same code path) before comparing across categories.
- Use timing together with edges: time without causality is ambiguous.

## Practical use

1. Find a slow or stuck node.
2. Follow `Needs` edges to identify the blocker.
3. Check whether delay is queueing, contention, or external I/O.
4. Use source location metadata to jump to the emitting call site.

Field names may evolve; the intent is stable: expose enough timing and location context to explain latency and stalls.
