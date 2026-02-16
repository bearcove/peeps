+++
title = "Locks"
weight = 2
+++

peeps provides four lock wrappers:

| Wrapper | Underlying | Use case |
|---------|-----------|----------|
| `peeps::Mutex<T>` | `parking_lot::Mutex` | Synchronous mutual exclusion |
| `peeps::RwLock<T>` | `parking_lot::RwLock` | Synchronous read-write |
| `peeps::AsyncMutex<T>` | `tokio::sync::Mutex` | Async mutual exclusion |
| `peeps::AsyncRwLock<T>` | `tokio::sync::RwLock` | Async read-write |

## What they track

- Current holders and waiters
- Total acquires and releases
- `needs` edges while waiting to acquire (removed once acquired)

**Node kind:** `Lock`
