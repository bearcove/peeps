+++
title = "Timers"
weight = 4
+++

Timer instrumentation answers "is this work waiting on time itself?"

## Intent

- Represent waits introduced by `sleep`/interval cadence/timeout guards.
- Distinguish timer-driven waiting from lock/channel/external I/O waits.
- Preserve causal links from parent tasks to timer nodes.

## Reading tip

If many tasks share long timer waits, that is usually intentional pacing. If a timeout node dominates critical paths, investigate downstream dependency latency.
