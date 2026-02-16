+++
title = "Semaphore, OnceCell, Notify"
weight = 5
+++

These primitives mostly coordinate wakeups and availability rather than carrying business data. Instrumentation emphasizes wait relationships.

## Semaphore

Shows permit contention and blocked acquirers.

## OnceCell

Shows lazy-init timing and whether initialization becomes a bottleneck hotspot.

## Notify

Shows wait/notify coordination patterns.

Across all three, `Needs` edges are the key signal: who is waiting and what they are waiting on.
