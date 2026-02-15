//! Future instrumentation: `PeepableFuture`, `peepable()`, `spawn_tracked()`.
//!
//! Tracks live futures in a registry, integrates with the task-local stack
//! for canonical edge emission, and emits future nodes into the graph.
//!
//! When `diagnostics` is disabled, all wrappers compile to zero-cost pass-throughs.

#[cfg(not(feature = "diagnostics"))]
mod disabled;
#[cfg(feature = "diagnostics")]
mod enabled;

#[cfg(not(feature = "diagnostics"))]
pub use disabled::*;
#[cfg(feature = "diagnostics")]
pub use enabled::*;

// emit_into_graph is crate-internal only
#[cfg(feature = "diagnostics")]
pub(crate) use enabled::emit_into_graph;
#[cfg(not(feature = "diagnostics"))]
pub(crate) use disabled::emit_into_graph;
