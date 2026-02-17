+++
title = "Runtime Events (v1)"
weight = 2
insert_anchor_links = "heading"
+++

This page defines the **v1 runtime event taxonomy** for peeps.

Scope is intentionally narrow:

- RPC request/response lifecycle events.
- Channel lifecycle and message-flow events.
- No poll events in v1 (for example, no `*.poll.*` names).

## Event schema

Each runtime event row uses this shape:

- `id` (ULID)
- `ts_ns`
- `proc_key`
- `entity_id`
- `name`
- `parent_entity_id` (nullable)
- `attrs_json`

Notes:

- `name` is the canonical event type field in v1. Do not use `event_kind` for runtime events.
- `entity_id` points at the node the event is about (for example `request:<ulid>`, `response:<ulid>`, `mpsc_tx:<ulid>`).
- `parent_entity_id` links to the immediate causal parent entity when known; otherwise `NULL`.

## Naming convention for `name`

`name` is dot-separated and lower-case:

`<domain>.<subject>.<action>[.<outcome>]`

Rules:

- Use stable nouns for `domain` and `subject`.
- Use verbs for `action`.
- Add the optional outcome segment only when it changes interpretation.
- Keep names short and query-friendly.

Examples:

- `rpc.request.sent`
- `rpc.response.completed.ok`
- `channel.recv.empty`

## Attrs conventions

`attrs_json` is event-specific, with these shared conventions:

- Use dot-separated keys (`request.id`, `channel.kind`, `rpc.connection`).
- Keep identifiers as strings unless numeric by nature.
- Keep timing as integer nanoseconds with `_ns` suffix.
- Put outcome details in attrs (`error.kind`, `error.message`) while keeping `name` stable.

Common keys used across v1 names:

- `request.id`, `request.method`, `request.direction`
- `rpc.connection`, `rpc.peer`
- `channel.id`, `channel.kind`, `channel.endpoint` (`tx` or `rx`)
- `queue.depth`, `queue.capacity`
- `elapsed_ns`, `queue_wait_ns`, `handler_ns`
- `close.cause`, `cancelled`

## v1 event names (RPC + channels only)

### RPC request/response events

| `name` | When emitted | Expected attrs in `attrs_json` |
|---|---|---|
| `rpc.request.created` | Request context/node is created | `request.id`, `request.method`, `request.direction`, `rpc.connection` |
| `rpc.request.sent` | Outgoing request is handed to transport | `request.id`, `request.method`, `rpc.connection`, `rpc.peer` |
| `rpc.request.cancelled` | Caller abandons in-flight request | `request.id`, `request.method`, `cancelled`, `close.cause` |
| `rpc.response.started` | Responder starts handling | `request.id`, `request.method`, `rpc.connection`, `queue_wait_ns` |
| `rpc.response.completed.ok` | Handler completed successfully | `request.id`, `request.method`, `handler_ns` |
| `rpc.response.completed.err` | Handler completed with error | `request.id`, `request.method`, `handler_ns`, `error.kind`, `error.message` |
| `rpc.response.sent` | Response is sent back to caller | `request.id`, `request.method`, `rpc.connection`, `elapsed_ns` |
| `rpc.response.cancelled` | Response lifecycle aborted/cancelled | `request.id`, `request.method`, `cancelled`, `close.cause` |

### Channel events

| `name` | When emitted | Expected attrs in `attrs_json` |
|---|---|---|
| `channel.created` | Channel endpoint pair created | `channel.id`, `channel.kind`, `queue.capacity` (if bounded) |
| `channel.send` | Send succeeds | `channel.id`, `channel.kind`, `channel.endpoint` (`tx`), `queue.depth` |
| `channel.send.blocked` | Send had to wait for capacity/receiver progress | `channel.id`, `channel.kind`, `channel.endpoint` (`tx`), `queue.depth`, `queue.capacity`, `elapsed_ns` |
| `channel.send.failed.closed` | Send fails because channel is closed | `channel.id`, `channel.kind`, `channel.endpoint` (`tx`), `close.cause` |
| `channel.recv` | Receive succeeds | `channel.id`, `channel.kind`, `channel.endpoint` (`rx`), `queue.depth` |
| `channel.recv.empty` | Non-blocking receive finds no message | `channel.id`, `channel.kind`, `channel.endpoint` (`rx`) |
| `channel.recv.failed.closed` | Receive fails because channel is closed/disconnected | `channel.id`, `channel.kind`, `channel.endpoint` (`rx`), `close.cause` |
| `channel.closed` | Endpoint or channel is closed | `channel.id`, `channel.kind`, `close.cause` |

## SQL cookbook (inspector timeline)

These examples are SQL-first and meant for the existing SQL query flow.

### 1) Anchor a timeline to `captured_at_ns`

Use `captured_at_ns` from snapshot capture and compute event age relative to that anchor:

```sql
WITH anchor AS (
  SELECT CAST(?2 AS INTEGER) AS captured_at_ns
)
SELECT
  e.id,
  e.ts_ns,
  e.proc_key,
  e.entity_id,
  e.name,
  (anchor.captured_at_ns - e.ts_ns) AS age_ns,
  e.attrs_json
FROM events e
CROSS JOIN anchor
WHERE (?1 IS NULL OR e.proc_key = ?1)
  AND e.ts_ns <= anchor.captured_at_ns
ORDER BY e.ts_ns DESC, e.id DESC;
```

Bind parameters:

- `?1`: `proc_key` (or `NULL` for all processes)
- `?2`: `captured_at_ns`

### 2) RPC timeline for one request

```sql
WITH anchor AS (
  SELECT CAST(?3 AS INTEGER) AS captured_at_ns
)
SELECT
  e.ts_ns,
  e.name,
  e.proc_key,
  json_extract(e.attrs_json, '$."request.method"') AS request_method,
  json_extract(e.attrs_json, '$."rpc.connection"') AS rpc_connection,
  (anchor.captured_at_ns - e.ts_ns) AS age_ns
FROM events e
CROSS JOIN anchor
WHERE e.proc_key = ?1
  AND e.ts_ns <= anchor.captured_at_ns
  AND json_extract(e.attrs_json, '$."request.id"') = ?2
  AND e.name GLOB 'rpc.*'
ORDER BY e.ts_ns ASC, e.id ASC;
```

Bind parameters:

- `?1`: `proc_key`
- `?2`: request ID (`request.id`)
- `?3`: `captured_at_ns`

### 3) Channel pressure view (blocked sends)

```sql
WITH anchor AS (
  SELECT CAST(?2 AS INTEGER) AS captured_at_ns
)
SELECT
  e.proc_key,
  e.entity_id AS channel_endpoint,
  COUNT(*) AS full_count,
  MIN(anchor.captured_at_ns - e.ts_ns) AS newest_age_ns
FROM events e
CROSS JOIN anchor
WHERE (?1 IS NULL OR e.proc_key = ?1)
  AND e.ts_ns <= anchor.captured_at_ns
  AND e.name = 'channel.v1.mpsc.try_send'
  AND json_extract(e.attrs_json, '$.error') = 'full'
GROUP BY e.proc_key, channel_endpoint
ORDER BY full_count DESC, newest_age_ns ASC;
```

Bind parameters:

- `?1`: `proc_key` (or `NULL` for all processes)
- `?2`: `captured_at_ns`

## Non-goals in v1

- No poll-event taxonomy (`future.poll.*`, `task.poll.*`, etc.).
- No extra endpoint design; inspector queries remain SQL-first.
