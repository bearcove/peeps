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

### `POST /api/snapshot`

Requests a live point-in-time snapshot from every connected process and returns a cross-process cut.

The server fans out a `SnapshotRequest` to all connected processes. Each process captures `PTime::now()` and its current entity/edge/event state atomically, then replies. The server waits up to **5 seconds** and returns whatever arrived, with a separate list for processes that did not reply in time.

Request body:

```json
{}
```

Response JSON:

```json
{
  "captured_at_unix_ms": 1739800000123,
  "processes": [
    {
      "process_id": 1,
      "process_name": "worker-a",
      "pid": 12345,
      "ptime_now_ms": 5000,
      "snapshot": {
        "entities": [
          {
            "id": "0a1b2c3d",
            "birth": 1245000,
            "source": "src/rpc/demo.rs:42",
            "name": "DemoRpc.sleepy_forever",
            "body": { "request": { "method": "DemoRpc.sleepy_forever", "args_preview": "(no args)" } },
            "meta": {}
          },
          {
            "id": "4e5f6a7b",
            "birth": 3590000,
            "source": "src/dispatch.rs:67",
            "name": "mpsc.send",
            "body": {
              "channel_tx": {
                "lifecycle": "open",
                "details": { "mpsc": { "buffer": { "occupancy": 0, "capacity": 128 } } }
              }
            },
            "meta": {}
          }
        ],
        "scopes": [],
        "edges": [
          { "src": "0a1b2c3d", "dst": "4e5f6a7b", "kind": "needs", "meta": null }
        ],
        "events": []
      }
    }
  ],
  "timed_out_processes": [
    {
      "process_id": 2,
      "process_name": "worker-b",
      "pid": 12346
    }
  ]
}
```

Notes:

1. `captured_at_unix_ms` is the server wall-clock time when the cut was assembled.
2. `ptime_now_ms` is milliseconds since that process started (process-relative, not wall clock). Entity `birth` fields are in the same unit. To convert a `birth` to an approximate wall-clock time: `captured_at_unix_ms - ptime_now_ms + birth`.
3. Entity identity is scoped per process: the globally unique key for an entity is `(process_id, entity.id)`.
4. `body` mirrors the `EntityBody` Rust enum via facet-json: unit variants as a plain string (e.g. `"future"`), data variants as `{ "variant_name": { ... } }`.
5. `edge.kind` is snake_case `EdgeKind`: `"needs"`, `"polls"`, `"closed_by"`, `"channel_link"`, `"rpc_link"`.
6. `timed_out_processes` lists processes that were connected when the request arrived but did not reply within the timeout. The `pid` field can be passed directly to `sample <pid>` or `spindump <pid>` for OS-level stack sampling.
7. For a consistent multi-process view, trigger a cut first and wait for `pending_connections == 0`, then call this endpoint.

## Snapshot flow in plain language

1. frontend calls `POST /api/snapshot`
2. server sends `SnapshotRequest` to each connected process
3. each process calls `PTime::now()`, materialises its current graph state, sends `SnapshotReply { ptime_now_ms, snapshot }`
4. server waits up to 5 s, collects replies, returns the cut

Process identity in the reply comes entirely from transport state (the connection established at handshake). The snapshot payload carries no self-reported process fields.

### `POST /api/query`

Runs a canonical named query pack maintained by the backend.

Request JSON:

```json
{
  "name": "blockers",
  "limit": 50
}
```

Response JSON is the same shape as `/api/sql`:

```json
{
  "columns": ["waiter_id", "waiter_name", "blocked_on_id", "blocked_on_name", "kind_json"],
  "rows": [],
  "row_count": 0
}
```

## SQLite tables currently materialized

These tables are written by ingest and available through `/api/sql`:

1. `connections`
2. `cuts`
3. `cut_acks`
4. `stream_cursors`
5. `delta_batches`
6. `entities`
7. `scopes`
8. `entity_scope_links`
9. `edges`
10. `events`

Notes:

1. `entities` / `edges` / `events` are materialized from delta stream changes.
2. `delta_batches` stores raw batch payloads for traceability/replay work.
3. `scopes` are materialized from delta stream scope changes (`upsert_scope` / `remove_scope`).
4. `entity_scope_links` is materialized from scope-membership delta changes.

## Cut flow in plain language

The shortest mental model:

1. frontend calls `POST /api/cuts`
2. server sends `CutRequest` to each connected process
3. each process replies with `CutAck { cut_id, cursor }`
4. frontend polls `GET /api/cuts/{cut_id}` until `pending_connections == 0`

This is exactly what `peeps-cli cut` does as one command.

## Canonical query packs

To reduce hand-written SQL in clients and agents, backend exposes these via `POST /api/query` and `peeps-cli query --name ...` consumes that endpoint:

1. `blockers`
2. `blocked-senders`
3. `blocked-receivers`
4. `stalled-sends`
5. `channel-pressure`
6. `channel-health`
7. `scope-membership`
8. `stale-blockers`
