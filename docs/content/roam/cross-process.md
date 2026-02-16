+++
title = "Cross-Process Linking"
weight = 2
insert_anchor_links = "heading"
+++

Cross-process linking is based on shared identity, not timing coincidence.

## Core idea

Caller and responder derive compatible request/response identities from propagated metadata. During snapshot ingest, peeps-web can join both halves into one causal path.

## Metadata contract

- Required for request/response linking: `peeps.span_id`
- Optional for channel correlation: `peeps.chain_id`
- Optional for nested request trees: `peeps.parent_span_id`

If one side of a linked edge is absent in a snapshot, the UI synthesizes a ghost endpoint from dangling edge references so causal paths remain visible.
