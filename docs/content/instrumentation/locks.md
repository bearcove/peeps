+++
title = "Locks"
weight = 2
+++

Lock instrumentation exists to make contention visible.

## Graph intent

- Lock nodes represent shared exclusion points.
- Waiters create `Needs` edges while blocked.
- Acquisitions/releases update lock state and remove stale waits.

## Debugging intent

Use lock nodes to answer:

- Is contention local or system-wide?
- Which tasks are waiting behind which holder?
- Is fairness/starvation behavior matching expectations?

Exact lock wrappers and attributes may evolve; contention semantics should remain stable.
