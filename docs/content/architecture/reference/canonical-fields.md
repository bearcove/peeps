+++
title = "Canonical Fields"
weight = 8
+++

The frontend inspector uses a strict canonical attribute contract. It does not read legacy aliases.

## Canonical fields

Inspector common fields read only:

- `created_at` (required, Unix epoch ns)
- `source` (required)
- `method` (optional)
- `correlation` (optional)

Node identity and process come from node envelope fields:

- `id` (required)
- `process` (required)

## Timeline origin

Timeline origin uses:

1. `created_at` if present and sane for the event window
2. first timeline event timestamp otherwise

Sanity guard:

- if `created_at > first_event_ts`, use first event
- if `first_event_ts - created_at > 30 days`, use first event

No fallback alias keys are used.

## Breaking-change migration

Removed aliases:

- `request.*` and `response.*` method/status/timing/correlation fields
- `ctx.location`
- `correlation_key`, `request_id`, `trace_id`, `correlation_id`
- `created_at_ns` as a canonical timestamp alias

Downstream consumers must migrate to:

- `method`
- `correlation`
- `source`
- `created_at`

Representative canonical payloads:

```json
{"id":"request:01J...","kind":"request","process":"api","attrs":{"created_at":1700000000000000000,"source":"/srv/api/request.rs:42","method":"GetUser","correlation":"01J...","status":"in_flight"}}
```

```json
{"id":"response:01J...","kind":"response","process":"api","attrs":{"created_at":1700000000500000000,"source":"/srv/api/response.rs:73","method":"GetUser","correlation":"01J...","status":"completed","elapsed_ns":50000000}}
```

```json
{"id":"tx:01J...","kind":"tx","process":"worker","attrs":{"created_at":1700000001000000000,"source":"/srv/queue.rs:88","channel_kind":"mpsc","age_ns":512000000,"queue_len":3}}
```
