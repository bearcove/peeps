+++
title = "peeps"
insert_anchor_links = "heading"
+++

# peeps

A graph-first causality debugger for async Rust.

peeps captures the structure of your async program as a live dependency graph. When something goes wrong, you trace causality instead of reconstructing it from logs.

## Design stance

- **Explicit instrumentation.** No magic. You tell peeps what matters; it tracks exactly that.
- **No heuristics.** peeps doesn't guess at relationships. Every edge in the graph comes from instrumentation you placed.
- **Local-first.** Runs on your machine, talks to your process. No cloud, no SaaS, no telemetry pipeline.
- **Concept-first docs.** These pages describe intent and invariants. Code-level fields and variant lists are intentionally left to source.

## Sections

- [**Concepts**](/concepts/) — The mental model: nodes, edges, events, the graph
- [**Architecture**](/architecture/) — How peeps, peeps-types, peeps-web, and the instrumentation layer fit together
- [**Instrumentation**](/instrumentation/) — How wrappers surface causality for Tokio and Roam primitives
