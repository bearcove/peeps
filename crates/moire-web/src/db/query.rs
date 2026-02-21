use facet::Facet;
use facet_value::Value;
use moire_types::{ScopeEntityLink, SqlResponse};
use rusqlite_facet::ConnectionFacetExt;

use crate::db::Db;

#[derive(Facet)]
struct ScopeEntityLinkParams {
    conn_id: u64,
}

pub fn fetch_scope_entity_links_blocking(
    db: &Db,
    conn_id: u64,
) -> Result<Vec<ScopeEntityLink>, String> {
    let conn = db.open()?;
    conn.facet_query_ref::<ScopeEntityLink, _>(
        "SELECT scope_id, entity_id FROM entity_scope_links WHERE conn_id = :conn_id",
        &ScopeEntityLinkParams { conn_id },
    )
    .map_err(|error| format!("query scope_entity_links: {error}"))
}

pub fn sql_query_blocking(db: &Db, sql: &str) -> Result<SqlResponse, String> {
    let sql = sql.trim();
    if sql.is_empty() {
        return Err("empty SQL".to_string());
    }

    let conn = db.open()?;
    let mut stmt = conn
        .prepare(sql)
        .map_err(|error| format!("prepare sql: {error}"))?;
    if !stmt.readonly() {
        return Err("only read-only statements are allowed".to_string());
    }

    let column_count = stmt.column_count();
    let columns: Vec<String> = (0..column_count)
        .map(|index| String::from(stmt.column_name(index).unwrap_or("?")))
        .collect();

    let mut rows = Vec::new();
    let mut raw_rows = stmt.raw_query();

    loop {
        let Some(row) = raw_rows
            .next()
            .map_err(|error| format!("query row: {error}"))?
        else {
            break;
        };

        let mut row_values = Vec::with_capacity(column_count);
        for index in 0..column_count {
            let value_ref = row
                .get_ref(index)
                .map_err(|error| format!("read column {index}: {error}"))?;
            row_values.push(moire_sqlite_facet::sqlite_value_ref_to_facet(value_ref));
        }
        let row_value: Value = row_values.into_iter().collect();
        rows.push(row_value);
    }

    Ok(SqlResponse {
        columns,
        row_count: rows.len() as u32,
        rows,
    })
}

pub fn query_named_blocking(db: &Db, name: &str, limit: u32) -> Result<SqlResponse, String> {
    let sql = named_query_sql(name, limit)?;
    sql_query_blocking(db, &sql)
}

fn named_query_sql(name: &str, limit: u32) -> Result<String, String> {
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
        "blocked-senders" => Ok(format!(
            "select \
             f.entity_id as send_future_id, \
             json_extract(f.entity_json, '$.name') as send_name, \
             e.dst_id as waiting_on_entity_id, \
             json_extract(ch.entity_json, '$.name') as waiting_on_name, \
             e.updated_at_ns \
             from edges e \
             join entities f on f.conn_id = e.conn_id and f.stream_id = e.stream_id and f.entity_id = e.src_id \
             left join entities ch on ch.conn_id = e.conn_id and ch.stream_id = e.stream_id and ch.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
               and json_extract(f.entity_json, '$.body') = 'future' \
               and json_extract(f.entity_json, '$.name') like '%.send' \
             order by e.updated_at_ns desc \
             limit {limit}"
        )),
        "blocked-receivers" => Ok(format!(
            "select \
             f.entity_id as recv_future_id, \
             json_extract(f.entity_json, '$.name') as recv_name, \
             e.dst_id as waiting_on_entity_id, \
             json_extract(ch.entity_json, '$.name') as waiting_on_name, \
             e.updated_at_ns \
             from edges e \
             join entities f on f.conn_id = e.conn_id and f.stream_id = e.stream_id and f.entity_id = e.src_id \
             left join entities ch on ch.conn_id = e.conn_id and ch.stream_id = e.stream_id and ch.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
               and json_extract(f.entity_json, '$.body') = 'future' \
               and json_extract(f.entity_json, '$.name') like '%.recv' \
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
        "channel-pressure" => Ok(format!(
            "select \
             entity_id, \
             json_extract(entity_json, '$.name') as name, \
             coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.capacity'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.capacity')) as capacity, \
             coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.occupancy'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.occupancy')) as occupancy, \
             case \
               when coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.capacity'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.capacity')) > 0 \
               then cast(coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.occupancy'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.occupancy')) as real) / \
                    cast(coalesce(json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.capacity'), json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.capacity')) as real) \
               else null \
             end as utilization \
             from entities \
             where json_extract(entity_json, '$.body.channel_tx.details.mpsc') is not null \
                or json_extract(entity_json, '$.body.channel_rx.details.mpsc') is not null \
             order by utilization desc, name asc \
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
               json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.capacity'), \
               json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.capacity') \
             ) as capacity, \
             coalesce( \
               json_extract(entity_json, '$.body.channel_tx.details.mpsc.buffer.occupancy'), \
               json_extract(entity_json, '$.body.channel_rx.details.mpsc.buffer.occupancy') \
             ) as occupancy \
             from entities \
             where json_extract(entity_json, '$.body.channel_tx') is not null \
                or json_extract(entity_json, '$.body.channel_rx') is not null \
             order by name \
             limit {limit}"
        )),
        "scope-membership" => Ok(format!(
            "select \
             l.scope_id, \
             json_extract(s.scope_json, '$.name') as scope_name, \
             l.entity_id, \
             json_extract(e.entity_json, '$.name') as entity_name \
             from entity_scope_links l \
             left join scopes s on s.conn_id = l.conn_id and s.stream_id = l.stream_id and s.scope_id = l.scope_id \
             left join entities e on e.conn_id = l.conn_id and e.stream_id = l.stream_id and e.entity_id = l.entity_id \
             order by scope_name asc, entity_name asc \
             limit {limit}"
        )),
        "missing-scope-links" => Ok(format!(
            "select \
             e.conn_id as process_id, \
             c.process_name, \
             c.pid, \
             e.stream_id, \
             e.entity_id, \
             json_extract(e.entity_json, '$.name') as entity_name, \
             json_extract(e.entity_json, '$.body') as entity_body, \
             case \
               when p.process_scope_count is null then 1 \
               else 0 \
             end as missing_process_scope_link, \
             case \
               when json_extract(e.entity_json, '$.body') = 'future' and t.task_scope_count is null then 1 \
               else 0 \
             end as missing_task_scope_link \
             from entities e \
             left join connections c \
               on c.conn_id = e.conn_id \
             left join ( \
               select \
                 l.conn_id, \
                 l.stream_id, \
                 l.entity_id, \
                 count(*) as process_scope_count \
               from entity_scope_links l \
               join scopes s \
                 on s.conn_id = l.conn_id \
                and s.stream_id = l.stream_id \
                and s.scope_id = l.scope_id \
               where json_extract(s.scope_json, '$.body') = 'process' \
               group by l.conn_id, l.stream_id, l.entity_id \
             ) p \
               on p.conn_id = e.conn_id \
              and p.stream_id = e.stream_id \
              and p.entity_id = e.entity_id \
             left join ( \
               select \
                 l.conn_id, \
                 l.stream_id, \
                 l.entity_id, \
                 count(*) as task_scope_count \
               from entity_scope_links l \
               join scopes s \
                 on s.conn_id = l.conn_id \
                and s.stream_id = l.stream_id \
                and s.scope_id = l.scope_id \
               where json_extract(s.scope_json, '$.body') = 'task' \
               group by l.conn_id, l.stream_id, l.entity_id \
             ) t \
               on t.conn_id = e.conn_id \
              and t.stream_id = e.stream_id \
              and t.entity_id = e.entity_id \
             where p.process_scope_count is null \
                or (json_extract(e.entity_json, '$.body') = 'future' and t.task_scope_count is null) \
             order by c.process_name asc, entity_name asc, e.entity_id asc \
             limit {limit}"
        )),
        "stale-blockers" => Ok(format!(
            "select \
             e.src_id as waiter_id, \
             json_extract(src.entity_json, '$.name') as waiter_name, \
             e.dst_id as blocked_on_id, \
             json_extract(dst.entity_json, '$.name') as blocked_on_name, \
             e.updated_at_ns \
             from edges e \
             left join entities src on src.conn_id = e.conn_id and src.stream_id = e.stream_id and src.entity_id = e.src_id \
             left join entities dst on dst.conn_id = e.conn_id and dst.stream_id = e.stream_id and dst.entity_id = e.dst_id \
             where e.kind_json = '\"needs\"' \
             order by e.updated_at_ns asc \
             limit {limit}"
        )),
        _ => Err(format!(
            "unknown query pack: {name}. expected one of: blockers, blocked-senders, blocked-receivers, stalled-sends, channel-pressure, channel-health, scope-membership, missing-scope-links, stale-blockers"
        )),
    }
}
