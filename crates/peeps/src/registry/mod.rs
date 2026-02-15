//! Unified diagnostics registry for all tracked resource types.
//!
//! Central storage for all live diagnostics objects, canonical edge tracking,
//! and process metadata. All resource modules register into this registry;
//! no private registries.
//!
//! When `diagnostics` is disabled, all operations compile away to no-ops
//! and `emit_graph()` returns an empty snapshot.

#[cfg(not(feature = "diagnostics"))]
mod disabled;
#[cfg(feature = "diagnostics")]
mod enabled;

// Re-export public API items for external crates
#[cfg(not(feature = "diagnostics"))]
pub use disabled::{
    edge, register_node, remove_edge, remove_edges_from, remove_edges_to, remove_node,
};
#[cfg(feature = "diagnostics")]
pub use enabled::{
    edge, register_node, remove_edge, remove_edges_from, remove_edges_to, remove_node,
};

// Re-export crate-internal items
#[cfg(not(feature = "diagnostics"))]
pub(crate) use disabled::{emit_graph, init, proc_key, process_name};
#[cfg(feature = "diagnostics")]
pub(crate) use enabled::{emit_graph, init, proc_key, process_name};
