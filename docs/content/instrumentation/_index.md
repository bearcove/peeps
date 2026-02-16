+++
title = "Instrumentation"
weight = 3
sort_by = "weight"
insert_anchor_links = "heading"
+++

peeps wraps Tokio and ecosystem primitives to track them in the dependency graph. All wrappers are feature-gated behind `diagnostics` — when disabled, they compile to zero-cost pass-throughs with no runtime overhead. This section documents each wrapped primitive.

| Category | Primitives |
|----------|-----------|
| Async | `Future`, `JoinSet` |
| Locks | `Mutex`, `RwLock` (sync via parking_lot), `AsyncMutex`, `AsyncRwLock` (async via tokio) |
| Channels | `mpsc` (bounded + unbounded), `oneshot`, `watch` |
| Timers | `sleep`, `interval`, `timeout` |
| Sync | `Semaphore`, `OnceCell`, `Notify` |
| System | `Command`, file ops, net ops (connect/accept/readable/writable) |
