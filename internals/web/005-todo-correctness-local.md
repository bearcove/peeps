# Correctness (Local Dev)

Status: todo
Owner: wg-quality
Scope: local correctness checks only

## Goal

Verify canonical graph correctness on a single local machine.
No perf benchmarking. No rollout planning.

## Checks

1. Snapshot synchronization
- `Jump to now` creates one `snapshot_id`.
- connected processes replying to that pull are recorded under same `snapshot_id`.
- missing responders are explicit in `snapshot_processes`.

2. Write integrity
- per-process reply writes are transactional.
- failed ingest does not leave partial rows for that reply.

3. Node/edge integrity
- every edge source and destination exists as a node in same snapshot.
- node IDs follow conventions.
 - exception: explicitly unresolved cross-process references may be represented via query-time left joins (must not crash analysis).

4. Edge model integrity
- all persisted edges have `kind = 'needs'`.
- no inferred/derived/heuristic edges in storage.

5. Track completeness (for finished 007 tracks)
- required node kinds appear.
- required node attrs exist.
- required dependency patterns appear.

## Quick validation SQL

```sql
-- Missing endpoints
SELECT e.src_id, e.dst_id
FROM edges e
LEFT JOIN nodes ns ON ns.snapshot_id = e.snapshot_id AND ns.id = e.src_id
LEFT JOIN nodes nd ON nd.snapshot_id = e.snapshot_id AND nd.id = e.dst_id
WHERE e.snapshot_id = ?1 AND (ns.id IS NULL OR nd.id IS NULL)
LIMIT 50;
```

```sql
-- Non-needs edges must be zero
SELECT kind, COUNT(*)
FROM edges
WHERE snapshot_id = ?1
GROUP BY kind
HAVING kind <> 'needs';
```

```sql
-- Node kind coverage
SELECT kind, COUNT(*)
FROM nodes
WHERE snapshot_id = ?1
GROUP BY kind
ORDER BY COUNT(*) DESC;
```

## Acceptance criteria

1. Local stuck-request workflow is reliable after `Jump to now`.
2. Edge/node integrity checks pass on live local runs.
3. Edge table contains only `needs` edges.
