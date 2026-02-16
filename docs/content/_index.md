+++
title = "peeps"
insert_anchor_links = "heading"
+++

# peeps

A graph-first causality debugger for async Rust.

peeps captures the structure of your async program — tasks, resources, wakers, polls — as a live dependency graph. When something goes wrong, you don't grep through logs. You trace causality.

## Design stance

- **Explicit instrumentation.** No magic. You tell peeps what matters; it tracks exactly that.
- **No heuristics.** peeps doesn't guess at relationships. Every edge in the graph comes from instrumentation you placed.
- **Local-first.** Runs on your machine, talks to your process. No cloud, no SaaS, no telemetry pipeline.

## Sections

- [**Concepts**](/concepts/) — The mental model: nodes, edges, events, the graph
- [**Architecture**](/architecture/) — How peeps, peeps-types, peeps-web, and the instrumentation layer fit together
- [**Instrumentation**](/instrumentation/) — Every wrapped primitive and what it tracks
- [**Roam**](/roam/) — How roam integrates with peeps for RPC lifecycle tracking
- [**Extending**](/extending/) — Adding new instrumented primitives
