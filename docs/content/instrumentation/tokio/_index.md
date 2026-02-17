+++
title = "Tokio"
weight = 1
insert_anchor_links = "heading"
+++

Tokio instrumentation is about reconstructing wait relationships without pretending we can recover a stable async call stack from the runtime.

The base layer is futures. A tracked future becomes a node. When it awaits something, peeps records a `needs` edge to whatever it is now blocked on. That gives you a live dependency graph instead of a pile of parked threads.

From there, peeps instruments the resource boundaries where waiting becomes meaningful:

- locks (`lock`)
- channels (`tx`, `rx`)
- sync primitives (`semaphore`, `oncecell`, `notify`)
- time boundaries (`sleep`, `interval`, `timeout`)
- system boundaries (`command`, `file_op`, `net_connect`, `net_accept`, `net_readable`, `net_writable`)

The causality stack is the glue that keeps this coherent. It is task-local context of "what node is currently running". That lets wrappers emit consistent causal links without passing parent IDs everywhere.

Practically, this is what Tokio instrumentation gives you:

- where contention is (`lock`, `semaphore`)
- where backpressure is (`tx`/`rx`)
- where wakeups stop happening (`notify`, channel receive paths)
- where time is the blocker versus I/O or contention (`sleep`/`timeout` vs lock/channel/system waits)
- where work left process memory and entered external boundaries (commands, file ops, net readiness/connectivity)

Canonical payload shapes for these nodes and their edges/events are in [Schema](/architecture/reference/schema/).
