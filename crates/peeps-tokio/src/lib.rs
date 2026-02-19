//! Tokio-backed peeps instrumentation surface.

#[doc(hidden)]
pub use facet_value;
#[doc(hidden)]
pub use parking_lot;
#[doc(hidden)]
pub use tokio;

#[cfg(target_arch = "wasm32")]
compile_error!("`peeps-tokio` is native-only; use `peeps-wasm` on wasm32");

#[cfg(not(feature = "diagnostics"))]
mod disabled;
#[cfg(feature = "diagnostics")]
mod enabled;

#[cfg(not(feature = "diagnostics"))]
pub use disabled::*;
#[cfg(feature = "diagnostics")]
pub use enabled::*;
