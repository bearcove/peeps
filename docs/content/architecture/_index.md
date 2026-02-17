+++
title = "Architecture"
weight = 2
sort_by = "weight"
insert_anchor_links = "heading"
+++

peeps has three crates. `peeps` is the instrumentation library linked into your application. `peeps-types` defines the shared data model. `peeps-web` is the server and web frontend. All instrumentation is feature-gated behind `diagnostics` — when the feature is off, every wrapper compiles to a zero-cost pass-through.

The practical problem is still the same: your async system is stuck, every local stack looks "fine", and the real blocker lives in a causal chain across tasks or processes. peeps is built around that reality, not around pretty graph theory.

Snapshot capture is pull-based, not push-based. When you trigger a snapshot, the server asks connected processes for their current graph, waits until timeout, and stores a complete-or-explicitly-partial result under one snapshot ID. That gives us "world at time T" semantics, which is what cross-process debugging actually needs.

The server is intentionally dumb. It collects snapshots, stores them, and exposes a constrained SQL surface. Most exploration logic lives in the client on purpose, so we can iterate quickly and keep investigation workflows flexible.

## Model boundaries

The model separates four concerns:

1. `Entity`: long-lived runtime things you can block on (locks, channel endpoints, requests, sockets, etc.).
2. `Scope`: execution containers (process, thread, task).
3. `Edge`: causal dependency between entities.
4. `Event`: point-in-time record attached to an entity or a scope.

The important boundary is that causal edges are `entity -> entity`. Scope is context, not causality.

## Fields vs events

`Entity.body` and `Scope.body` are canonical snapshot facts. Events are transitions and per-occurrence detail.

Use this rule:

1. Put it in a body field if it describes current state at snapshot time and is needed for filtering/highlighting.
2. Put it in an event if it is an occurrence a user may inspect in time order.
3. Do not store both a fact and a trivially derived status from that fact.

Example: if you already have waiter/holder counts, `contended` is derived and should not be stored as a separate state field.

## Enums, booleans, and redundancy

1. Use enums for finite, mutually-exclusive states.
2. Do not use booleans in persisted model types. Even binary concepts should be named enums.
3. Use multiple enums when there are multiple independent axes.
4. Prefer one source of truth. If `close_cause` fully implies closure, avoid a redundant `closed` flag.

This keeps payloads small and avoids contradictory state.

## SQLite shape

Storage is local SQLite and snapshot-scoped for deterministic replay. The core shape is:

1. `entities`: snapshot facts for entities.
2. `scopes`: snapshot facts for scopes.
3. `edges`: causal entity-to-entity links.
4. `events`: append-only records with `target_type` (`entity` or `scope`) and `target_id`.

Queries are read-mostly and always keyed by snapshot ID so investigations do not drift with live runtime changes.

## Where derivation lives

Instrumentation should emit canonical facts and non-derivable lifecycle states. Shared derived analysis (for example deadlock-risk heuristics) belongs in backend query logic so all clients get the same answer. UI-only presentation derivations can stay in the frontend.

For exact payload contracts and canonical fields, see [Schema](/architecture/schema/).
