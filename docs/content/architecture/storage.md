+++
title = "Storage"
weight = 3
+++

## SQLite schema

All data is stored in a local SQLite database (default path: `./peeps-web.sqlite`, configurable via `PEEPS_DB`). The database uses `journal_mode=WAL` and `synchronous=NORMAL`.

### `snapshots`

One row per captured snapshot.

| Column | Type | Description |
|--------|------|-------------|
| `snapshot_id` | INTEGER PK | Auto-incrementing snapshot ID |
| `requested_at_ns` | INTEGER NOT NULL | When the snapshot was requested (unix nanos) |
| `completed_at_ns` | INTEGER | When all responses were collected |
| `timeout_ms` | INTEGER NOT NULL | Timeout used for this snapshot |

### `snapshot_processes`

One row per process per snapshot. Primary key: `(snapshot_id, proc_key)`.

| Column | Type | Description |
|--------|------|-------------|
| `snapshot_id` | INTEGER | FK to snapshots |
| `proc_key` | TEXT | Unique process key |
| `process` | TEXT | Human-readable process name |
| `pid` | INTEGER | OS process ID |
| `status` | TEXT | `responded`, `timeout`, or `disconnected` |
| `recv_at_ns` | INTEGER | When the response was received |
| `error_text` | TEXT | Error message if any |

### `nodes`

All nodes across all snapshots. Primary key: `(snapshot_id, id)`.

| Column | Type | Description |
|--------|------|-------------|
| `snapshot_id` | INTEGER | FK to snapshots |
| `id` | TEXT | Node ID (`{kind}:{ulid}`) |
| `kind` | TEXT | Node kind name |
| `process` | TEXT | Process name |
| `proc_key` | TEXT | Process key |
| `attrs_json` | TEXT | JSON attributes |

### `edges`

All edges across all snapshots. Primary key: `(snapshot_id, src_id, dst_id, kind)`.

| Column | Type | Description |
|--------|------|-------------|
| `snapshot_id` | INTEGER | FK to snapshots |
| `src_id` | TEXT | Source node ID |
| `dst_id` | TEXT | Destination node ID |
| `kind` | TEXT | Edge kind |
| `attrs_json` | TEXT | JSON attributes |

### `unresolved_edges`

Edges where one endpoint is in a different process that hasn't responded. Primary key: `(snapshot_id, src_id, dst_id)`.

| Column | Type | Description |
|--------|------|-------------|
| `snapshot_id` | INTEGER | FK to snapshots |
| `src_id` | TEXT | Source node ID |
| `dst_id` | TEXT | Destination node ID |
| `missing_side` | TEXT | Which side is missing |
| `reason` | TEXT | Why it's unresolved |
| `referenced_proc_key` | TEXT | Expected process |
| `attrs_json` | TEXT | JSON attributes |

### `ingest_events`

Log of snapshot ingestion events and errors.

| Column | Type | Description |
|--------|------|-------------|
| `event_id` | INTEGER PK | Auto-incrementing event ID |
| `event_at_ns` | INTEGER | When the event occurred (unix nanos) |
| `snapshot_id` | INTEGER | Associated snapshot |
| `process` | TEXT | Process name |
| `pid` | INTEGER | OS process ID |
| `proc_key` | TEXT | Process key |
| `event_kind` | TEXT | Type of event |
| `detail` | TEXT | Event details |

### Indexes

- `nodes(snapshot_id, kind)`
- `nodes(snapshot_id, proc_key)`
- `edges(snapshot_id, src_id)`
- `edges(snapshot_id, dst_id)`
- `unresolved_edges(snapshot_id)`

### Snapshot scoping

When the frontend queries via `/api/sql`, it operates against TEMP VIEWs that filter to a single `snapshot_id`. Queries never see data from other snapshots.

### Retention

A maximum of 500 snapshots are kept. When a new snapshot is finalized, snapshots beyond the limit and their associated nodes, edges, and process records are deleted. Ingest events older than 7 days are also pruned.
