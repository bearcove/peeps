use peeps_types::{GraphSnapshot, SyncSnapshot};

#[cfg(feature = "diagnostics")]
pub fn snapshot_all() -> SyncSnapshot {
    let reg = crate::registry::REGISTRY.lock().unwrap();
    let now = std::time::Instant::now();

    SyncSnapshot {
        mpsc_channels: reg
            .mpsc
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
        oneshot_channels: reg
            .oneshot
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
        watch_channels: reg
            .watch
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
        semaphores: reg
            .semaphore
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
        once_cells: reg
            .once_cell
            .iter()
            .filter_map(|w| w.upgrade())
            .map(|info| info.snapshot(now))
            .collect(),
    }
}

#[cfg(not(feature = "diagnostics"))]
pub fn snapshot_all() -> SyncSnapshot {
    SyncSnapshot {
        mpsc_channels: Vec::new(),
        oneshot_channels: Vec::new(),
        watch_channels: Vec::new(),
        semaphores: Vec::new(),
        once_cells: Vec::new(),
    }
}

// ── Canonical graph emission ────────────────────────────

#[cfg(feature = "diagnostics")]
pub fn emit_graph(process_name: &str, proc_key: &str) -> GraphSnapshot {
    use std::sync::atomic::Ordering;

    use peeps_types::{Edge, GraphEdgeOrigin, GraphSnapshotBuilder, Node};

    let reg = crate::registry::REGISTRY.lock().unwrap();
    let now = std::time::Instant::now();
    let mut builder = GraphSnapshotBuilder::new();

    // ── mpsc channels ────────────────────────────────────
    for info in reg.mpsc.iter().filter_map(|w| w.upgrade()) {
        let name = &info.name;
        let created_at_ns = now.duration_since(info.created_at).as_nanos() as u64;
        let sent = info.sent.load(Ordering::Relaxed);
        let received = info.received.load(Ordering::Relaxed);
        let queue_len = sent.saturating_sub(received);
        let high_watermark = info.high_watermark.load(Ordering::Relaxed);
        let send_waiters = info.send_waiters.load(Ordering::Relaxed);
        let recv_waiters = info.recv_waiters.load(Ordering::Relaxed);
        let sender_count = info.sender_count.load(Ordering::Relaxed);
        let sender_closed = info.sender_closed.load(Ordering::Relaxed) != 0;
        let receiver_closed = info.receiver_closed.load(Ordering::Relaxed) != 0;

        let tx_id = peeps_types::new_node_id("mpsc_tx");
        let rx_id = peeps_types::new_node_id("mpsc_rx");

        // TX node
        {
            let mut attrs = String::with_capacity(384);
            attrs.push('{');
            write_json_kv_str(&mut attrs, "name", name, true);
            write_json_kv_u64(&mut attrs, "created_at_ns", created_at_ns, false);
            if let Some(tid) = info.creator_task_id {
                write_json_kv_u64(&mut attrs, "creator_task_id", tid, false);
            }
            write_json_kv_bool(&mut attrs, "closed", sender_closed, false);
            write_json_kv_bool(&mut attrs, "bounded", info.bounded, false);
            if let Some(cap) = info.capacity {
                write_json_kv_u64(&mut attrs, "capacity", cap, false);
            }
            write_json_kv_u64(&mut attrs, "sender_count", sender_count, false);
            write_json_kv_u64(&mut attrs, "send_waiters", send_waiters, false);
            write_json_kv_u64(&mut attrs, "sent_total", sent, false);
            write_json_kv_u64(&mut attrs, "queue_len", queue_len, false);
            write_json_kv_u64(&mut attrs, "high_watermark", high_watermark, false);
            if info.bounded {
                if let Some(cap) = info.capacity {
                    if cap > 0 {
                        let utilization = (queue_len as f64 / cap as f64 * 1000.0).round() / 1000.0;
                        write_json_kv_f64(&mut attrs, "utilization", utilization, false);
                    }
                }
            }
            attrs.push_str(",\"meta\":{}");
            attrs.push('}');

            builder.push_node(Node {
                id: tx_id.clone(),
                kind: "mpsc_tx".to_string(),
                process: process_name.to_string(),
                proc_key: proc_key.to_string(),
                label: Some(format!("{name}:tx")),
                attrs_json: attrs,
            });
        }

        // RX node
        {
            let mut attrs = String::with_capacity(384);
            attrs.push('{');
            write_json_kv_str(&mut attrs, "name", name, true);
            write_json_kv_u64(&mut attrs, "created_at_ns", created_at_ns, false);
            if let Some(tid) = info.creator_task_id {
                write_json_kv_u64(&mut attrs, "creator_task_id", tid, false);
            }
            write_json_kv_bool(&mut attrs, "closed", receiver_closed, false);
            write_json_kv_u64(&mut attrs, "recv_waiters", recv_waiters, false);
            write_json_kv_u64(&mut attrs, "recv_total", received, false);
            write_json_kv_u64(&mut attrs, "queue_len", queue_len, false);
            attrs.push_str(",\"meta\":{}");
            attrs.push('}');

            builder.push_node(Node {
                id: rx_id.clone(),
                kind: "mpsc_rx".to_string(),
                process: process_name.to_string(),
                proc_key: proc_key.to_string(),
                label: Some(format!("{name}:rx")),
                attrs_json: attrs,
            });
        }

        // tx → rx edge
        builder.push_edge(Edge {
            src_id: tx_id.clone(),
            dst_id: rx_id.clone(),
            kind: "needs".to_string(),
            observed_at_ns: None,
            attrs_json: "{}".to_string(),
            origin: GraphEdgeOrigin::Explicit,
        });

        // Task relationships remain in attrs, not canonical graph edges.
    }

    // ── oneshot channels ─────────────────────────────────
    for info in reg.oneshot.iter().filter_map(|w| w.upgrade()) {
        let name = &info.name;
        let age_ns = now.duration_since(info.created_at).as_nanos() as u64;
        let state_val = info.state.load(Ordering::Relaxed);
        let state_str = match state_val {
            1 => "sent",
            2 => "received",
            3 => "sender_dropped",
            4 => "receiver_dropped",
            _ => "pending",
        };
        let sender_closed = state_val == 3 || state_val == 2;
        let receiver_closed = state_val == 4 || state_val == 2;

        let tx_id = peeps_types::new_node_id("oneshot_tx");
        let rx_id = peeps_types::new_node_id("oneshot_rx");

        // TX node
        {
            let mut attrs = String::with_capacity(256);
            attrs.push('{');
            write_json_kv_str(&mut attrs, "name", name, true);
            write_json_kv_u64(&mut attrs, "created_at_ns", age_ns, false);
            if let Some(tid) = info.creator_task_id {
                write_json_kv_u64(&mut attrs, "creator_task_id", tid, false);
            }
            write_json_kv_bool(&mut attrs, "closed", sender_closed, false);
            write_json_kv_str(&mut attrs, "state", state_str, false);
            write_json_kv_u64(&mut attrs, "age_ns", age_ns, false);
            attrs.push_str(",\"meta\":{}");
            attrs.push('}');

            builder.push_node(Node {
                id: tx_id.clone(),
                kind: "oneshot_tx".to_string(),
                process: process_name.to_string(),
                proc_key: proc_key.to_string(),
                label: Some(format!("{name}:tx")),
                attrs_json: attrs,
            });
        }

        // RX node
        {
            let mut attrs = String::with_capacity(256);
            attrs.push('{');
            write_json_kv_str(&mut attrs, "name", name, true);
            write_json_kv_u64(&mut attrs, "created_at_ns", age_ns, false);
            if let Some(tid) = info.creator_task_id {
                write_json_kv_u64(&mut attrs, "creator_task_id", tid, false);
            }
            write_json_kv_bool(&mut attrs, "closed", receiver_closed, false);
            write_json_kv_str(&mut attrs, "state", state_str, false);
            write_json_kv_u64(&mut attrs, "age_ns", age_ns, false);
            attrs.push_str(",\"meta\":{}");
            attrs.push('}');

            builder.push_node(Node {
                id: rx_id.clone(),
                kind: "oneshot_rx".to_string(),
                process: process_name.to_string(),
                proc_key: proc_key.to_string(),
                label: Some(format!("{name}:rx")),
                attrs_json: attrs,
            });
        }

        // tx → rx edge
        builder.push_edge(Edge {
            src_id: tx_id.clone(),
            dst_id: rx_id.clone(),
            kind: "needs".to_string(),
            observed_at_ns: None,
            attrs_json: "{}".to_string(),
            origin: GraphEdgeOrigin::Explicit,
        });

        // Task relationships remain in attrs, not canonical graph edges.
    }

    // ── watch channels ───────────────────────────────────
    for info in reg.watch.iter().filter_map(|w| w.upgrade()) {
        let name = &info.name;
        let age_ns = now.duration_since(info.created_at).as_nanos() as u64;
        let changes = info.changes.load(Ordering::Relaxed);
        let receiver_count = (info.receiver_count)() as u64;

        let tx_id = peeps_types::new_node_id("watch_tx");
        let rx_id = peeps_types::new_node_id("watch_rx");

        // TX node
        {
            let mut attrs = String::with_capacity(256);
            attrs.push('{');
            write_json_kv_str(&mut attrs, "name", name, true);
            write_json_kv_u64(&mut attrs, "created_at_ns", age_ns, false);
            if let Some(tid) = info.creator_task_id {
                write_json_kv_u64(&mut attrs, "creator_task_id", tid, false);
            }
            write_json_kv_u64(&mut attrs, "changes", changes, false);
            write_json_kv_u64(&mut attrs, "receiver_count", receiver_count, false);
            write_json_kv_u64(&mut attrs, "age_ns", age_ns, false);
            attrs.push_str(",\"meta\":{}");
            attrs.push('}');

            builder.push_node(Node {
                id: tx_id.clone(),
                kind: "watch_tx".to_string(),
                process: process_name.to_string(),
                proc_key: proc_key.to_string(),
                label: Some(format!("{name}:tx")),
                attrs_json: attrs,
            });
        }

        // RX node
        {
            let mut attrs = String::with_capacity(256);
            attrs.push('{');
            write_json_kv_str(&mut attrs, "name", name, true);
            write_json_kv_u64(&mut attrs, "created_at_ns", age_ns, false);
            if let Some(tid) = info.creator_task_id {
                write_json_kv_u64(&mut attrs, "creator_task_id", tid, false);
            }
            write_json_kv_u64(&mut attrs, "changes", changes, false);
            write_json_kv_u64(&mut attrs, "receiver_count", receiver_count, false);
            write_json_kv_u64(&mut attrs, "age_ns", age_ns, false);
            attrs.push_str(",\"meta\":{}");
            attrs.push('}');

            builder.push_node(Node {
                id: rx_id.clone(),
                kind: "watch_rx".to_string(),
                process: process_name.to_string(),
                proc_key: proc_key.to_string(),
                label: Some(format!("{name}:rx")),
                attrs_json: attrs,
            });
        }

        // tx → rx edge
        builder.push_edge(Edge {
            src_id: tx_id.clone(),
            dst_id: rx_id.clone(),
            kind: "needs".to_string(),
            observed_at_ns: None,
            attrs_json: "{}".to_string(),
            origin: GraphEdgeOrigin::Explicit,
        });

        // Task relationships remain in attrs, not canonical graph edges.
    }

    // ── semaphores ───────────────────────────────────────
    for info in reg.semaphore.iter().filter_map(|w| w.upgrade()) {
        let name = &info.name;
        let node_id = peeps_types::new_node_id("semaphore");
        let permits_available = (info.available_permits)() as u64;
        let waiters = info.waiters.load(Ordering::Relaxed);
        let acquires = info.acquires.load(Ordering::Relaxed);
        let high_waiters_watermark = info.high_waiters_watermark.load(Ordering::Relaxed);

        let oldest_wait_ns = {
            let active = info.active_waiters.lock().unwrap();
            active
                .iter()
                .map(|w| now.duration_since(w.started_at).as_nanos() as u64)
                .max()
                .unwrap_or(0)
        };

        let mut attrs = String::with_capacity(384);
        attrs.push('{');
        write_json_kv_str(&mut attrs, "name", name, true);
        write_json_kv_u64(&mut attrs, "permits_total", info.permits_total, false);
        write_json_kv_u64(&mut attrs, "permits_available", permits_available, false);
        write_json_kv_u64(&mut attrs, "waiters", waiters, false);
        write_json_kv_u64(&mut attrs, "acquires", acquires, false);
        write_json_kv_u64(&mut attrs, "oldest_wait_ns", oldest_wait_ns, false);
        write_json_kv_u64(
            &mut attrs,
            "high_waiters_watermark",
            high_waiters_watermark,
            false,
        );
        if let Some(tid) = info.creator_task_id {
            write_json_kv_u64(&mut attrs, "creator_task_id", tid, false);
        }
        attrs.push_str(",\"meta\":{}");
        attrs.push('}');

        builder.push_node(Node {
            id: node_id.clone(),
            kind: "semaphore".to_string(),
            process: proc_key.to_string(),
            proc_key: proc_key.to_string(),
            label: Some(name.clone()),
            attrs_json: attrs,
        });

        // Task relationships remain in attrs/snapshots, not canonical graph edges.
    }

    // ── oncecells ────────────────────────────────────────
    for info in reg.once_cell.iter().filter_map(|w| w.upgrade()) {
        let name = &info.name;
        let node_id = peeps_types::new_node_id("oncecell");
        let age_ns = now.duration_since(info.created_at).as_nanos() as u64;
        let state_val = info.state.load(Ordering::Relaxed);
        let state_str = match state_val {
            1 => "initializing",
            2 => "initialized",
            _ => "empty",
        };
        let init_duration_ns = info
            .init_duration
            .lock()
            .unwrap()
            .map(|d| d.as_nanos() as u64);

        let mut attrs = String::with_capacity(256);
        attrs.push('{');
        write_json_kv_str(&mut attrs, "name", name, true);
        write_json_kv_str(&mut attrs, "state", state_str, false);
        write_json_kv_u64(&mut attrs, "age_ns", age_ns, false);
        if let Some(dur) = init_duration_ns {
            write_json_kv_u64(&mut attrs, "init_duration_ns", dur, false);
        }
        attrs.push_str(",\"meta\":{}");
        attrs.push('}');

        builder.push_node(Node {
            id: node_id,
            kind: "oncecell".to_string(),
            process: proc_key.to_string(),
            proc_key: proc_key.to_string(),
            label: Some(name.clone()),
            attrs_json: attrs,
        });
    }

    builder.finish()
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn emit_graph(_process_name: &str, _proc_key: &str) -> GraphSnapshot {
    GraphSnapshot::empty()
}

// ── JSON helpers ─────────────────────────────────────────

#[cfg(feature = "diagnostics")]
fn write_json_kv_str(out: &mut String, key: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":\"");
    peeps_types::json_escape_into(out, value);
    out.push('"');
}

#[cfg(feature = "diagnostics")]
fn write_json_kv_u64(out: &mut String, key: &str, value: u64, first: bool) {
    use std::io::Write;
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    let mut buf = [0u8; 20];
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = write!(cursor, "{value}");
    let len = cursor.position() as usize;
    out.push_str(std::str::from_utf8(&buf[..len]).unwrap_or("0"));
}

#[cfg(feature = "diagnostics")]
fn write_json_kv_bool(out: &mut String, key: &str, value: bool, first: bool) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(if value { "true" } else { "false" });
}

#[cfg(feature = "diagnostics")]
fn write_json_kv_f64(out: &mut String, key: &str, value: f64, first: bool) {
    use std::io::Write;
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    let mut buf = [0u8; 32];
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = write!(cursor, "{value}");
    let len = cursor.position() as usize;
    out.push_str(std::str::from_utf8(&buf[..len]).unwrap_or("0"));
}
