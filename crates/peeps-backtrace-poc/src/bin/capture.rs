use peeps_backtrace_poc::{FrameRecord, TraceBundle, TraceRecord};
use peeps_backtrace_poc_leaf::pipeline::stage_one::stage_two;
use peeps_backtrace_poc_router::{alpha_path, beta_path};
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let out_path = parse_out_path()?;

    let traces = vec![
        alpha_path(),
        beta_path(),
        stage_two::collect_here("leaf_direct"),
    ];

    let trace_records: Vec<TraceRecord> = traces
        .into_iter()
        .map(|trace| TraceRecord {
            label: trace.label,
            frames: trace
                .frames
                .into_iter()
                .map(|frame| FrameRecord {
                    ip: frame.ip,
                    module_base: frame.module_base,
                    module_path: frame.module_path,
                })
                .collect(),
        })
        .collect();

    if trace_records.iter().any(|r| r.frames.is_empty()) {
        return Err("invariant violated: trace record had zero frames".to_owned());
    }

    let capture_binary = std::env::current_exe()
        .map_err(|e| format!("failed to resolve current executable path: {e}"))?
        .to_string_lossy()
        .into_owned();

    let bundle = TraceBundle {
        schema_version: 2,
        capture_binary,
        traces: trace_records,
    };

    let encoded = serde_json::to_string_pretty(&bundle)
        .map_err(|e| format!("failed to encode trace bundle as JSON: {e}"))?;
    fs::write(&out_path, encoded).map_err(|e| {
        format!(
            "failed to write trace bundle to {}: {e}",
            out_path.display()
        )
    })?;

    println!(
        "captured {} traces into {}",
        bundle.traces.len(),
        out_path.display()
    );

    Ok(())
}

fn parse_out_path() -> Result<PathBuf, String> {
    let mut args = std::env::args().skip(1);
    let flag = args
        .next()
        .ok_or_else(|| "usage: capture --out <trace_bundle.json>".to_owned())?;

    if flag != "--out" {
        return Err(format!(
            "expected first argument to be --out, got {flag:?}; usage: capture --out <trace_bundle.json>"
        ));
    }

    let out = args.next().ok_or_else(|| {
        "missing output path; usage: capture --out <trace_bundle.json>".to_owned()
    })?;

    if args.next().is_some() {
        return Err(
            "unexpected trailing arguments; usage: capture --out <trace_bundle.json>".to_owned(),
        );
    }

    Ok(PathBuf::from(out))
}
