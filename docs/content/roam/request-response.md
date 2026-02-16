+++
title = "Request and Response Nodes"
weight = 1
insert_anchor_links = "heading"
+++

Every RPC call creates a pair of nodes linked by a shared span ID.

## Caller side (outgoing request)

1. A ULID span ID is generated: e.g., `01ARZ3NDEKTSV4RRFFQ69G5FAV`
2. A `Request` node is registered: `request:01ARZ3NDEKTSV4RRFFQ69G5FAV`
3. Attributes include: `request.id`, `request.method`, `rpc.connection`, `request.args`
4. An edge is created from the caller's current stack context to the request node (`touches`)
5. An edge is created from the request node to the expected response node
6. The span ID is embedded in outgoing RPC metadata as `peeps.span_id`
7. The entire call future is wrapped in `peeps::stack::scope(&request_node_id, ...)` — so anything the caller does while waiting is attributed to this request
8. On completion: the request node is **removed** from the registry (unless `PEEPS_KEEP_COMPLETED_RPC=1` is set)

## Server side (incoming request)

1. The span ID is extracted from incoming metadata
2. A `Response` node is registered: `response:01ARZ3NDEKTSV4RRFFQ69G5FAV`
3. Initial attributes: `response.state = "handling"`, `response.created_at_ns`, `request.id`, `request.method`
4. An edge links `request:{span_id}` → `response:{span_id}`
5. The handler future is wrapped in `peeps::stack::scope(&response_node_id, ...)` — so all work done by the handler is causally linked to this response

## Response completion

When the handler finishes, the response node is updated with `elapsed_ns`.

In `roam-shm`: state transitions from `"handling"` to `"queued"` with `response.queued_at_ns` and `response.handled_elapsed_ns`.

Response nodes **persist** (unlike request nodes) — they represent the server's view of work done.

## Deriving timing from response attributes

- Queue wait = `queued_at_ns - created_at_ns`
- Service time = `handled_elapsed_ns`
