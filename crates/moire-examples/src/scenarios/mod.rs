pub mod channel_full_stall;
pub mod instrument_macro_smoke;
pub mod mutex_lock_order_inversion;
pub mod oneshot_sender_lost_in_map;
// TODO: disabled until roam crate re-exports the types that #[roam::service] macro expects
#[cfg(any())]
pub mod roam_rpc_stuck_request;
#[cfg(any())]
pub mod roam_rust_swift_stuck_request;
pub mod semaphore_starvation;
