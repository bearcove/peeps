//! New peeps instrumentation surface.
//!
//! Top-level split:
//! - `enabled`: real diagnostics runtime
//! - `disabled`: zero-cost pass-through API

#[cfg(not(feature = "diagnostics"))]
mod disabled;
#[cfg(feature = "diagnostics")]
mod enabled;

#[cfg(not(feature = "diagnostics"))]
pub use disabled::*;
#[cfg(feature = "diagnostics")]
pub use enabled::*;
