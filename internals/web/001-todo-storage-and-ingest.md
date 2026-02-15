# Storage And Ingest Spec

Status: todo
Owner: wg-storage-ingest
Scope: `crates/peeps-web`

## Goal

Persist synchronized multi-process graph snapshots keyed by `snapshot_id`.

## Ingest contract (v1)

Server-orchestrated pull snapshots:

1. UI triggers `POST /api/jump-now`.
2. Server allocates `snapshot_id`.
3. Server requests dumps from all currently connected processes.
4. Each process replies with framed UTF-8 JSON payload tagged with `snapshot_id`.

Frame format:
- header: 4-byte big-endian length
- body: UTF-8 JSON `ProcessDump`

Control-plane message contract:
- server -> process request:
```json
{
  "type": "snapshot_request",
  "snapshot_id": 1234,
  "timeout_ms": 1500
}
```
- process -> server response envelope:
```json
{
  "type": "snapshot_reply",
  "snapshot_id": 1234,
  "process": "vx-vfsd",
  "pid": 51859,
  "dump": { "...": "ProcessDump" }
}
```

Lifecycle rules:
- connected processes are discovered/tracked via active ingest connections.
- only one in-flight snapshot is allowed in v1.
- reply with unknown/mismatched `snapshot_id` is rejected from snapshot writes and recorded in `ingest_events`.
- late replies after snapshot completion are dropped and logged to `ingest_events`.

## SQLite schema (v1)

```sql
CREATE TABLE snapshots (
  snapshot_id INTEGER PRIMARY KEY,
  requested_at_ns INTEGER NOT NULL,
  completed_at_ns INTEGER,
  timeout_ms INTEGER NOT NULL
);

CREATE TABLE snapshot_processes (
  snapshot_id INTEGER NOT NULL,
  process TEXT NOT NULL,
  pid INTEGER,
  proc_key TEXT NOT NULL, -- e.g. "{process}:{pid}" (or stable runtime instance id)
  status TEXT NOT NULL, -- responded|timeout|disconnected|error
  recv_at_ns INTEGER,
  error_text TEXT,
  PRIMARY KEY (snapshot_id, proc_key)
);

CREATE TABLE nodes (
  snapshot_id INTEGER NOT NULL,
  id TEXT NOT NULL,
  kind TEXT NOT NULL,
  process TEXT NOT NULL,
  proc_key TEXT NOT NULL,
  attrs_json TEXT NOT NULL,
  PRIMARY KEY (snapshot_id, id)
);

CREATE TABLE edges (
  snapshot_id INTEGER NOT NULL,
  src_id TEXT NOT NULL,
  dst_id TEXT NOT NULL,
  kind TEXT NOT NULL, -- always 'needs'
  attrs_json TEXT NOT NULL,
  PRIMARY KEY (snapshot_id, src_id, dst_id)
);

CREATE TABLE unresolved_edges (
  snapshot_id INTEGER NOT NULL,
  src_id TEXT NOT NULL,
  dst_id TEXT NOT NULL,
  missing_side TEXT NOT NULL, -- src|dst|both
  reason TEXT NOT NULL, -- referenced_proc_missing|referenced_proc_timeout|referenced_proc_disconnected
  referenced_proc_key TEXT,
  attrs_json TEXT NOT NULL,
  PRIMARY KEY (snapshot_id, src_id, dst_id)
);

CREATE TABLE ingest_events (
  event_id INTEGER PRIMARY KEY,
  event_at_ns INTEGER NOT NULL,
  snapshot_id INTEGER,
  process TEXT,
  pid INTEGER,
  proc_key TEXT,
  event_kind TEXT NOT NULL, -- snapshot_id_mismatch|late_reply|decode_error|other
  detail TEXT NOT NULL
);

CREATE INDEX idx_nodes_snapshot_kind ON nodes(snapshot_id, kind);
CREATE INDEX idx_nodes_snapshot_proc_key ON nodes(snapshot_id, proc_key);
CREATE INDEX idx_edges_snapshot_src ON edges(snapshot_id, src_id);
CREATE INDEX idx_edges_snapshot_dst ON edges(snapshot_id, dst_id);
CREATE INDEX idx_unresolved_edges_snapshot ON unresolved_edges(snapshot_id);
```

## Write semantics

- `snapshot_id` is global and monotonic.
- Process replies are written transactionally per reply.
- Missing responders are represented in `snapshot_processes`.
- reply skew is stored (`recv_at_ns`) and surfaced for debugging.
- if edge endpoints are missing because referenced process did not respond, write into `unresolved_edges` (not `edges`).
- if referenced process responded but endpoint is still missing, treat as ingest error.

## Retention

- Keep latest N snapshots (default 500).
- Delete old rows from `unresolved_edges`, `edges`, `nodes`, `snapshot_processes`, `snapshots` by `snapshot_id` cutoff.
- Keep `ingest_events` for latest 7 days.

## Acceptance criteria

1. `jump-now` yields one coherent `snapshot_id` across processes.
2. Missing process responses are explicit in status table.
3. No partial write for an individual process reply transaction.
4. Unresolved cross-process references are preserved in `unresolved_edges`.
