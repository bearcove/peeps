+++
title = "Request and Response Nodes"
weight = 1
insert_anchor_links = "heading"
+++

Each RPC is represented as a caller-side request node and a responder-side response node, tied by shared identity.

## Intent

- Show the handoff from caller to responder as one causal chain.
- Attribute downstream handler work to the response context.
- Expose queueing vs handling vs delivery time at a high level.

## Lifecycle (broad strokes)

1. Caller creates request context and propagates correlation metadata.
2. Responder creates response context and scopes handler work under it.
3. Caller waits on responder completion via causal dependency edges.
4. Both sides update state as transport/handling progresses.
5. Request/response nodes are cleaned up when the RPC finishes.

These nodes are usually short-lived. For persistent debugging, follow the child futures/resources they touched while active.
