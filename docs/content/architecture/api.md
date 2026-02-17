+++
title = "API"
weight = 3
insert_anchor_links = "heading"
+++

This page is the contract the frontend can build against right now.

`peeps-web` intentionally stays dumb. It ingests runtime deltas, stores them in SQLite, and exposes a tiny HTTP surface.

## Base URL and defaults

By default:

1. HTTP API listens on `http://127.0.0.1:9130`
2. TCP ingest listens on `127.0.0.1:9119`
3. SQLite path is `peeps-web.sqlite` (override with `PEEPS_DB`)

## Endpoints

### `GET /health`

Liveness probe.

Response body:

```text
ok
```

### `GET /api/connections`

Returns currently connected runtime processes.

Response JSON:

```json
{
  "connected_processes": 2,
  "processes": [
    {
      "conn_id": 1,
      "process_name": "worker-a",
      "pid": 12345
    },
    {
      "conn_id": 2,
      "process_name": "worker-b",
      "pid": 12346
    }
  ]
}
```

### `POST /api/cuts`

Triggers a cut across currently connected processes. A cut is a coordination barrier: each process reports the cursor (`stream_id`, `next_seq_no`) it has reached.

Request body:

```json
{}
```

Response JSON:

```json
{
  "cut_id": "cut:1",
  "requested_at_ns": 1739830000000000000,
  "requested_connections": 2
}
```

### `GET /api/cuts/{cut_id}`

Reads current status for one cut.

Response JSON:

```json
{
  "cut_id": "cut:1",
  "requested_at_ns": 1739830000000000000,
  "pending_connections": 1,
  "acked_connections": 1,
  "pending_conn_ids": [2]
}
```

When `pending_connections` reaches `0`, the cut is complete.

### `POST /api/sql`

Runs a SQL query directly against the current SQLite database.

Request JSON:

```json
{
  "sql": "select conn_id, process_name, pid from connections order by conn_id"
}
```

Response JSON:

```json
{
  "columns": ["conn_id", "process_name", "pid"],
  "rows": [
    [1, "worker-a", 12345],
    [2, "worker-b", 12346]
  ],
  "row_count": 2
}
```

Current rule:

1. query must be read-only (`stmt.readonly()`); non-read-only statements are rejected.

No other SQL "safety theater" constraints are enforced right now.

## SQLite tables currently materialized

These tables are written by ingest and available through `/api/sql`:

1. `connections`
2. `cuts`
3. `cut_acks`
4. `stream_cursors`
5. `delta_batches`
6. `entities`
7. `edges`
8. `events`

Notes:

1. `entities` / `edges` / `events` are materialized from delta stream changes.
2. `delta_batches` stores raw batch payloads for traceability/replay work.
3. scope materialization is not wired yet because scope changes are not emitted in the current delta stream.

## Cut flow in plain language

The shortest mental model:

1. frontend calls `POST /api/cuts`
2. server sends `CutRequest` to each connected process
3. each process replies with `CutAck { cut_id, cursor }`
4. frontend polls `GET /api/cuts/{cut_id}` until `pending_connections == 0`

This is exactly what `peeps-cli cut` does as one command.
