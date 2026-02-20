use backtrace::trace;
use std::ffi::c_void;
use std::ffi::CStr;

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
    let mut frames = Vec::new();

    trace(|frame| {
        let ip = frame.ip() as usize;
        let module_info = module_info_for_ip(ip as *const c_void);
        frames.push(CapturedFrame {
            ip: ip as u64,
            module_base: module_info.as_ref().map(|info| info.base),
            module_path: module_info.and_then(|info| info.path),
        });

        true
    });

    if frames.is_empty() {
        panic!("invariant violated: no frames collected");
    }

    CapturedTrace {
        label: label.to_owned(),
        frames,
    }
}

struct ModuleInfo {
    base: u64,
    path: Option<String>,
}

fn module_info_for_ip(ip: *const c_void) -> Option<ModuleInfo> {
    let mut info = std::mem::MaybeUninit::<libc::Dl_info>::zeroed();
    let ok = unsafe { libc::dladdr(ip, info.as_mut_ptr()) };
    if ok == 0 {
        return None;
    }

    let info = unsafe { info.assume_init() };
    if info.dli_fbase.is_null() {
        return None;
    }

    let path = if info.dli_fname.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(info.dli_fname) }
                .to_string_lossy()
                .into_owned(),
        )
    };

    Some(ModuleInfo {
        base: info.dli_fbase as usize as u64,
        path,
    })
}
