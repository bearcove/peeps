use std::collections::HashMap;

use peeps_types::{Diagnostics, ProcessDump};

/// Manually collect a diagnostic dump.
pub fn collect_dump(process_name: &str, custom: HashMap<String, String>) -> ProcessDump {
    let timestamp = format_timestamp();

    let tasks = peeps_tasks::snapshot_all_tasks();
    let wake_edges = peeps_tasks::snapshot_wake_edges();
    let future_wake_edges = peeps_tasks::snapshot_future_wake_edges();
    let future_waits = peeps_tasks::snapshot_future_waits();
    let future_spawn_edges = peeps_tasks::snapshot_future_spawn_edges();
    let future_poll_edges = peeps_tasks::snapshot_future_poll_edges();
    let future_resume_edges = peeps_tasks::snapshot_future_resume_edges();
    let threads = peeps_threads::collect_all_thread_stacks();

    #[cfg(feature = "locks")]
    let locks = Some(peeps_locks::snapshot_lock_diagnostics());
    #[cfg(not(feature = "locks"))]
    let locks = None;

    let sync = {
        let snap = peeps_sync::snapshot_all();
        if snap.mpsc_channels.is_empty()
            && snap.oneshot_channels.is_empty()
            && snap.watch_channels.is_empty()
            && snap.once_cells.is_empty()
        {
            None
        } else {
            Some(snap)
        }
    };

    // Collect roam diagnostics from inventory-registered sources
    let all_diags = peeps_types::collect_all_diagnostics();
    let mut roam = None;
    let mut shm = None;
    for diag in all_diags {
        match diag {
            Diagnostics::RoamSession(s) => roam = Some(s),
            Diagnostics::RoamShm(s) => shm = Some(s),
        }
    }

    // Extract cross-process request parent edges from incoming request metadata.
    let request_parents = extract_request_parents(process_name, &roam);
    let future_resource_edges = collect_future_resource_edges(process_name, &future_waits);

    ProcessDump {
        process_name: process_name.to_string(),
        pid: std::process::id(),
        timestamp,
        tasks,
        wake_edges,
        future_wake_edges,
        future_waits,
        threads,
        locks,
        sync,
        roam,
        shm,
        future_spawn_edges,
        future_poll_edges,
        future_resume_edges,
        future_resource_edges,
        request_parents,
        graph: None,
        custom,
    }
}

fn collect_future_resource_edges(
    process_name: &str,
    waits: &[peeps_types::FutureWaitSnapshot],
) -> Vec<peeps_types::FutureResourceEdgeSnapshot> {
    waits
        .iter()
        .map(|w| {
            let resource = classify_resource_ref(process_name, &w.resource);
            peeps_types::FutureResourceEdgeSnapshot {
                future_id: w.future_id,
                resource,
                wait_count: w.pending_count,
                total_wait_secs: w.total_pending_secs,
                last_wait_age_secs: w.last_seen_age_secs,
            }
        })
        .collect()
}

fn classify_resource_ref(process_name: &str, raw: &str) -> peeps_types::ResourceRefSnapshot {
    use peeps_types::{ResourceRefSnapshot, SocketWaitDirection};

    if let Some(fd) = raw.strip_prefix("socket:").and_then(|s| s.parse::<u64>().ok()) {
        return ResourceRefSnapshot::Socket {
            process: process_name.to_string(),
            fd,
            label: Some(raw.to_string()),
            direction: None,
            peer: None,
        };
    }
    if raw.starts_with("socket.") || raw.contains(".socket.") || raw == "socket" {
        let direction = if raw.contains(".read") || raw.contains(".recv") {
            Some(SocketWaitDirection::Readable)
        } else if raw.contains(".write")
            || raw.contains(".send")
            || raw.contains(".flush")
            || raw.contains(".connect")
        {
            Some(SocketWaitDirection::Writable)
        } else {
            None
        };
        return ResourceRefSnapshot::Socket {
            process: process_name.to_string(),
            fd: 0,
            label: Some(raw.to_string()),
            direction,
            peer: None,
        };
    }

    if raw.starts_with("lock:") || raw.starts_with("lock.") {
        return ResourceRefSnapshot::Lock {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("lock:")
                .or_else(|| raw.strip_prefix("lock."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("mpsc:") || raw.starts_with("mpsc.") || raw.starts_with("channel.") {
        return ResourceRefSnapshot::Mpsc {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("mpsc:")
                .or_else(|| raw.strip_prefix("mpsc."))
                .or_else(|| raw.strip_prefix("channel."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("oneshot:") || raw.starts_with("oneshot.") {
        return ResourceRefSnapshot::Oneshot {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("oneshot:")
                .or_else(|| raw.strip_prefix("oneshot."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("watch:") || raw.starts_with("watch.") {
        return ResourceRefSnapshot::Watch {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("watch:")
                .or_else(|| raw.strip_prefix("watch."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("semaphore:") || raw.starts_with("semaphore.") {
        return ResourceRefSnapshot::Semaphore {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("semaphore:")
                .or_else(|| raw.strip_prefix("semaphore."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if raw.starts_with("once_cell:")
        || raw.starts_with("once_cell.")
        || raw.starts_with("oncecell:")
        || raw.starts_with("oncecell.")
    {
        return ResourceRefSnapshot::OnceCell {
            process: process_name.to_string(),
            name: raw
                .strip_prefix("once_cell:")
                .or_else(|| raw.strip_prefix("once_cell."))
                .or_else(|| raw.strip_prefix("oncecell:"))
                .or_else(|| raw.strip_prefix("oncecell."))
                .unwrap_or(raw)
                .to_string(),
        };
    }
    if let Some(id) = raw
        .strip_prefix("roam.channel.")
        .and_then(|s| s.parse::<u64>().ok())
    {
        return ResourceRefSnapshot::RoamChannel {
            process: process_name.to_string(),
            channel_id: id,
        };
    }

    ResourceRefSnapshot::Unknown {
        label: raw.to_string(),
    }
}

/// Extract `RequestParentSnapshot` entries from incoming requests that carry
/// explicit caller identity metadata (`peeps.caller_process`, `peeps.caller_connection`,
/// `peeps.caller_request_id`).
fn extract_request_parents(
    process_name: &str,
    roam: &Option<peeps_types::SessionSnapshot>,
) -> Vec<peeps_types::RequestParentSnapshot> {
    let Some(session) = roam else {
        return vec![];
    };
    let mut parents = Vec::new();
    for conn in &session.connections {
        for req in &conn.in_flight {
            if !matches!(req.direction, peeps_types::Direction::Incoming) {
                continue;
            }
            let Some(ref meta) = req.metadata else {
                continue;
            };
            let caller_process = meta.get(peeps_types::PEEPS_CALLER_PROCESS_KEY);
            let caller_connection = meta.get(peeps_types::PEEPS_CALLER_CONNECTION_KEY);
            let caller_request_id = meta
                .get(peeps_types::PEEPS_CALLER_REQUEST_ID_KEY)
                .and_then(|v| v.parse::<u64>().ok());
            if let (Some(parent_process), Some(parent_connection), Some(parent_request_id)) =
                (caller_process, caller_connection, caller_request_id)
            {
                parents.push(peeps_types::RequestParentSnapshot {
                    child_process: process_name.to_string(),
                    child_connection: conn.name.clone(),
                    child_request_id: req.request_id,
                    parent_process: parent_process.clone(),
                    parent_connection: parent_connection.clone(),
                    parent_request_id,
                });
            }
        }
    }
    parents
}

fn format_timestamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = d.as_secs();
    let millis = d.subsec_millis();

    let day_secs = (total_secs % 86400) as u32;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let seconds = day_secs % 60;

    let days = (total_secs / 86400) as i64;
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}.{millis:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use peeps_types::{
        ConnectionSnapshot, Direction, RequestSnapshot, SessionSnapshot, TransportStats,
    };

    fn make_transport() -> TransportStats {
        TransportStats {
            frames_sent: 0,
            frames_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            last_sent_ago_secs: None,
            last_recv_ago_secs: None,
        }
    }

    #[test]
    fn extract_request_parents_from_incoming_metadata() {
        let mut meta = HashMap::new();
        meta.insert(
            peeps_types::PEEPS_CALLER_PROCESS_KEY.to_string(),
            "frontend".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_CONNECTION_KEY.to_string(),
            "conn-a".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_REQUEST_ID_KEY.to_string(),
            "42".to_string(),
        );

        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn-b".to_string(),
                peer_name: None,
                age_secs: 1.0,
                total_completed: 0,
                max_concurrent_requests: 10,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 7,
                    method_name: Some("get_page".to_string()),
                    method_id: 1,
                    direction: Direction::Incoming,
                    elapsed_secs: 0.5,
                    task_id: Some(10),
                    task_name: Some("handler".to_string()),
                    metadata: Some(meta),
                    args: None,
                    backtrace: None,
                    server_task_id: Some(10),
                    server_task_name: Some("handler".to_string()),
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![],
        };

        let parents = extract_request_parents("backend", &Some(session));
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].child_process, "backend");
        assert_eq!(parents[0].child_connection, "conn-b");
        assert_eq!(parents[0].child_request_id, 7);
        assert_eq!(parents[0].parent_process, "frontend");
        assert_eq!(parents[0].parent_connection, "conn-a");
        assert_eq!(parents[0].parent_request_id, 42);
    }

    #[test]
    fn extract_request_parents_skips_outgoing() {
        let mut meta = HashMap::new();
        meta.insert(
            peeps_types::PEEPS_CALLER_PROCESS_KEY.to_string(),
            "other".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_CONNECTION_KEY.to_string(),
            "conn".to_string(),
        );
        meta.insert(
            peeps_types::PEEPS_CALLER_REQUEST_ID_KEY.to_string(),
            "1".to_string(),
        );

        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn".to_string(),
                peer_name: None,
                age_secs: 1.0,
                total_completed: 0,
                max_concurrent_requests: 10,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 1,
                    method_name: None,
                    method_id: 1,
                    direction: Direction::Outgoing,
                    elapsed_secs: 0.5,
                    task_id: None,
                    task_name: None,
                    metadata: Some(meta),
                    args: None,
                    backtrace: None,
                    server_task_id: None,
                    server_task_name: None,
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![],
        };

        let parents = extract_request_parents("app", &Some(session));
        assert!(parents.is_empty());
    }

    #[test]
    fn extract_request_parents_skips_missing_metadata() {
        let session = SessionSnapshot {
            connections: vec![ConnectionSnapshot {
                name: "conn".to_string(),
                peer_name: None,
                age_secs: 1.0,
                total_completed: 0,
                max_concurrent_requests: 10,
                initial_credit: 65536,
                in_flight: vec![RequestSnapshot {
                    request_id: 1,
                    method_name: None,
                    method_id: 1,
                    direction: Direction::Incoming,
                    elapsed_secs: 0.5,
                    task_id: None,
                    task_name: None,
                    metadata: None,
                    args: None,
                    backtrace: None,
                    server_task_id: None,
                    server_task_name: None,
                }],
                recent_completions: vec![],
                channels: vec![],
                transport: make_transport(),
                channel_credits: vec![],
            }],
            method_names: HashMap::new(),
            channel_details: vec![],
        };

        let parents = extract_request_parents("app", &Some(session));
        assert!(parents.is_empty());
    }
}
