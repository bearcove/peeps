#[cfg(feature = "diagnostics")]
pub fn snapshot_lock_diagnostics() -> crate::LockSnapshot {
    use std::sync::atomic::Ordering;

    use crate::registry::{AcquireKind, LOCK_REGISTRY};

    let Ok(registry) = LOCK_REGISTRY.lock() else {
        return crate::LockSnapshot { locks: Vec::new() };
    };

    let mut locks = Vec::new();
    for weak in registry.iter() {
        let Some(info) = weak.upgrade() else {
            continue;
        };

        let (Ok(waiters), Ok(holders)) = (info.waiters.lock(), info.holders.lock()) else {
            continue;
        };

        let holder_snapshots: Vec<crate::LockHolderSnapshot> = holders
            .iter()
            .map(|h| {
                let bt = format!("{}", h.backtrace);
                crate::LockHolderSnapshot {
                    kind: match h.kind {
                        AcquireKind::Read => crate::LockAcquireKind::Read,
                        AcquireKind::Write => crate::LockAcquireKind::Write,
                        AcquireKind::Mutex => crate::LockAcquireKind::Mutex,
                    },
                    held_secs: h.since.elapsed().as_secs_f64(),
                    backtrace: if bt.is_empty() { None } else { Some(bt) },
                    task_id: h.peeps_task_id,
                    task_name: h.peeps_task_id.and_then(peeps_tasks::task_name),
                }
            })
            .collect();

        let waiter_snapshots: Vec<crate::LockWaiterSnapshot> = waiters
            .iter()
            .map(|w| {
                let bt = format!("{}", w.backtrace);
                crate::LockWaiterSnapshot {
                    kind: match w.kind {
                        AcquireKind::Read => crate::LockAcquireKind::Read,
                        AcquireKind::Write => crate::LockAcquireKind::Write,
                        AcquireKind::Mutex => crate::LockAcquireKind::Mutex,
                    },
                    waiting_secs: w.since.elapsed().as_secs_f64(),
                    backtrace: if bt.is_empty() { None } else { Some(bt) },
                    task_id: w.peeps_task_id,
                    task_name: w.peeps_task_id.and_then(peeps_tasks::task_name),
                }
            })
            .collect();

        locks.push(crate::LockInfoSnapshot {
            name: info.name.to_string(),
            acquires: info.total_acquires.load(Ordering::SeqCst),
            releases: info.total_releases.load(Ordering::SeqCst),
            holders: holder_snapshots,
            waiters: waiter_snapshots,
        });
    }

    crate::LockSnapshot { locks }
}

#[cfg(not(feature = "diagnostics"))]
#[inline]
pub fn snapshot_lock_diagnostics() -> crate::LockSnapshot {
    crate::LockSnapshot { locks: Vec::new() }
}

#[cfg(not(feature = "diagnostics"))]
#[inline]
pub fn dump_lock_diagnostics() -> String {
    String::new()
}

// ── Canonical graph emission ────────────────────────────

#[cfg(feature = "diagnostics")]
pub fn emit_lock_graph(process_name: &str, proc_key: &str) -> peeps_types::GraphSnapshot {
    use std::sync::atomic::Ordering;

    use peeps_types::{GraphSnapshotBuilder, Node};

    use crate::registry::{AcquireKind, LOCK_REGISTRY};

    let Ok(registry) = LOCK_REGISTRY.lock() else {
        return peeps_types::GraphSnapshot::empty();
    };

    let mut builder = GraphSnapshotBuilder::new();

    for weak in registry.iter() {
        let Some(info) = weak.upgrade() else {
            continue;
        };

        let (Ok(waiters), Ok(holders)) = (info.waiters.lock(), info.holders.lock()) else {
            continue;
        };

        let acquires = info.total_acquires.load(Ordering::SeqCst);
        let releases = info.total_releases.load(Ordering::SeqCst);
        let holder_count = holders.len() as u64;
        let waiter_count = waiters.len() as u64;

        // Determine lock_kind from first holder or waiter
        let lock_kind = {
            let first_kind = holders.first().or(waiters.first()).map(|e| e.kind);
            match first_kind {
                Some(AcquireKind::Mutex) => "mutex",
                Some(AcquireKind::Write) => "rwlock_write",
                Some(AcquireKind::Read) => {
                    // Check if any write holders exist too
                    if holders.iter().any(|h| matches!(h.kind, AcquireKind::Write))
                        || waiters.iter().any(|w| matches!(w.kind, AcquireKind::Write))
                    {
                        "rwlock_write"
                    } else {
                        "rwlock_read"
                    }
                }
                None => "mutex",
            }
        };

        let node_id = peeps_types::new_node_id("lock");

        // Build attrs_json
        let mut attrs = String::with_capacity(256);
        attrs.push('{');
        write_json_kv_str(&mut attrs, "name", info.name, true);
        write_json_kv_str(&mut attrs, "lock_kind", lock_kind, false);
        write_json_kv_u64(&mut attrs, "acquires", acquires, false);
        write_json_kv_u64(&mut attrs, "releases", releases, false);
        write_json_kv_u64(&mut attrs, "holder_count", holder_count, false);
        write_json_kv_u64(&mut attrs, "waiter_count", waiter_count, false);
        attrs.push_str(",\"meta\":{}");
        attrs.push('}');

        builder.push_node(Node {
            id: node_id,
            kind: "lock".to_string(),
            process: process_name.to_string(),
            proc_key: proc_key.to_string(),
            label: Some(info.name.to_string()),
            attrs_json: attrs,
        });
    }

    builder.finish()
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn emit_lock_graph(_process_name: &str, _proc_key: &str) -> peeps_types::GraphSnapshot {
    peeps_types::GraphSnapshot::empty()
}

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
