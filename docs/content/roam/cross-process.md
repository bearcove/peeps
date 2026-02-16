+++
title = "Cross-Process Linking"
weight = 2
insert_anchor_links = "heading"
+++

The key insight: both sides derive node IDs from the same span ID. The caller creates `request:{span_id}` and the server creates `response:{span_id}`. When peeps-web collects snapshots from both processes, it can join them.

## Metadata propagated on each RPC call

| Key | Description |
|-----|-------------|
| `peeps.span_id` | ULID identifying this request/response pair |
| `peeps.caller_process` | Process name of the caller |
| `peeps.caller_connection` | Connection name at the call site |
| `peeps.caller_request_id` | Request ID on the caller side |
| `peeps.chain_id` | Chain ID for channel correlation |
| `peeps.parent_span_id` | Parent span for nested calls |

## Same-process RPC

Both request and response nodes are in the same process's registry. They appear in the same snapshot graph with the edge between them.

## Cross-process RPC

The request node is in the caller's graph. The response node is in the server's graph. When the peeps-web server collects snapshots from both processes, it persists both into the same snapshot. The shared span ID in the canonical node IDs (`request:{span_id}` and `response:{span_id}`) makes the link visible.

## Unresolved edges

If the server process hasn't responded to the snapshot request (timeout/disconnect), edges pointing to nodes in that process end up in the `unresolved_edges` table. The frontend synthesizes "ghost nodes" for these missing endpoints so the graph remains navigable.
