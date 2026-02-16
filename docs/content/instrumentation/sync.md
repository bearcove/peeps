+++
title = "Semaphore, OnceCell, Notify"
weight = 5
+++

## Semaphore

`peeps::Semaphore` wraps `tokio::sync::Semaphore`.

- Tracks: available permits, waiters
- `needs` edges while waiting to acquire a permit

## OnceCell

`peeps::OnceCell` wraps lazy initialization.

- Tracks: initialization timing
- Node exists until the cell is dropped

## Notify

`peeps::Notify` wraps `tokio::sync::Notify`.

- Tracks: notify/wait patterns
- `needs` edges while waiting for notification
