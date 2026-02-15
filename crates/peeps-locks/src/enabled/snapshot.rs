use std::sync::atomic::Ordering;
use std::sync::OnceLock;

use peeps_types::{GraphSnapshot, GraphSnapshotBuilder, Node};

use crate::enabled::registry::{AcquireKind, LOCK_REGISTRY};

static PROCESS_NAME: OnceLock<String> = OnceLock::new();
static PROC_KEY: OnceLock<String> = OnceLock::new();

pub fn set_process_info(process_name: impl Into<String>, proc_key: impl Into<String>) {
    PROCESS_NAME.set(process_name.into()).ok();
    PROC_KEY.set(proc_key.into()).ok();
}

// ── Canonical graph emission ────────────────────────────

pub fn emit_lock_graph() -> GraphSnapshot {
    let Ok(registry) = LOCK_REGISTRY.lock() else {
        return GraphSnapshot::empty();
    };

    let mut builder = GraphSnapshotBuilder::new();

    let process_name = PROCESS_NAME.get().map(|s| s.as_str()).unwrap_or("unknown");
    let proc_key = PROC_KEY.get().map(|s| s.as_str()).unwrap_or("unknown");
    builder.set_process_info(process_name, proc_key);

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
            kind: peeps_types::NodeKind::Lock,
            label: Some(info.name.to_string()),
            attrs_json: attrs,
        });
    }

    builder.finish()
}

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
