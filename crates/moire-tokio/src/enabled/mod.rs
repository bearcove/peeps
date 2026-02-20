pub(crate) mod channels;
pub(crate) mod joinset;
pub(crate) mod process;
pub(crate) mod rpc;
pub(crate) mod sync;

pub use self::channels::*;
pub use self::process::*;
pub use self::rpc::*;
pub use self::sync::*;
pub use moire_runtime::*;

use core::sync::atomic::{AtomicU64, Ordering};
use ctor::ctor;
use moire_trace_capture::{capture_current, trace_capabilities, CaptureOptions};
use moire_trace_types::BacktraceId;
use moire_wire::{BacktraceFrameKey, BacktraceRecord};
use std::sync::Once;

static NEXT_BACKTRACE_ID: AtomicU64 = AtomicU64::new(1);
static DIAGNOSTICS_INIT_ONCE: Once = Once::new();

// r[impl process.auto-init]
#[ctor]
fn init_diagnostics_runtime() {
    init_diagnostics_runtime_once();
}

fn init_diagnostics_runtime_once() {
    DIAGNOSTICS_INIT_ONCE.call_once(|| {
        moire_trace_capture::validate_frame_pointers_or_panic();
        init_runtime_from_macro();
    });
}

pub(crate) fn capture_backtrace_id() -> SourceId {
    let capabilities = trace_capabilities();
    assert!(
        capabilities.trace_v1,
        "trace capture prerequisites missing: trace_v1 unsupported on this platform"
    );

    let raw = NEXT_BACKTRACE_ID.fetch_add(1, Ordering::Relaxed);
    let backtrace_id = BacktraceId::new(raw)
        .expect("backtrace id invariant violated: generated id must be non-zero");

    let captured = capture_current(backtrace_id, CaptureOptions::default()).unwrap_or_else(|err| {
        panic!("failed to capture backtrace for enabled API boundary: {err}")
    });
    // r[impl wire.backtrace-record]
    moire_runtime::remember_backtrace_record(BacktraceRecord {
        id: captured.backtrace.id.get(),
        frames: captured
            .backtrace
            .frames
            .into_iter()
            .map(|frame| BacktraceFrameKey {
                module_id: frame.module_id.get(),
                rel_pc: frame.rel_pc,
            })
            .collect(),
    });

    SourceId::new(backtrace_id.get())
}
