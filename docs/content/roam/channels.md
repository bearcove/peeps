+++
title = "Remote Channel Endpoints"
weight = 3
insert_anchor_links = "heading"
+++

When a roam RPC handler creates streaming channels (for bidirectional communication), both endpoints are registered as peeps nodes.

## Node creation

- The sender endpoint gets a `RemoteTx` node: `roam_channel_tx:{chain_id}:{channel_id}`
- The receiver endpoint gets a `RemoteRx` node: `roam_channel_rx:{chain_id}:{channel_id}`
- Labels: `"ch#N tx"` and `"ch#N rx"`

## Edges

- `tx → rx` — structural edge linking the two endpoints
- `request → tx` and `request → rx` — context edges linking the channel to the RPC that created it

## Cross-process channel identity

Both sides of a cross-process channel derive the same node ID from shared values:

- `chain_id` — propagated in RPC metadata as `peeps.chain_id`
- `channel_id` — sequential per-connection channel counter

This ensures that when peeps-web collects graphs from both processes, the Tx node in one process and the Rx node in the other can be correlated by their matching IDs.
