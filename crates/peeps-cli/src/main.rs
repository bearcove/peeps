use compact_str::CompactString;
use facet::Facet;
use std::time::{Duration, Instant};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:9130";
const DEFAULT_POLL_MS: u64 = 100;
const DEFAULT_TIMEOUT_MS: u64 = 5_000;

#[derive(Facet)]
struct TriggerCutResponse {
    cut_id: CompactString,
    requested_at_ns: i64,
    requested_connections: usize,
}

#[derive(Facet)]
struct CutStatusResponse {
    cut_id: CompactString,
    requested_at_ns: i64,
    pending_connections: usize,
    acked_connections: usize,
    pending_conn_ids: Vec<u64>,
}

#[derive(Facet)]
struct SqlRequest {
    sql: CompactString,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return Err(usage());
    }

    let command = args.remove(0);
    match command.as_str() {
        "cut" => run_cut(args),
        "sql" => run_sql(args),
        "-h" | "--help" | "help" => {
            println!("{}", usage());
            Ok(())
        }
        other => Err(format!("unknown command: {other}\n\n{}", usage())),
    }
}

fn run_cut(args: Vec<String>) -> Result<(), String> {
    let mut base_url = DEFAULT_BASE_URL.to_string();
    let mut poll_ms = DEFAULT_POLL_MS;
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--url" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err("missing value for --url".to_string());
                };
                base_url = value.clone();
            }
            "--poll-ms" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err("missing value for --poll-ms".to_string());
                };
                poll_ms = value
                    .parse::<u64>()
                    .map_err(|e| format!("invalid --poll-ms: {e}"))?;
            }
            "--timeout-ms" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err("missing value for --timeout-ms".to_string());
                };
                timeout_ms = value
                    .parse::<u64>()
                    .map_err(|e| format!("invalid --timeout-ms: {e}"))?;
            }
            "--help" | "-h" => {
                println!("{}", cut_usage());
                return Ok(());
            }
            other => return Err(format!("unknown flag for cut: {other}\n\n{}", cut_usage())),
        }
        i += 1;
    }

    let trigger_url = format!("{}/api/cuts", base_url.trim_end_matches('/'));
    let trigger_body = http_post_json(&trigger_url, "{}")?;
    let trigger: TriggerCutResponse = facet_json::from_str(&trigger_body)
        .map_err(|e| format!("decode cut trigger response: {e}"))?;

    let status_url = format!(
        "{}/api/cuts/{}",
        base_url.trim_end_matches('/'),
        trigger.cut_id
    );
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let status_body = http_get_text(&status_url)?;
        let status: CutStatusResponse = facet_json::from_str(&status_body)
            .map_err(|e| format!("decode cut status response: {e}"))?;
        if status.pending_connections == 0 {
            println!(
                "{}",
                facet_json::to_string_pretty(&status)
                    .map_err(|e| format!("encode cut status: {e}"))?
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "cut {} timed out after {}ms (pending_connections={})",
                status.cut_id, timeout_ms, status.pending_connections
            ));
        }
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
}

fn run_sql(args: Vec<String>) -> Result<(), String> {
    let mut base_url = DEFAULT_BASE_URL.to_string();
    let mut query: Option<String> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--url" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err("missing value for --url".to_string());
                };
                base_url = value.clone();
            }
            "--query" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err("missing value for --query".to_string());
                };
                query = Some(value.clone());
            }
            "--help" | "-h" => {
                println!("{}", sql_usage());
                return Ok(());
            }
            other => return Err(format!("unknown flag for sql: {other}\n\n{}", sql_usage())),
        }
        i += 1;
    }

    let Some(query) = query else {
        return Err(format!("missing --query\n\n{}", sql_usage()));
    };

    let req = SqlRequest {
        sql: CompactString::from(query),
    };
    let body = facet_json::to_string(&req).map_err(|e| format!("encode sql request: {e}"))?;
    let url = format!("{}/api/sql", base_url.trim_end_matches('/'));
    let response = http_post_json(&url, &body)?;
    let pretty = facet_json::to_string_pretty(
        &facet_json::from_str::<facet_value::Value>(&response)
            .map_err(|e| format!("decode sql response as json: {e}"))?,
    )
    .map_err(|e| format!("pretty sql response: {e}"))?;
    println!("{pretty}");
    Ok(())
}

fn http_get_text(url: &str) -> Result<String, String> {
    let response = ureq::get(url)
        .call()
        .map_err(|e| format!("GET {url}: {e}"))?;
    response
        .into_string()
        .map_err(|e| format!("read GET response body: {e}"))
}

fn http_post_json(url: &str, body: &str) -> Result<String, String> {
    let response = ureq::post(url)
        .set("content-type", "application/json")
        .send_string(body)
        .map_err(|e| format!("POST {url}: {e}"))?;
    response
        .into_string()
        .map_err(|e| format!("read POST response body: {e}"))
}

fn usage() -> String {
    format!(
        "peeps-cli commands:\n  cut [--url URL] [--poll-ms N] [--timeout-ms N]\n  sql --query \"...\" [--url URL]\n\n{}",
        defaults_usage()
    )
}

fn cut_usage() -> String {
    format!(
        "peeps-cli cut [--url URL] [--poll-ms N] [--timeout-ms N]\n\n{}",
        defaults_usage()
    )
}

fn sql_usage() -> String {
    format!(
        "peeps-cli sql --query \"...\" [--url URL]\n\n{}",
        defaults_usage()
    )
}

fn defaults_usage() -> String {
    format!(
        "defaults:\n  --url {}\n  --poll-ms {}\n  --timeout-ms {}",
        DEFAULT_BASE_URL, DEFAULT_POLL_MS, DEFAULT_TIMEOUT_MS
    )
}
