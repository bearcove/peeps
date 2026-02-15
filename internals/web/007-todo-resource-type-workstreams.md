# Resource-Type Workstreams Index

Status: todo
Owner: wg-resources
Scope: wrapper crates (`peeps-tasks`, `peeps-locks`, `peeps-sync`) + producers (`peeps`, roam diagnostics) + storage consumer (`peeps-web`)

## Why this exists

`peeps-web` is moving to one canonical graph model (nodes + edges). To avoid another kitchen-sink dashboard, we first need high-quality, explicit data for every resource type.

This directory splits work by resource type so multiple agents can work in parallel without stepping on each other.

Prerequisites:
- complete `/Users/amos/bearcove/peeps/internals/web/000-todo-crate-split-for-parallelization.md` first.
- complete `/Users/amos/bearcove/peeps/internals/web/006-todo-wrapper-emission-api.md` contract freeze before parallel `007-*` implementation.

## Hard invariants (apply to every track)

1. Only explicit measured edges. No inferred/derived/heuristic edges.
2. Stable IDs. Same real-world resource maps to same ID pattern.
3. Required attrs present. Missing required attrs means incomplete track.
4. Wrapper-first. Emit in wrappers; keep consumer changes minimal.
5. Consumer changes only when wrappers are not transparent enough.

## How to execute a track

1. Read your resource file (`007-todo-resource-XX-*.md`).
2. Implement wrapper-side emission into canonical graph API (`peeps-types` graph structs).
3. Make required consumer migrations (if listed).
4. Add validation SQL queries and run against live data.
5. Update file status to `done` and include short implementation notes.

## Track list

1. `007-todo-resource-02-tasks.md`
2. `007-todo-resource-03-futures.md`
3. `007-todo-resource-05-sync-locks.md`
4. `007-todo-resource-06-tokio-channels.md`
5. `007-todo-resource-09-semaphore.md`
6. `007-todo-resource-10-oncecell.md`
7. `007-todo-resource-11-rpc-requests.md`
8. `007-todo-resource-12-roam-channels.md`

## Coordination note

This is data-model-first. UI remains stub-only until these tracks are mostly complete.
