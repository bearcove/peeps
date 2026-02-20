//! Tokio-backed moire instrumentation surface.

#[cfg(target_arch = "wasm32")]
compile_error!("`moire-tokio` is native-only; use `moire-wasm` on wasm32");

// r[impl process.feature-gate]
#[cfg(not(feature = "diagnostics"))]
mod disabled;
#[cfg(feature = "diagnostics")]
mod enabled;

#[cfg(not(feature = "diagnostics"))]
#[allow(unused_imports)]
pub use disabled::*;
#[cfg(feature = "diagnostics")]
pub use enabled::*;
