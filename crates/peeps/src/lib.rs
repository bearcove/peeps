//! Shim crate that re-exports the target backend.
//!
//! - Native targets: `peeps-tokio`
//! - wasm32 targets: `peeps-wasm`

#[cfg(not(target_arch = "wasm32"))]
pub use peeps_tokio::*;
#[cfg(target_arch = "wasm32")]
pub use peeps_wasm::*;

#[doc(hidden)]
#[cfg(not(target_arch = "wasm32"))]
pub use peeps_tokio as __backend;
#[doc(hidden)]
#[cfg(target_arch = "wasm32")]
pub use peeps_wasm as __backend;

#[macro_export]
macro_rules! facade {
    () => {
        $crate::__backend::facade!();
    };
}
