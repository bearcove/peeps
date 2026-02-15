# API Contract Spec

Status: todo
Owner: wg-api
Scope: `crates/peeps-web` HTTP API

## Goal

Support manual, synchronized, local investigation.

## Endpoints (v1)

- `POST /api/jump-now`
  - triggers synchronized snapshot pull
  - returns:
    - `snapshot_id`
    - requested/responded/timed_out counts

- `POST /api/sql`
  - executes read-only SQL against selected snapshot
  - request:
```json
{
  "snapshot_id": 1234,
  "sql": "SELECT id, kind, process, attrs_json FROM nodes LIMIT 200",
  "params": []
}
```
  - response:
```json
{
  "snapshot_id": 1234,
  "columns": ["id", "kind", "process", "attrs_json"],
  "rows": [],
  "row_count": 0,
  "truncated": false
}
```

## SQL policy

Allow only read-only statements:
- `SELECT`
- `WITH`
- `EXPLAIN QUERY PLAN`

Reject mutations and dangerous operations:
- `INSERT`, `UPDATE`, `DELETE`, `ALTER`, `DROP`, `ATTACH`, `DETACH`, `PRAGMA`
- multiple statements

Enforcement requirements (not optional):
- prepare exactly one statement; reject trailing SQL.
- enforce read-only via SQLite authorizer callback.
- enforce max execution time via progress handler / interrupt.
- enforce hard result caps:
  - max rows: 5000
  - max response bytes: 4 MiB
  - max execution time: 750 ms
- set `truncated=true` when row/byte cap is hit.

Snapshot scoping enforcement (mechanical, server-side):
- open a per-request SQLite connection.
- create TEMP VIEWs scoped to `snapshot_id`:
  - `nodes AS SELECT * FROM main.nodes WHERE snapshot_id = :snapshot_id`
  - `edges AS SELECT * FROM main.edges WHERE snapshot_id = :snapshot_id`
  - `unresolved_edges AS SELECT * FROM main.unresolved_edges WHERE snapshot_id = :snapshot_id`
  - `snapshot_processes AS SELECT * FROM main.snapshot_processes WHERE snapshot_id = :snapshot_id`
- authorizer must reject direct reads of `main.nodes`, `main.edges`, `main.unresolved_edges`, `main.snapshot_processes`; only scoped TEMP VIEWs are allowed.

## UX contract implications

- no snapshot picker required in UI
- user flow: click `Jump to now`, then inspect that snapshot
- no auto-refresh

Canonical stuck-request query (for UI, run against scoped views):

```sql
SELECT
  r.id,
  json_extract(r.attrs_json, '$.method') AS method,
  r.process,
  CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) AS elapsed_ns,
  json_extract(r.attrs_json, '$.task_id') AS task_id,
  json_extract(r.attrs_json, '$.correlation_key') AS correlation_key
FROM nodes r
LEFT JOIN nodes resp
  ON resp.kind = 'response'
 AND json_extract(resp.attrs_json, '$.correlation_key') = json_extract(r.attrs_json, '$.correlation_key')
WHERE r.kind = 'request'
  AND CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) >= ?1
  AND (resp.id IS NULL OR json_extract(resp.attrs_json, '$.status') = 'in_flight')
ORDER BY CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) DESC
LIMIT 500;
```

## Acceptance criteria

1. `jump-now` creates synchronized snapshots.
2. `sql` queries are read-only and bounded.
3. `snapshot_id` is explicit in every query/response.
4. SQL always runs against server-enforced scoped views.
