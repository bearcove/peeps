+++
title = "Schema"
weight = 2
insert_anchor_links = "heading"
+++

The schema source of truth is the Rust model in `crates/peeps-types/src/new_model.rs`.

This page is intentionally thin and points to those types directly.

For HTTP endpoints and current SQLite ingestion tables, see [API](/architecture/api/).

## Canonical Types

1. `Snapshot`: point-in-time graph payload
2. `Entity`: runtime object tracked over time
3. `Scope`: execution container (`process`, `thread`, `task`)
4. `Edge`: causal relationship (`entity -> entity`)
5. `Event`: point-in-time record targeting either an entity or a scope

## Snapshot Shape

`Snapshot` now contains only graph data:

1. `entities: Vec<Entity>`
2. `scopes: Vec<Scope>`
3. `edges: Vec<Edge>`
4. `events: Vec<Event>`

`process_name` and `proc_key` are not part of the snapshot payload. Process identity is established in protocol handshake/session context.

## Identity and References

1. Entity identity uses `EntityId`.
2. Scope identity uses `ScopeId`.
3. Causal edges reference entities only.
4. Events use `EventTarget` (`Entity` or `Scope`).

## State Modeling Rules

Canonical model rules (as reflected in `new_model.rs`):

1. Prefer enums over booleans, including binary concepts.
2. Use multiple enums for independent axes.
3. Keep counts numeric.
4. Do not store trivially derived state.
5. Put transitions/details in events; keep snapshot fields for current facts.

## SQL Translation (Current Direction)

1. `entities` table for `Entity`
2. `scopes` table for `Scope`
3. `edges` table for causal entity links
4. `events` table for append-only records with typed target (`entity`/`scope`)
