use ctor::ctor;
use std::sync::Once;

pub mod custom;
pub mod process;
pub mod rpc;
pub mod sync;
pub mod task;
pub mod time;

pub use task::{spawn, spawn_blocking};

static DASHBOARD_DISABLED_WARNING_ONCE: Once = Once::new();

#[ctor]
fn init_disabled_runtime() {
    emit_disabled_dashboard_warning_once();
}

fn emit_disabled_dashboard_warning_once() {
    // r[impl config.dashboard-feature-gate]
    let Some(value) = std::env::var_os("MOIRE_DASHBOARD") else {
        return;
    };
    if value.to_string_lossy().trim().is_empty() {
        return;
    }

    DASHBOARD_DISABLED_WARNING_ONCE.call_once(|| {
        eprintln!(
            "\n\x1b[1;31m\
======================================================================\n\
 MOIRE WARNING: MOIRE_DASHBOARD is set, but moire diagnostics is disabled.\n\
 This process will NOT connect to moire-web in this build.\n\
 Enable the `diagnostics` cargo feature of `moire` to use dashboard push.\n\
======================================================================\x1b[0m\n"
        );
    });
}
