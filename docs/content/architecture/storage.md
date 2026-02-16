+++
title = "Storage"
weight = 3
+++

peeps-web stores snapshot data in local SQLite for portability and inspectability.

## Design goals

- Keep full snapshots addressable by ID.
- Support fast read-mostly investigation queries.
- Preserve process-level ingest outcomes (responded, timeout, disconnected).
- Retain enough history for comparisons while bounding disk growth.

## Data shape (broad strokes)

- Snapshot metadata.
- Per-process snapshot status.
- Node rows.
- Edge rows.
- Unresolved cross-process edge placeholders.
- Ingest event log for diagnostics.

## Query model

Frontend queries are scoped to one snapshot at a time through read-only views, so investigations stay deterministic and isolated.

## Retention

Retention is bounded. Old snapshots and stale ingest logs are pruned to keep the database operationally lightweight.

For exact schema and index definitions, use the migrations/source of `peeps-web`.
