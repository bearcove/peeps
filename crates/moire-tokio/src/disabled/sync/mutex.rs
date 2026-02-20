use std::ops::Deref;

/// Pass-through `parking_lot::Mutex` wrapper, accepting a name parameter for API parity.
pub struct Mutex<T>(parking_lot::Mutex<T>);

pub use parking_lot::MutexGuard;

impl<T> Mutex<T> {
    pub fn new(_name: &'static str, value: T) -> Self {
        Self(parking_lot::Mutex::new(value))
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.0.lock()
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.0.try_lock()
    }
}

impl<T> Deref for Mutex<T> {
    type Target = parking_lot::Mutex<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
