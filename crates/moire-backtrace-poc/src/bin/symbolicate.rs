use addr2line::Loader;
use moire_backtrace_poc::TraceBundle;
use object::{Object, ObjectSegment};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const CYAN: &str = "\x1b[36m";

fn main() {
    if let Err(err) = run() {
        eprintln!("{}error:{} {err}", RED, RESET);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let bundle = read_bundle(&args.input)?;
    if bundle.schema_version != 2 {
        return Err(format!(
            "unsupported schema version {}; expected 2",
            bundle.schema_version
        ));
    }

    if bundle.traces.is_empty() {
        return Err("invariant violated: trace bundle contained zero traces".to_owned());
    }

    println!(
        "{}{}Trace Bundle{} {}",
        BOLD,
        BLUE,
        RESET,
        args.input.display()
    );

    let mut module_cache: HashMap<String, ModuleResolver> = HashMap::new();

    for trace in &bundle.traces {
        if trace.frames.is_empty() {
            return Err(format!(
                "invariant violated: trace '{}' contains zero frames",
                trace.label
            ));
        }

        println!("\n{}{}Trace:{} {}", BOLD, BLUE, RESET, trace.label);
        println!("{}frames:{} {}", DIM, RESET, trace.frames.len());

        for (idx, frame) in trace.frames.iter().enumerate() {
            let module_path = match &frame.module_path {
                Some(path) => path,
                None => {
                    println!(
                        "{}#{idx:02}{} {}<unresolved>{} ip=0x{:016x} reason=no_module_path",
                        YELLOW, RESET, RED, RESET, frame.ip
                    );
                    continue;
                }
            };

            let runtime_base = match frame.module_base {
                Some(base) => base,
                None => {
                    println!(
                        "{}#{idx:02}{} {}<unresolved>{} ip=0x{:016x} module={} reason=no_module_base",
                        YELLOW, RESET, RED, RESET, frame.ip, module_path
                    );
                    continue;
                }
            };

            if !module_cache.contains_key(module_path) {
                let resolver = match ModuleResolver::new(module_path) {
                    Ok(resolver) => resolver,
                    Err(err) => {
                        println!(
                            "{}#{idx:02}{} {}<unresolved>{} ip=0x{:016x} module={} reason={}",
                            YELLOW, RESET, RED, RESET, frame.ip, module_path, err
                        );
                        continue;
                    }
                };
                module_cache.insert(module_path.clone(), resolver);
            }

            let resolver = module_cache
                .get(module_path)
                .expect("invariant violated: module cache insert/get mismatch");

            let module_offset = match frame.ip.checked_sub(runtime_base) {
                Some(offset) => offset,
                None => {
                    return Err(format!(
                        "invariant violated: frame ip 0x{:x} is below module base 0x{:x} for module {}",
                        frame.ip, runtime_base, module_path
                    ));
                }
            };

            let probe = resolver
                .linked_image_base
                .checked_add(module_offset)
                .ok_or_else(|| {
                    format!(
                        "address overflow combining linked image base 0x{:x} with offset 0x{:x} for module {}",
                        resolver.linked_image_base, module_offset, module_path
                    )
                })?;

            let (function_name, source_path, line, column, absolute_flag) =
                resolver.resolve(probe)?;
            let (crate_name, module_name) = split_crate_and_module(&function_name);
            let source_link = format_source_link(&source_path, line, column, absolute_flag);

            println!(
                "{}#{idx:02}{} {}{}{}",
                YELLOW, RESET, GREEN, function_name, RESET
            );
            println!("  {}at{} {}", BLUE, RESET, source_link);
            println!(
                "  {}crate{} {}  {}module{} {}",
                CYAN, RESET, crate_name, CYAN, RESET, module_name
            );
            println!(
                "  {}ip{} 0x{:016x}  {}base{} 0x{:016x}  {}offset{} 0x{:016x}  {}probe{} 0x{:016x}",
                DIM,
                RESET,
                frame.ip,
                DIM,
                RESET,
                runtime_base,
                DIM,
                RESET,
                module_offset,
                DIM,
                RESET,
                probe
            );
            println!(
                "  {}binary{} {}  {}abs{} {}",
                DIM, RESET, module_path, DIM, RESET, absolute_flag
            );
        }
    }

    Ok(())
}

struct ModuleResolver {
    loader: Loader,
    linked_image_base: u64,
}

impl ModuleResolver {
    fn new(path: &str) -> Result<Self, String> {
        let module_path = PathBuf::from(path);
        if !module_path.is_file() {
            return Err(format!(
                "module path from trace is not a file: {}",
                module_path.display()
            ));
        }

        let loader = Loader::new(&module_path).map_err(|e| {
            format!(
                "failed to load debug data for module {}: {e}",
                module_path.display()
            )
        })?;

        let linked_image_base = linked_image_base_for_file(&module_path)?;

        Ok(Self {
            loader,
            linked_image_base,
        })
    }

    fn resolve(
        &self,
        probe: u64,
    ) -> Result<(String, String, Option<u32>, Option<u32>, bool), String> {
        let mut function_name = "<no-function-symbol>".to_owned();
        let mut source_path = "<no-source-location>".to_owned();
        let mut line = None;
        let mut column = None;
        let mut absolute_flag = false;

        let mut frames = self
            .loader
            .find_frames(probe)
            .map_err(|e| format!("find_frames failed at 0x{probe:x}: {e}"))?;

        while let Some(frame) = frames
            .next()
            .map_err(|e| format!("iterating frames failed at 0x{probe:x}: {e}"))?
        {
            if function_name == "<no-function-symbol>" {
                if let Some(func) = frame.function {
                    if let Ok(demangled) = func.demangle() {
                        function_name = demangled.into_owned();
                    } else if let Ok(raw) = func.raw_name() {
                        function_name = raw.into_owned();
                    }
                }
            }

            if source_path == "<no-source-location>" {
                if let Some(location) = frame.location {
                    if let Some(file) = location.file {
                        source_path = file.to_owned();
                        absolute_flag = Path::new(file).is_absolute();
                    }
                    line = location.line;
                    column = location.column;
                }
            }

            if function_name != "<no-function-symbol>" && source_path != "<no-source-location>" {
                break;
            }
        }

        if function_name == "<no-function-symbol>" {
            if let Some(symbol) = self.loader.find_symbol(probe) {
                function_name = strip_rust_hash_suffix(symbol).to_owned();
            }
        }

        if source_path == "<no-source-location>" {
            match self.loader.find_location(probe) {
                Ok(Some(location)) => {
                    if let Some(file) = location.file {
                        source_path = file.to_owned();
                        absolute_flag = Path::new(file).is_absolute();
                    }
                    line = location.line;
                    column = location.column;
                }
                Ok(None) => {}
                Err(e) => return Err(format!("find_location failed at 0x{probe:x}: {e}")),
            }
        }

        Ok((function_name, source_path, line, column, absolute_flag))
    }
}

fn linked_image_base_for_file(path: &Path) -> Result<u64, String> {
    let data = fs::read(path)
        .map_err(|e| format!("failed to read module file {}: {e}", path.display()))?;
    let object = object::File::parse(&*data)
        .map_err(|e| format!("failed to parse object file {}: {e}", path.display()))?;

    object
        .segments()
        .filter_map(|seg| {
            let (_, file_size) = seg.file_range();
            if file_size == 0 {
                return None;
            }
            Some(seg.address())
        })
        .min()
        .ok_or_else(|| {
            format!(
                "failed to determine linked image base for module {}: no file-backed segments",
                path.display()
            )
        })
}

struct Args {
    input: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut args = std::env::args().skip(1);

    let flag = args.next().ok_or_else(|| {
        "missing arguments; usage: symbolicate --input <trace_bundle.json>".to_owned()
    })?;
    if flag != "--input" {
        return Err(format!(
            "unknown argument {flag:?}; usage: symbolicate --input <trace_bundle.json>"
        ));
    }

    let input = args
        .next()
        .ok_or_else(|| "--input requires a path value".to_owned())?;

    if args.next().is_some() {
        return Err(
            "unexpected trailing arguments; usage: symbolicate --input <trace_bundle.json>"
                .to_owned(),
        );
    }

    Ok(Args {
        input: PathBuf::from(input),
    })
}

fn read_bundle(path: &Path) -> Result<TraceBundle, String> {
    let data = fs::read_to_string(path)
        .map_err(|e| format!("failed to read trace bundle {}: {e}", path.display()))?;
    serde_json::from_str::<TraceBundle>(&data)
        .map_err(|e| format!("failed to parse trace bundle JSON {}: {e}", path.display()))
}

fn split_crate_and_module(function_name: &str) -> (String, String) {
    let cleaned = strip_rust_hash_suffix(function_name);
    let mut parts = cleaned.split("::");

    let crate_name = parts.next().unwrap_or("<unknown-crate>").to_owned();

    let mut all_parts: Vec<&str> = cleaned.split("::").collect();
    if all_parts.len() >= 2 {
        all_parts.pop();
    }
    let module_path = if all_parts.is_empty() {
        "<unknown-module>".to_owned()
    } else {
        all_parts.join("::")
    };

    (crate_name, module_path)
}

fn strip_rust_hash_suffix(name: &str) -> &str {
    if let Some(idx) = name.rfind("::h") {
        let suffix = &name[idx + 3..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
            return &name[..idx];
        }
    }
    name
}

fn hyperlink(label: &str, target: &str) -> String {
    format!("\x1b]8;;{target}\x1b\\{label}\x1b]8;;\x1b\\")
}

fn format_source_link(
    path: &str,
    line: Option<u32>,
    column: Option<u32>,
    absolute: bool,
) -> String {
    let mut label = String::from(path);
    if let Some(line) = line {
        label.push(':');
        label.push_str(line.to_string().as_str());
        if let Some(column) = column {
            label.push(':');
            label.push_str(column.to_string().as_str());
        }
    }
    if absolute {
        return hyperlink(label.as_str(), format!("file://{path}").as_str());
    }
    label
}
