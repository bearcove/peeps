//! New peeps instrumentation surface.
//!
//! Top-level split:
//! - `enabled`: real diagnostics runtime
//! - `disabled`: zero-cost pass-through API

#[cfg(not(target_arch = "wasm32"))]
pub mod fs;
#[cfg(target_arch = "wasm32")]
pub mod fs {}
pub mod net;

#[doc(hidden)]
pub use facet_value;
#[doc(hidden)]
pub use parking_lot;
#[doc(hidden)]
pub use tokio;

#[cfg(all(feature = "diagnostics", target_arch = "wasm32"))]
compile_error!(
    "`peeps` diagnostics is not supported on wasm32; build wasm targets without `feature=\"diagnostics\"`"
);

#[cfg(not(feature = "diagnostics"))]
mod disabled;
#[cfg(all(feature = "diagnostics", not(target_arch = "wasm32")))]
mod enabled;

#[cfg(not(feature = "diagnostics"))]
pub use disabled::*;
#[cfg(all(feature = "diagnostics", not(target_arch = "wasm32")))]
pub use enabled::*;
