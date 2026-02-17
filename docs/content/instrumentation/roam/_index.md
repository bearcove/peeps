+++
title = "Roam"
weight = 2
sort_by = "weight"
insert_anchor_links = "heading"
+++

Roam instrumentation is where peeps becomes genuinely cross-process.

With local Tokio primitives, you can explain why one runtime is blocked. With Roam, you can explain why process A is blocked on process B, which may itself be blocked on process A.

The main node kinds emitted by Roam integration are:

- `connection`: transport / RPC connection identity and state
- `request`: outgoing request lifecycle entities
- `response`: handling/delivery lifecycle entities
- `remote_tx` and `remote_rx`: remote channel endpoints bridged across process boundaries

In practice, this gives you explicit request/response handoff points and connection context in the same graph as local futures, locks, and channels.

That is the part single-process executor views cannot reconstruct on their own.

Canonical payload shapes are documented in [Schema](/architecture/schema/).
