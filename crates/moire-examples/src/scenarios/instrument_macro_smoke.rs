#![allow(dead_code)]

use std::future::Future;

#[moire::instrument]
pub async fn async_instrumented(value: u64) -> u64 {
    value + 1
}

#[moire::instrument]
pub fn impl_future_instrumented(value: u64) -> impl Future<Output = u64> {
    async move { value + 2 }
}
