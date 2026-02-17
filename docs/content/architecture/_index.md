+++
title = "Architecture"
weight = 2
sort_by = "weight"
insert_anchor_links = "heading"
+++

peeps has three crates. `peeps` is the instrumentation library linked into your application. `peeps-types` defines the shared data model. `peeps-web` is the server and web frontend. All instrumentation is feature-gated behind `diagnostics` — when the feature is off, every wrapper compiles to a zero-cost pass-through.

The server is intentionally dumb. It collects snapshots, stores them, and exposes a constrained SQL surface. Most exploration logic lives in the client, on purpose, so we can iterate quickly and keep investigation workflows flexible.

Snapshot capture is pull-based, not push-based. When you trigger a snapshot, the server asks connected processes for their current graph, waits until timeout, and stores a complete-or-explicitly-partial result under one snapshot ID. That gives us "world at time T" semantics, which is what cross-process debugging actually needs.

Storage is local SQLite. We keep snapshot metadata, process-level ingest outcomes, nodes, edges, and related ingest diagnostics. Queries are read-mostly and scoped to snapshot IDs so investigations are deterministic instead of drifting with live state.

For exact payload contracts and canonical fields, see [Schema](/architecture/schema/).
