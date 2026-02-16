+++
title = "Channels"
weight = 3
+++

Channel instrumentation answers three questions:

1. Where is backpressure?
2. Who is waiting for whom?
3. Why did a channel close?

## Graph intent

- Sender and receiver endpoints appear as distinct nodes.
- Waiting to send/receive creates `Needs` edges.
- Closure propagation can create `ClosedBy` edges.

## Edge behavior

- `Needs`: active wait on capacity/value/message.
- `Touches`: observed interactions during polling.
- `ClosedBy`: causal close chain when one side disappears.

For per-channel-type attributes, check the corresponding wrapper implementation.
