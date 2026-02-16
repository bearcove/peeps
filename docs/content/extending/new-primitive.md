+++
title = "Adding a New Instrumented Primitive"
weight = 1
insert_anchor_links = "heading"
+++

Adding a new instrumented primitive.

Follow this checklist.

## 1. Add the node kind

Add a variant to `NodeKind` in `peeps-types/src/lib.rs`.

## 2. Create the wrapper module

In `peeps/src/`, create a directory with two files:

- `enabled.rs` — the real implementation (compiled when `diagnostics` feature is on)
- `disabled.rs` — a pass-through that delegates to the underlying type (compiled when `diagnostics` is off)

The module's `mod.rs` uses `cfg` to select:

```rust
#[cfg(feature = "diagnostics")]
mod enabled;
#[cfg(not(feature = "diagnostics"))]
mod disabled;

#[cfg(feature = "diagnostics")]
pub use enabled::*;
#[cfg(not(feature = "diagnostics"))]
pub use disabled::*;
```

## 3. In the enabled module

**On creation:** register a node with `peeps::registry::register_node(Node { id, kind, label, attrs_json })`.

- ID convention: `{kind}:{ulid}` — generate a ULID for each instance via `peeps_types::new_node_id`.

**While waiting:** emit `needs` edges via `peeps::registry::edge(src, dst)`.

**On interaction:** the causality stack handles `touches` edges automatically if you use `peeps::stack::scope` or if the wrapper is polled inside a `PeepableFuture`.

**On state change:** update the node by calling `register_node` again with the same ID (it upserts).

**On Drop:** call `peeps::registry::remove_node(id)` to clean up the node and all its edges.

## 4. Required attributes

- `elapsed_ns` — at minimum, track lifetime duration.
- Location metadata — use `std::panic::Location::caller()` with `crate::caller_location(caller)` to capture the call site.

## 5. Edge conventions

- `needs` — emit while blocked, remove when unblocked.
- `touches` — handled automatically by the stack for polled futures.
- `spawned` — use for parent/child relationships (via `peeps::registry::spawn_edge`).
- `closed_by` — use when tracking why something was closed.

## 6. Re-export

Add the new type to `peeps/src/lib.rs` public API.
