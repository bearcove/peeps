# 002 - Edge Ingestion + Normalization

## Goal

Convert raw snapshots into a normalized wait graph each refresh tick.

## Scope

- Ingest from:
  - task snapshots
  - lock snapshots
  - sync/channel snapshots
  - RPC/session snapshots
  - wake edges (`task -> task`, `task -> future`)
  - future waits metadata (`future_id`, creator, poller)
- Produce:
  - normalized node map
  - normalized edge list
  - per-edge freshness and confidence

## Deliverables

- `build_wait_graph(snapshot_bundle)` implementation.
- Resource ownership edge extraction (`resource -> owner task`).
- Wait edge extraction (`task -> awaited resource`).
- Cross-process stitching via RPC context (`chain_id`/`span_id`).

## Acceptance

- Graph contains all currently instrumented resource types.
- Missing data is represented explicitly (`unknown`) instead of dropped silently.

## Notes

- Prefer deterministic ordering for stable diffs/debugging.

