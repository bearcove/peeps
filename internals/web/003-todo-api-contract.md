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
  "sql": "SELECT id, kind, process, attrs_json FROM nodes WHERE snapshot_id = ?1 LIMIT 200",
  "params": [1234]
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

Snapshot scoping rule:
- API must enforce snapshot scoping server-side.
- queries that do not constrain `snapshot_id` are rejected.

## UX contract implications

- no snapshot picker required in UI
- user flow: click `Jump to now`, then inspect that snapshot
- no auto-refresh

Canonical stuck-request query (for UI):

```sql
SELECT
  r.id,
  json_extract(r.attrs_json, '$.method') AS method,
  r.process,
  json_extract(r.attrs_json, '$.elapsed_ns') AS elapsed_ns,
  json_extract(r.attrs_json, '$.task_id') AS task_id
FROM nodes r
LEFT JOIN nodes resp
  ON resp.snapshot_id = r.snapshot_id
 AND resp.kind = 'response'
 AND resp.id = replace(r.id, 'request:', 'response:')
WHERE r.snapshot_id = ?1
  AND r.kind = 'request'
  AND CAST(json_extract(r.attrs_json, '$.elapsed_ns') AS INTEGER) >= ?2
  AND (resp.id IS NULL OR json_extract(resp.attrs_json, '$.status') = 'in_flight')
ORDER BY elapsed_ns DESC
LIMIT 500;
```

## Acceptance criteria

1. `jump-now` creates synchronized snapshots.
2. `sql` queries are read-only and bounded.
3. `snapshot_id` is explicit in every query/response.
