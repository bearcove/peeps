# Peeps Web Rebuild Overview

Status: todo
Owner: wg-peeps-web

## Objective

Build `peeps-web` around a canonical local graph model in SQLite:
- `nodes(snapshot_id, id, kind, process, attrs_json)`
- `edges(snapshot_id, src_id, dst_id, kind, attrs_json)` where `kind = needs`

Snapshots are synchronized pulls (`Jump to now`), not free-running per-process pushes.

## Non-negotiables

1. Single graph edge kind: `needs`.
2. No inferred/derived/heuristic edges.
3. Process is context, not a node.
4. Threads are out of graph scope.
5. Frontend is Requests-first (single tab).
6. Theme follows OS via CSS `light-dark()`.

## Workstreams

0. `000-todo-crate-split-for-parallelization.md`
1. `001-todo-storage-and-ingest.md`
2. `002-todo-node-edge-projection.md`
3. `003-todo-api-contract.md`
4. `004-todo-frontend-investigate-mvp.md`
5. `005-todo-correctness-local.md`
6. `006-todo-wrapper-emission-api.md`
7. `007-todo-resource-type-workstreams.md`

## Execution order

- run `000` first
- run `001/002/003` in parallel
- run `004` after `003` endpoint signatures are stubbed (mock data allowed meanwhile)
- run `005` continuously as correctness gate
- run `006` first to freeze canonical IDs/types/contracts
- then run `007` tracks in parallel by resource area

## Definition of done

1. `Jump to now` produces synchronized `snapshot_id` snapshots.
2. Nodes and `needs` edges are populated from explicit instrumentation only.
3. Requests tab can identify stuck requests and traverse dependency graph.
4. UI remains focused (no kitchen sink tabs).

## Initial product slice

1. One top-level tab: `Requests`.
2. Stuck request table first.
3. ELK graph prototype allowed (mock-first).
4. Side inspector + hover cards.
