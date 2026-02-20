use moire_trace_types::{
    BacktraceId, BacktraceRecord, InvariantError, ModuleId, ModulePath, TraceCapabilities,
};
use std::error::Error;
use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Once;

#[derive(Debug, Clone, Copy)]
pub struct CaptureOptions {
    pub max_frames: NonZeroUsize,
    pub skip_frames: usize,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            max_frames: NonZeroUsize::new(256)
                .expect("invariant violated: default max_frames must be non-zero"),
            skip_frames: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedModule {
    pub id: ModuleId,
    pub path: ModulePath,
    pub runtime_base: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedBacktrace {
    pub backtrace: BacktraceRecord,
    pub modules: Vec<CapturedModule>,
}

#[derive(Debug)]
pub enum CaptureError {
    UnsupportedPlatform {
        target_os: &'static str,
    },
    EmptyBacktrace,
    MissingModuleInfo {
        ip: u64,
    },
    MissingModulePath {
        ip: u64,
    },
    ZeroModuleBase {
        ip: u64,
    },
    IpBeforeModuleBase {
        ip: u64,
        module_base: u64,
    },
    ModuleIdOverflow,
    InvariantViolation {
        context: &'static str,
        source: InvariantError,
    },
}

impl CaptureError {
    fn invariant(context: &'static str, source: InvariantError) -> Self {
        Self::InvariantViolation { context, source }
    }
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform { target_os } => {
                write!(
                    f,
                    "unsupported platform for trace capture backend: {target_os}; only Unix targets are implemented"
                )
            }
            Self::EmptyBacktrace => write!(f, "invariant violated: captured backtrace must be non-empty"),
            Self::MissingModuleInfo { ip } => {
                write!(f, "invariant violated: dladdr returned no module info for ip=0x{ip:x}")
            }
            Self::MissingModulePath { ip } => {
                write!(f, "invariant violated: module path is required for ip=0x{ip:x}")
            }
            Self::ZeroModuleBase { ip } => {
                write!(f, "invariant violated: module base must be non-zero for ip=0x{ip:x}")
            }
            Self::IpBeforeModuleBase { ip, module_base } => write!(
                f,
                "invariant violated: instruction pointer 0x{ip:x} is below module base 0x{module_base:x}"
            ),
            Self::ModuleIdOverflow => write!(f, "invariant violated: module id overflow while capturing backtrace"),
            Self::InvariantViolation { context, source } => {
                write!(f, "invariant violated in {context}: {source}")
            }
        }
    }
}

impl Error for CaptureError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvariantViolation { source, .. } => Some(source),
            _ => None,
        }
    }
}

// r[impl process.frame-pointers]
pub fn trace_capabilities() -> TraceCapabilities {
    TraceCapabilities {
        trace_v1: cfg!(unix) && cfg!(any(target_arch = "x86_64", target_arch = "aarch64")),
        requires_frame_pointers: true,
        sampling_supported: false,
        alloc_tracking_supported: false,
    }
}

static FRAME_POINTER_VALIDATION_ONCE: Once = Once::new();

// r[impl process.frame-pointer-validation]
pub fn validate_frame_pointers_or_panic() {
    FRAME_POINTER_VALIDATION_ONCE.call_once(|| {
        if let Err(reason) = platform::validate_frame_pointers_impl() {
            panic!(
                "frame-pointer validation failed: {reason}. \
recompile with -C force-frame-pointers=yes"
            );
        }
    });
}

// r[impl process.backtrace-capture]
pub fn capture_current(
    backtrace_id: BacktraceId,
    options: CaptureOptions,
) -> Result<CapturedBacktrace, CaptureError> {
    platform::capture_current_impl(backtrace_id, options)
}

#[cfg(unix)]
mod platform {
    use super::{CaptureError, CaptureOptions, CapturedBacktrace, CapturedModule};
    use moire_trace_types::{BacktraceId, BacktraceRecord, FrameKey, ModuleId, ModulePath};
    use std::collections::BTreeMap;
    use std::ffi::{c_void, CStr};

    pub fn validate_frame_pointers_impl() -> Result<(), String> {
        #[inline(never)]
        fn layer0() -> Result<(), String> {
            layer1()
        }
        #[inline(never)]
        fn layer1() -> Result<(), String> {
            layer2()
        }
        #[inline(never)]
        fn layer2() -> Result<(), String> {
            layer3()
        }
        #[inline(never)]
        fn layer3() -> Result<(), String> {
            layer4()
        }
        #[inline(never)]
        fn layer4() -> Result<(), String> {
            validate_frame_pointer_chain(6)
        }

        layer0()
    }

    fn validate_frame_pointer_chain(min_depth: usize) -> Result<(), String> {
        let mut frame_ptr = read_frame_pointer()?;
        if frame_ptr == 0 {
            return Err("current frame pointer is null".to_string());
        }

        let mut prev_frame_ptr = 0usize;
        let mut depth = 0usize;
        const MAX_FRAMES: usize = 4096;

        for _ in 0..MAX_FRAMES {
            if frame_ptr == 0 {
                break;
            }

            if frame_ptr % std::mem::align_of::<usize>() != 0 {
                return Err(format!("misaligned frame pointer 0x{frame_ptr:x}"));
            }

            if prev_frame_ptr != 0 && frame_ptr <= prev_frame_ptr {
                return Err(format!(
                    "frame pointer did not increase: current=0x{frame_ptr:x}, previous=0x{prev_frame_ptr:x}"
                ));
            }

            let next_frame_ptr = unsafe { *(frame_ptr as *const usize) };
            depth += 1;
            if next_frame_ptr == 0 {
                break;
            }

            prev_frame_ptr = frame_ptr;
            frame_ptr = next_frame_ptr;
        }

        if depth < min_depth {
            return Err(format!(
                "frame pointer chain too shallow: got {depth}, need at least {min_depth}"
            ));
        }

        Ok(())
    }

    #[cfg(target_arch = "x86_64")]
    fn read_frame_pointer() -> Result<usize, String> {
        let frame_ptr: usize;
        unsafe {
            core::arch::asm!(
                "mov {}, rbp",
                out(reg) frame_ptr,
                options(nomem, nostack, preserves_flags)
            );
        }
        Ok(frame_ptr)
    }

    #[cfg(target_arch = "aarch64")]
    fn read_frame_pointer() -> Result<usize, String> {
        let frame_ptr: usize;
        unsafe {
            core::arch::asm!(
                "mov {}, x29",
                out(reg) frame_ptr,
                options(nomem, nostack, preserves_flags)
            );
        }
        Ok(frame_ptr)
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    fn read_frame_pointer() -> Result<usize, String> {
        Err(format!(
            "unsupported architecture for frame pointer validation: {}",
            std::env::consts::ARCH
        ))
    }

    // r[impl process.backtrace-capture.impl]
    pub fn capture_current_impl(
        backtrace_id: BacktraceId,
        options: CaptureOptions,
    ) -> Result<CapturedBacktrace, CaptureError> {
        let raw_ips = collect_raw_ips(options)?;

        if raw_ips.is_empty() {
            return Err(CaptureError::EmptyBacktrace);
        }

        let mut modules_by_key: BTreeMap<(u64, String), ModuleId> = BTreeMap::new();
        let mut modules = Vec::new();
        let mut frames = Vec::with_capacity(raw_ips.len());
        let mut next_module_id = 1u64;

        for ip in raw_ips {
            let module = module_info_for_ip(ip)?;

            let key = (module.runtime_base, module.path.clone());
            let module_id = if let Some(module_id) = modules_by_key.get(&key).copied() {
                module_id
            } else {
                let id_value = next_module_id;
                next_module_id = next_module_id
                    .checked_add(1)
                    .ok_or(CaptureError::ModuleIdOverflow)?;

                let module_id = ModuleId::new(id_value)
                    .map_err(|err| CaptureError::invariant("module_id", err))?;
                let module_path = ModulePath::new(module.path)
                    .map_err(|err| CaptureError::invariant("module_path", err))?;

                modules.push(CapturedModule {
                    id: module_id,
                    path: module_path,
                    runtime_base: module.runtime_base,
                });

                modules_by_key.insert(key, module_id);
                module_id
            };

            if ip < module.runtime_base {
                return Err(CaptureError::IpBeforeModuleBase {
                    ip,
                    module_base: module.runtime_base,
                });
            }

            frames.push(FrameKey {
                module_id,
                rel_pc: ip - module.runtime_base,
            });
        }

        let backtrace = BacktraceRecord::new(backtrace_id, frames)
            .map_err(|err| CaptureError::invariant("backtrace_record", err))?;

        Ok(CapturedBacktrace { backtrace, modules })
    }

    fn collect_raw_ips(options: CaptureOptions) -> Result<Vec<u64>, CaptureError> {
        let mut raw_ips = Vec::new();
        let mut skip_remaining = options.skip_frames;
        let mut frame_ptr =
            read_frame_pointer().map_err(|_| CaptureError::UnsupportedPlatform {
                target_os: std::env::consts::OS,
            })?;

        while frame_ptr != 0 && raw_ips.len() < options.max_frames.get() {
            if frame_ptr % std::mem::align_of::<usize>() != 0 {
                break;
            }

            let next_frame_ptr = unsafe { *(frame_ptr as *const usize) };
            let return_ip = unsafe { *((frame_ptr as *const usize).add(1)) };

            if return_ip != 0 {
                if skip_remaining > 0 {
                    skip_remaining -= 1;
                } else {
                    raw_ips.push(return_ip as u64);
                }
            }

            if next_frame_ptr == 0 || next_frame_ptr <= frame_ptr {
                break;
            }

            frame_ptr = next_frame_ptr;
        }

        Ok(raw_ips)
    }

    struct RawModuleInfo {
        runtime_base: u64,
        path: String,
    }

    fn module_info_for_ip(ip: u64) -> Result<RawModuleInfo, CaptureError> {
        let mut info = std::mem::MaybeUninit::<libc::Dl_info>::zeroed();
        let ok = unsafe { libc::dladdr(ip as usize as *const c_void, info.as_mut_ptr()) };
        if ok == 0 {
            return Err(CaptureError::MissingModuleInfo { ip });
        }

        let info = unsafe { info.assume_init() };
        if info.dli_fbase.is_null() {
            return Err(CaptureError::ZeroModuleBase { ip });
        }

        let runtime_base = info.dli_fbase as usize as u64;
        if runtime_base == 0 {
            return Err(CaptureError::ZeroModuleBase { ip });
        }

        if info.dli_fname.is_null() {
            return Err(CaptureError::MissingModulePath { ip });
        }

        let path = unsafe { CStr::from_ptr(info.dli_fname) }
            .to_string_lossy()
            .into_owned();
        if path.is_empty() {
            return Err(CaptureError::MissingModulePath { ip });
        }

        Ok(RawModuleInfo { runtime_base, path })
    }
}

#[cfg(not(unix))]
mod platform {
    use super::{CaptureError, CaptureOptions, CapturedBacktrace};
    use moire_trace_types::BacktraceId;

    pub fn validate_frame_pointers_impl() -> Result<(), String> {
        Err(format!(
            "unsupported platform for trace capture backend: {}",
            std::env::consts::OS
        ))
    }

    pub fn capture_current_impl(
        _backtrace_id: BacktraceId,
        _options: CaptureOptions,
    ) -> Result<CapturedBacktrace, CaptureError> {
        Err(CaptureError::UnsupportedPlatform {
            target_os: std::env::consts::OS,
        })
    }
}
