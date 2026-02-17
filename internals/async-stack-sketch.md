# Async stack sketch (future-only, no manual push/pop)

This is a sketch, not final API.

The goal is simple:

- no public `push/pop` stack API
- async causality comes from instrumented futures
- when `diagnostics` is off, this should compile down to "just run the future"

## Usage sketch

```rust
use peeps::{peeps, EntityHandle, EdgeKind};

async fn handler(tx: &mut MySender, item: Item, tx_handle: EntityHandle) -> Result<(), SendError> {
    // Form 1: just name this future.
    // Side-effect (diagnostics on): creates/updates a "future entity" with name "handler.prep".
    // Side-effect (diagnostics off): no entity, no edge, no stack tracking.
    peeps! {
        name = "handler.prep",
        fut = async {
            do_some_prep().await;
        }
    }
    .await;

    // Form 2: this future is an operation on a specific entity.
    // Here we say "this send operation is about tx_handle".
    //
    // Proposed behavior:
    // - edge defaults to `polls`
    // - while polling and still pending, runtime upgrades active relation to `needs`
    // - once ready, active relation is removed
    //
    // Side-effects (diagnostics on):
    // 1. wrapper enters task-local "current future entity" during poll (RAII guard)
    // 2. records/refreshes edge current_future -> tx_handle
    // 3. on Poll::Pending, relation is treated as blocking (`needs`)
    // 4. on Poll::Ready, removes active edge and emits a completion event
    //
    // Side-effects (diagnostics off):
    // - this is just `tx.send(item).await`
    peeps! {
        name = "mpsc.send",
        on = tx_handle,
        edge = EdgeKind::Polls, // optional; default
        fut = tx.send(item),
    }
    .await?;

    Ok(())
}
```

## Runtime shape (enabled path)

```rust
// pseudo-code only
impl<F: Future> Future for InstrumentedFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Side-effect: task-local current future = this.future_entity_id
        let _guard = task_local::enter(self.future_entity_id);

        if let Some(target) = self.target_entity {
            // Side-effect: active relation exists while this future is being polled.
            registry::upsert_edge(self.future_entity_id, target, EdgeKind::Polls);
        }

        match self.inner.poll(cx) {
            Poll::Pending => {
                if let Some(target) = self.target_entity {
                    // Side-effect: mark this relation as blocking for deadlock views.
                    registry::upsert_edge(self.future_entity_id, target, EdgeKind::Needs);
                }
                Poll::Pending
            }
            Poll::Ready(output) => {
                if let Some(target) = self.target_entity {
                    // Side-effect: this wait is done; remove live dependency edge.
                    registry::remove_edge(self.future_entity_id, target);
                }
                // Side-effect: event for timeline/debug inspector.
                registry::record_event(self.future_entity_id, "future.ready");
                Poll::Ready(output)
            }
        }
    }
}
```

## Snapshot + local store sketch

`peeps` runtime keeps a local in-memory state with:

- live entities
- live edges
- append-only event ring/log

On snapshot request:

1. freeze/copy current entities
2. freeze/copy current live edges
3. include relevant recent events (policy TBD)
4. ship one `Snapshot` payload

This is why the "active edge removed on ready" behavior matters: snapshots should show current blockers, not historical blockers pretending to still be active.

## Diagnostics-off sketch

Macro expansion should keep this cheap:

```rust
#[cfg(not(feature = "diagnostics"))]
{
    fut // no wrapper, no ID allocation, no atomics, no string formatting
}
```

Any `EntityHandle` should be a no-op shell in this mode.
