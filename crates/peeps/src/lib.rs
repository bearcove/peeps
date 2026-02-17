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

#[cfg(all(feature = "diagnostics", target_arch = "wasm32"))]
compile_error!(
    "`peeps` diagnostics is not supported on wasm32; build wasm targets without `feature=\"diagnostics\"`"
);

pub struct Mutex<T> {
    inner: parking_lot::Mutex<T>,
}

impl<T> Mutex<T> {
    pub fn new(_name: &'static str, value: T) -> Self {
        Self {
            inner: parking_lot::Mutex::new(value),
        }
    }

    pub fn lock(&self) -> parking_lot::MutexGuard<'_, T> {
        self.inner.lock()
    }

    pub fn try_lock(&self) -> Option<parking_lot::MutexGuard<'_, T>> {
        self.inner.try_lock()
    }
}

pub struct RwLock<T> {
    inner: parking_lot::RwLock<T>,
}

impl<T> RwLock<T> {
    pub fn new(_name: &'static str, value: T) -> Self {
        Self {
            inner: parking_lot::RwLock::new(value),
        }
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, T> {
        self.inner.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, T> {
        self.inner.write()
    }

    pub fn try_read(&self) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        self.inner.try_read()
    }

    pub fn try_write(&self) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        self.inner.try_write()
    }
}

#[cfg(not(feature = "diagnostics"))]
mod disabled;
#[cfg(all(feature = "diagnostics", not(target_arch = "wasm32")))]
mod enabled;

#[cfg(not(feature = "diagnostics"))]
pub use disabled::*;
#[cfg(all(feature = "diagnostics", not(target_arch = "wasm32")))]
pub use enabled::*;
