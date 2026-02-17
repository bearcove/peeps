+++
title = "Schema"
weight = 2
insert_anchor_links = "heading"
+++

This page is schema-first reference for the canonical payload shapes used by peeps snapshots.

## Schema

Snapshots are per-process envelopes containing `nodes`, `edges`, and optional `events`.

```json
{
  "process_name": "worker",
  "proc_key": "worker@1234",
  "nodes": [],
  "edges": [],
  "events": []
}
```

## Canonical Node Payload

```json
{
  "id": "request:01J...",
  "kind": "request",
  "label": "DemoRpc.sleepy_forever",
  "attrs_json": "{\"created_at\":1700000000000000000,\"source\":\"/srv/app.rs:42\",\"method\":\"DemoRpc.sleepy_forever\",\"correlation\":\"1\",\"status\":\"in_flight\"}"
}
```

Notes:

- `id` is globally unique per logical entity.
- `kind` is one of the canonical node kinds (`future`, `lock`, `tx`, `rx`, `request`, `response`, etc.).
- `attrs_json` must include `created_at` and `source` for inspector compatibility.
- `method` and `correlation` are optional canonical fields in `attrs_json`.

## Canonical Inspector Fields

Inspector common fields read only:

- `created_at` (required, Unix epoch ns)
- `source` (required)
- `method` (optional)
- `correlation` (optional)

Node identity and process come from envelope fields:

- `id` (required)
- `process` (required)

Timeline origin uses:

1. `created_at` if present and sane for the event window
2. first timeline event timestamp otherwise

Sanity guard:

- if `created_at > first_event_ts`, use first event
- if `first_event_ts - created_at > 30 days`, use first event

## Canonical Edge Payload

```json
{
  "src": "future:01J...",
  "dst": "lock:01J...",
  "kind": "needs",
  "attrs_json": "{}"
}
```

Notes:

- `src` and `dst` refer to node IDs.
- `kind` is one of `needs`, `touches`, `spawned`, `closed_by`.
- `attrs_json` is reserved for edge metadata and may be empty.

## Canonical Event Payload

```json
{
  "id": "event:01J...",
  "ts_ns": 1700000000123456789,
  "proc_key": "worker@1234",
  "entity_id": "response:01J...",
  "name": "rpc.response.sent",
  "parent_entity_id": "request:01J...",
  "attrs_json": "{\"request.id\":\"1\",\"request.method\":\"DemoRpc.sleepy_forever\",\"rpc.connection\":\"initiator<->acceptor\"}"
}
```

Notes:

- `entity_id` points to the node the event is about.
- `name` is the canonical event type field.
- `parent_entity_id` is optional and captures immediate causal parent when known.
- `attrs_json` is event-specific detail; keep key naming stable and queryable.
