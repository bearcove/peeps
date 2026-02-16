+++
title = "Remote Channel Endpoints"
weight = 3
insert_anchor_links = "heading"
+++

When roam creates streaming channels during RPC handling, both endpoints become graph nodes so stream backpressure can be correlated with request flow.

## Intent

- Make remote stream endpoints first-class causal entities.
- Preserve correlation across process boundaries.
- Keep channel lineage attached to the originating request context.

## Causal shape

- Endpoint pairing links sender/receiver as one logical channel.
- Request context links channel activity back to the RPC that created it.

## Identity strategy

Both sides derive correlated IDs from shared metadata so snapshots can join them even when endpoints live in different processes.
