use moire_trace_capture::{CaptureOptions, capture_current};
use moire_trace_types::{BacktraceId, ModuleId};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;

#[derive(Debug, Clone)]
pub struct CapturedTrace {
    pub label: String,
    pub frames: Vec<CapturedFrame>,
}

#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub ip: u64,
    pub module_base: Option<u64>,
    pub module_path: Option<String>,
}

pub mod pipeline {
    use super::CapturedTrace;

    pub mod stage_one {
        use super::CapturedTrace;

        pub mod stage_two {
            use super::CapturedTrace;

            #[inline(never)]
            pub fn collect_here(label: &str) -> CapturedTrace {
                super::super::super::capture_trace(label)
            }
        }
    }
}

#[inline(never)]
fn capture_trace(label: &str) -> CapturedTrace {
    moire_trace_capture::validate_frame_pointers_or_panic();

    let backtrace_id =
        BacktraceId::next().expect("invariant violated: generated backtrace id must be valid");

    let captured = capture_current(
        backtrace_id,
        CaptureOptions {
            max_frames: NonZeroUsize::new(1024)
                .expect("invariant violated: max_frames must be non-zero"),
            skip_frames: 1,
        },
    )
    .expect("invariant violated: frame-pointer capture failed in PoC");

    let modules_by_id: BTreeMap<ModuleId, (u64, String)> = captured
        .modules
        .iter()
        .map(|module| {
            (
                module.id,
                (module.runtime_base, module.path.as_str().to_string()),
            )
        })
        .collect();

    let frames: Vec<CapturedFrame> = captured
        .backtrace
        .frames
        .iter()
        .map(|frame| {
            let (module_base, module_path) = modules_by_id
                .get(&frame.module_id)
                .expect("invariant violated: frame references unknown module id");
            let ip = module_base
                .checked_add(frame.rel_pc)
                .expect("invariant violated: ip overflow");

            CapturedFrame {
                ip,
                module_base: Some(*module_base),
                module_path: Some(module_path.clone()),
            }
        })
        .collect();

    if frames.is_empty() {
        panic!("invariant violated: no frames collected");
    }

    CapturedTrace {
        label: label.to_owned(),
        frames,
    }
}
