use compact_str::CompactString;
use facet::Facet;
use std::time::{Duration, Instant};

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:9130";
const DEFAULT_POLL_MS: u64 = 100;
const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_QUERY_LIMIT: u32 = 50;

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
        "query" => run_query_pack(args),
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

fn run_query_pack(args: Vec<String>) -> Result<(), String> {
    let mut base_url = DEFAULT_BASE_URL.to_string();
    let mut name: Option<String> = None;
    let mut limit: u32 = DEFAULT_QUERY_LIMIT;
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
            "--name" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err("missing value for --name".to_string());
                };
                name = Some(value.clone());
            }
            "--limit" => {
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err("missing value for --limit".to_string());
                };
                limit = value
                    .parse::<u32>()
                    .map_err(|e| format!("invalid --limit: {e}"))?;
            }
            "--help" | "-h" => {
                println!("{}", query_usage());
                return Ok(());
            }
            other => {
                return Err(format!(
                    "unknown flag for query: {other}\n\n{}",
                    query_usage()
                ))
            }
        }
        i += 1;
    }

    let Some(name) = name else {
        return Err(format!("missing --name\n\n{}", query_usage()));
    };
    let sql = query_sql(&name, limit)?;
    run_sql(vec![
        "--url".to_string(),
        base_url,
        "--query".to_string(),
        sql,
    ])
}

fn query_sql(name: &str, limit: u32) -> Result<String, String> {
    match name {
        "blockers" => Ok(format!(
            "select \
             e.src_id as waiter_id, \
             json_extract(src.entity_json, '$.name') as waiter_name, \
             e.dst_id as blocked_on_id, \
             json_extract(dst.entity_json, '$.name') as blocked_on_name, \
             e.kind_json \
             from edges e \
             left join entities src on src.conn_id = e.conn_id and src.stream_id = e.stream_id and src.entity_id = e.src_id \
             left join entities dst on dst.conn_id = e.conn_id and dst.stream_id = e.stream_id and dst.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
             order by e.updated_at_ns desc \
             limit {limit}"
        )),
        "stalled-sends" => Ok(format!(
            "select \
             f.entity_id as send_future_id, \
             json_extract(f.entity_json, '$.name') as send_name, \
             e.dst_id as waiting_on_entity_id, \
             json_extract(ch.entity_json, '$.name') as waiting_on_name \
             from edges e \
             join entities f on f.conn_id = e.conn_id and f.stream_id = e.stream_id and f.entity_id = e.src_id \
             left join entities ch on ch.conn_id = e.conn_id and ch.stream_id = e.stream_id and ch.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
               and json_extract(f.entity_json, '$.body') = 'future' \
               and json_extract(f.entity_json, '$.name') like '%.send' \
             order by e.updated_at_ns desc \
             limit {limit}"
        )),
        "channel-health" => Ok(format!(
            "select \
             entity_id, \
             json_extract(entity_json, '$.name') as name, \
             coalesce( \
               json_extract(entity_json, '$.body.channel_tx.lifecycle'), \
               json_extract(entity_json, '$.body.channel_rx.lifecycle') \
             ) as lifecycle, \
             coalesce( \
               json_extract(entity_json, '$.body.channel_tx.details.mpsc.capacity'), \
               json_extract(entity_json, '$.body.channel_rx.details.mpsc.capacity') \
             ) as capacity, \
             coalesce( \
               json_extract(entity_json, '$.body.channel_tx.details.mpsc.queue_len'), \
               json_extract(entity_json, '$.body.channel_rx.details.mpsc.queue_len') \
             ) as queue_len \
             from entities \
             where json_extract(entity_json, '$.body.channel_tx') is not null \
                or json_extract(entity_json, '$.body.channel_rx') is not null \
             order by name \
             limit {limit}"
        )),
        _ => Err(format!(
            "unknown query pack: {name}. expected one of: blockers, stalled-sends, channel-health"
        )),
    }
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
        "peeps-cli commands:\n  cut [--url URL] [--poll-ms N] [--timeout-ms N]\n  sql --query \"...\" [--url URL]\n  query --name <blockers|stalled-sends|channel-health> [--url URL]\n\n{}",
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

fn query_usage() -> String {
    format!(
        "peeps-cli query --name <blockers|stalled-sends|channel-health> [--url URL]\n\n{}",
        defaults_usage()
    )
}

fn defaults_usage() -> String {
    format!(
        "defaults:\n  --url {}\n  --poll-ms {}\n  --timeout-ms {}\n  --limit {}",
        DEFAULT_BASE_URL, DEFAULT_POLL_MS, DEFAULT_TIMEOUT_MS, DEFAULT_QUERY_LIMIT
    )
}
