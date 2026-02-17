+++
title = "Futures and tasks"
weight = 1
+++

Future instrumentation is the spine of peeps causality.

## Intent

- Attribute work to the future currently being polled.
- Show what that future is blocked on right now.
- Preserve lineage when tasks spawn other tasks.

### Edge behavior

- `Needs` captures active awaits.
- `Touches` captures nested poll context via the causality stack.
- `Spawned` captures parent/child task lineage.

## Practical reading

When a service appears stuck, start at long-lived pending futures and follow their `Needs` chain outward.

For concrete wrapper APIs (`PeepableFuture`, tracked spawn, join set integration), refer to the crate docs/source.
