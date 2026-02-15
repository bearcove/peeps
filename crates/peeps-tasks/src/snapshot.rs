#[cfg(feature = "diagnostics")]
use peeps_types::{FutureId, TaskId, TaskState};

use peeps_types::{
    FuturePollEdgeSnapshot, FutureResumeEdgeSnapshot, FutureSpawnEdgeSnapshot, FutureWaitSnapshot,
    FutureWakeEdgeSnapshot, GraphSnapshot, TaskSnapshot, WakeEdgeSnapshot,
};

// ── snapshot_all_tasks ──────────────────────────────────

#[cfg(feature = "diagnostics")]
pub fn snapshot_all_tasks() -> Vec<TaskSnapshot> {
    let now = std::time::Instant::now();
    let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
    let Some(tasks) = registry.as_ref() else {
        return Vec::new();
    };

    let cutoff = now - std::time::Duration::from_secs(30);

    tasks
        .iter()
        .filter(|task| {
            let state = task.state.lock().unwrap();
            state.state != TaskState::Completed || task.spawned_at > cutoff
        })
        .map(|task| task.snapshot(now, tasks))
        .collect()
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn snapshot_all_tasks() -> Vec<TaskSnapshot> {
    Vec::new()
}

// ── snapshot_wake_edges ─────────────────────────────────

#[cfg(feature = "diagnostics")]
pub fn snapshot_wake_edges() -> Vec<WakeEdgeSnapshot> {
    let now = std::time::Instant::now();

    let task_lookup: std::collections::HashMap<TaskId, String> = {
        let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
        let Some(tasks) = registry.as_ref() else {
            return Vec::new();
        };
        tasks
            .iter()
            .map(|task| (task.id, task.name.clone()))
            .collect()
    };

    let registry = crate::wakes::WAKE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_ref() else {
        return Vec::new();
    };

    let mut out: Vec<WakeEdgeSnapshot> = edges
        .iter()
        .map(
            |((source_task_id, target_task_id), edge)| WakeEdgeSnapshot {
                source_task_id: *source_task_id,
                source_task_name: source_task_id.and_then(|id| task_lookup.get(&id).cloned()),
                target_task_id: *target_task_id,
                target_task_name: task_lookup.get(target_task_id).cloned(),
                wake_count: edge.wake_count,
                last_wake_age_secs: now.duration_since(edge.last_wake_at).as_secs_f64(),
            },
        )
        .collect();
    out.sort_by(|a, b| b.wake_count.cmp(&a.wake_count));
    out
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn snapshot_wake_edges() -> Vec<WakeEdgeSnapshot> {
    Vec::new()
}

// ── snapshot_future_wake_edges ──────────────────────────

#[cfg(feature = "diagnostics")]
pub fn snapshot_future_wake_edges() -> Vec<FutureWakeEdgeSnapshot> {
    let now = std::time::Instant::now();

    let task_lookup: std::collections::HashMap<TaskId, String> = {
        let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
        let Some(tasks) = registry.as_ref() else {
            return Vec::new();
        };
        tasks
            .iter()
            .map(|task| (task.id, task.name.clone()))
            .collect()
    };

    let future_meta: std::collections::HashMap<FutureId, (String, Option<TaskId>)> = {
        let registry = crate::futures::FUTURE_WAIT_REGISTRY.lock().unwrap();
        let Some(waits) = registry.as_ref() else {
            return Vec::new();
        };
        waits
            .iter()
            .map(|(future_id, wait)| {
                (
                    *future_id,
                    (
                        wait.resource.clone(),
                        wait.last_polled_by_task_id.or(wait.created_by_task_id),
                    ),
                )
            })
            .collect()
    };

    let registry = crate::wakes::FUTURE_WAKE_EDGE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_ref() else {
        return Vec::new();
    };

    let mut out: Vec<FutureWakeEdgeSnapshot> = edges
        .iter()
        .map(|((source_task_id, future_id), edge)| {
            let (future_resource, target_task_id) = future_meta
                .get(future_id)
                .cloned()
                .unwrap_or_else(|| ("future".to_string(), None));
            FutureWakeEdgeSnapshot {
                source_task_id: *source_task_id,
                source_task_name: source_task_id.and_then(|id| task_lookup.get(&id).cloned()),
                future_id: *future_id,
                future_resource,
                target_task_id,
                target_task_name: target_task_id.and_then(|id| task_lookup.get(&id).cloned()),
                wake_count: edge.wake_count,
                last_wake_age_secs: now.duration_since(edge.last_wake_at).as_secs_f64(),
            }
        })
        .collect();
    out.sort_by(|a, b| b.wake_count.cmp(&a.wake_count));
    out
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn snapshot_future_wake_edges() -> Vec<FutureWakeEdgeSnapshot> {
    Vec::new()
}

// ── snapshot_future_waits ───────────────────────────────

#[cfg(feature = "diagnostics")]
pub fn snapshot_future_waits() -> Vec<FutureWaitSnapshot> {
    let now = std::time::Instant::now();
    let task_lookup: std::collections::HashMap<TaskId, String> = {
        let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
        let Some(tasks) = registry.as_ref() else {
            return Vec::new();
        };
        tasks
            .iter()
            .map(|task| (task.id, task.name.clone()))
            .collect()
    };

    let registry = crate::futures::FUTURE_WAIT_REGISTRY.lock().unwrap();
    let Some(waits) = registry.as_ref() else {
        return Vec::new();
    };

    let mut out: Vec<FutureWaitSnapshot> = waits
        .iter()
        .map(|(future_id, wait)| FutureWaitSnapshot {
            future_id: *future_id,
            task_id: wait
                .last_polled_by_task_id
                .or(wait.created_by_task_id)
                .unwrap_or(0),
            task_name: wait
                .last_polled_by_task_id
                .or(wait.created_by_task_id)
                .and_then(|id| task_lookup.get(&id).cloned()),
            resource: wait.resource.clone(),
            created_by_task_id: wait.created_by_task_id,
            created_by_task_name: wait
                .created_by_task_id
                .and_then(|id| task_lookup.get(&id).cloned()),
            created_age_secs: now.duration_since(wait.created_at).as_secs_f64(),
            last_polled_by_task_id: wait.last_polled_by_task_id,
            last_polled_by_task_name: wait
                .last_polled_by_task_id
                .and_then(|id| task_lookup.get(&id).cloned()),
            pending_count: wait.pending_count,
            ready_count: wait.ready_count,
            total_pending_secs: wait.total_pending.as_secs_f64(),
            last_seen_age_secs: now.duration_since(wait.last_seen).as_secs_f64(),
        })
        .collect();
    out.sort_by(|a, b| {
        b.total_pending_secs
            .partial_cmp(&a.total_pending_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn snapshot_future_waits() -> Vec<FutureWaitSnapshot> {
    Vec::new()
}

// ── snapshot_future_spawn_edges ─────────────────────────

#[cfg(feature = "diagnostics")]
pub fn snapshot_future_spawn_edges() -> Vec<FutureSpawnEdgeSnapshot> {
    let now = std::time::Instant::now();
    let task_lookup: std::collections::HashMap<TaskId, String> = {
        let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
        let Some(tasks) = registry.as_ref() else {
            return Vec::new();
        };
        tasks
            .iter()
            .map(|task| (task.id, task.name.clone()))
            .collect()
    };

    let registry = crate::futures::FUTURE_SPAWN_EDGE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_ref() else {
        return Vec::new();
    };

    edges
        .iter()
        .map(|edge| FutureSpawnEdgeSnapshot {
            parent_future_id: edge.parent_future_id,
            parent_resource: edge.parent_resource.clone(),
            child_future_id: edge.child_future_id,
            child_resource: edge.child_resource.clone(),
            created_by_task_id: edge.created_by_task_id,
            created_by_task_name: edge
                .created_by_task_id
                .and_then(|id| task_lookup.get(&id).cloned()),
            created_age_secs: now.duration_since(edge.created_at).as_secs_f64(),
        })
        .collect()
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn snapshot_future_spawn_edges() -> Vec<FutureSpawnEdgeSnapshot> {
    Vec::new()
}

// ── snapshot_future_poll_edges ──────────────────────────

#[cfg(feature = "diagnostics")]
pub fn snapshot_future_poll_edges() -> Vec<FuturePollEdgeSnapshot> {
    let now = std::time::Instant::now();
    let task_lookup: std::collections::HashMap<TaskId, String> = {
        let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
        let Some(tasks) = registry.as_ref() else {
            return Vec::new();
        };
        tasks
            .iter()
            .map(|task| (task.id, task.name.clone()))
            .collect()
    };

    let registry = crate::futures::FUTURE_POLL_EDGE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_ref() else {
        return Vec::new();
    };

    edges
        .iter()
        .map(|((task_id, future_id), edge)| FuturePollEdgeSnapshot {
            task_id: *task_id,
            task_name: task_lookup.get(task_id).cloned(),
            future_id: *future_id,
            future_resource: edge.resource.clone(),
            poll_count: edge.poll_count,
            total_poll_secs: edge.total_poll.as_secs_f64(),
            last_poll_age_secs: now.duration_since(edge.last_poll_at).as_secs_f64(),
        })
        .collect()
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn snapshot_future_poll_edges() -> Vec<FuturePollEdgeSnapshot> {
    Vec::new()
}

// ── snapshot_future_resume_edges ────────────────────────

#[cfg(feature = "diagnostics")]
pub fn snapshot_future_resume_edges() -> Vec<FutureResumeEdgeSnapshot> {
    let now = std::time::Instant::now();
    let task_lookup: std::collections::HashMap<TaskId, String> = {
        let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
        let Some(tasks) = registry.as_ref() else {
            return Vec::new();
        };
        tasks
            .iter()
            .map(|task| (task.id, task.name.clone()))
            .collect()
    };

    let registry = crate::wakes::FUTURE_RESUME_EDGE_REGISTRY.lock().unwrap();
    let Some(edges) = registry.as_ref() else {
        return Vec::new();
    };

    edges
        .iter()
        .map(
            |((future_id, target_task_id), edge)| FutureResumeEdgeSnapshot {
                future_id: *future_id,
                future_resource: edge.future_resource.clone(),
                target_task_id: *target_task_id,
                target_task_name: task_lookup.get(target_task_id).cloned(),
                resume_count: edge.resume_count,
                last_resume_age_secs: now.duration_since(edge.last_resume_at).as_secs_f64(),
            },
        )
        .collect()
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn snapshot_future_resume_edges() -> Vec<FutureResumeEdgeSnapshot> {
    Vec::new()
}

// ── emit_graph ──────────────────────────────────────────

#[cfg(feature = "diagnostics")]
pub fn emit_graph(process_name: &str, proc_key: &str) -> GraphSnapshot {
    use peeps_types::{GraphSnapshotBuilder, Node};

    let mut builder = GraphSnapshotBuilder::new();
    let mut future_node_ids: std::collections::HashMap<FutureId, String> =
        std::collections::HashMap::new();

    // Task nodes
    {
        let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
        if let Some(tasks) = registry.as_ref() {
            let now = std::time::Instant::now();
            for task in tasks.iter() {
                let state = task.state.lock().unwrap();
                let node_id = format!("task:{proc_key}:{}", task.id);
                let spawned_at_ns = now.duration_since(task.spawned_at).as_nanos() as u64;
                let loc = task.spawn_location;

                let mut attrs = String::with_capacity(256);
                attrs.push('{');
                write_json_kv_u64(&mut attrs, "task_id", task.id, true);
                write_json_kv_str(&mut attrs, "name", &task.name, false);
                write_json_kv_str(
                    &mut attrs,
                    "state",
                    match state.state {
                        peeps_types::TaskState::Pending => "pending",
                        peeps_types::TaskState::Polling => "polling",
                        peeps_types::TaskState::Completed => "completed",
                    },
                    false,
                );
                write_json_kv_u64(&mut attrs, "spawned_at_ns", spawned_at_ns, false);
                if let Some(pid) = task.parent_id {
                    write_json_kv_u64(&mut attrs, "parent_task_id", pid, false);
                }

                // Location as metadata
                attrs.push_str(",\"meta\":{");
                write_json_kv_str(&mut attrs, "ctx.file", loc.file(), true);
                write_json_kv_u64(&mut attrs, "ctx.line", loc.line() as u64, false);
                write_json_kv_u64(&mut attrs, "ctx.column", loc.column() as u64, false);
                attrs.push('}');

                attrs.push('}');

                builder.push_node(Node {
                    id: node_id,
                    kind: "task".to_string(),
                    process: process_name.to_string(),
                    proc_key: proc_key.to_string(),
                    label: Some(task.name.clone()),
                    attrs_json: attrs,
                });
            }
        }
    }

    // Future nodes
    {
        let registry = crate::futures::FUTURE_WAIT_REGISTRY.lock().unwrap();
        if let Some(waits) = registry.as_ref() {
            for (&future_id, wait) in waits.iter() {
                let node_id = peeps_types::new_node_id("future");
                future_node_ids.insert(future_id, node_id.clone());

                let mut attrs = String::with_capacity(256);
                attrs.push('{');
                write_json_kv_u64(&mut attrs, "future_id", future_id, true);
                write_json_kv_str(&mut attrs, "label", &wait.resource, false);
                write_json_kv_u64(&mut attrs, "pending_count", wait.pending_count, false);
                write_json_kv_u64(&mut attrs, "ready_count", wait.ready_count, false);
                if let Some(tid) = wait.created_by_task_id {
                    write_json_kv_u64(&mut attrs, "created_by_task_id", tid, false);
                }
                if let Some(tid) = wait.last_polled_by_task_id {
                    write_json_kv_u64(&mut attrs, "last_polled_by_task_id", tid, false);
                }
                let total_pending_ns = wait.total_pending.as_nanos() as u64;
                if total_pending_ns > 0 {
                    write_json_kv_u64(&mut attrs, "total_pending_ns", total_pending_ns, false);
                }
                if wait.meta_json.is_empty() {
                    attrs.push_str(",\"meta\":{}");
                } else {
                    attrs.push_str(",\"meta\":");
                    attrs.push_str(&wait.meta_json);
                }
                attrs.push('}');

                builder.push_node(Node {
                    id: node_id,
                    kind: "future".to_string(),
                    process: process_name.to_string(),
                    proc_key: proc_key.to_string(),
                    label: Some(wait.resource.clone()),
                    attrs_json: attrs,
                });
            }
        }
    }

    builder.finish()
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn emit_graph(_process_name: &str, _proc_key: &str) -> GraphSnapshot {
    GraphSnapshot::empty()
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

// ── cleanup_completed_tasks ─────────────────────────────

#[cfg(feature = "diagnostics")]
pub fn cleanup_completed_tasks() {
    let now = std::time::Instant::now();
    let cutoff = now - std::time::Duration::from_secs(30);

    if let Some(registry) = crate::tasks::TASK_REGISTRY.lock().unwrap().as_mut() {
        registry.retain(|task| {
            let state = task.state.lock().unwrap();
            state.state != TaskState::Completed || task.spawned_at > cutoff
        });
    }

    let live_ids: std::collections::HashSet<TaskId> = {
        let registry = crate::tasks::TASK_REGISTRY.lock().unwrap();
        let Some(tasks) = registry.as_ref() else {
            return;
        };
        tasks.iter().map(|task| task.id).collect()
    };
    if let Some(edges) = crate::wakes::WAKE_REGISTRY.lock().unwrap().as_mut() {
        edges.retain(|(_, target), edge| {
            let recent = edge.last_wake_at > cutoff;
            live_ids.contains(target) || recent
        });
    }
    let live_future_ids: std::collections::HashSet<FutureId> = {
        let waits = crate::futures::FUTURE_WAIT_REGISTRY.lock().unwrap();
        waits
            .as_ref()
            .map(|waits| waits.keys().copied().collect())
            .unwrap_or_default()
    };
    if let Some(edges) = crate::wakes::FUTURE_WAKE_EDGE_REGISTRY
        .lock()
        .unwrap()
        .as_mut()
    {
        edges.retain(|(_, future_id), edge| {
            let recent = edge.last_wake_at > cutoff;
            recent || live_future_ids.contains(future_id)
        });
    }
    if let Some(waits) = crate::futures::FUTURE_WAIT_REGISTRY
        .lock()
        .unwrap()
        .as_mut()
    {
        waits.retain(|_, wait| {
            let recent = wait.last_seen > cutoff;
            let creator_live = wait
                .created_by_task_id
                .is_some_and(|task_id| live_ids.contains(&task_id));
            let last_polled_live = wait
                .last_polled_by_task_id
                .is_some_and(|task_id| live_ids.contains(&task_id));
            creator_live || last_polled_live || recent
        });
    }
    if let Some(edges) = crate::futures::FUTURE_SPAWN_EDGE_REGISTRY
        .lock()
        .unwrap()
        .as_mut()
    {
        edges.retain(|edge| {
            let recent = edge.created_at > cutoff;
            recent || live_future_ids.contains(&edge.child_future_id)
        });
    }
    if let Some(edges) = crate::futures::FUTURE_POLL_EDGE_REGISTRY
        .lock()
        .unwrap()
        .as_mut()
    {
        edges.retain(|(task_id, future_id), edge| {
            let recent = edge.last_poll_at > cutoff;
            recent || (live_ids.contains(task_id) && live_future_ids.contains(future_id))
        });
    }
    if let Some(edges) = crate::wakes::FUTURE_RESUME_EDGE_REGISTRY
        .lock()
        .unwrap()
        .as_mut()
    {
        edges.retain(|(future_id, target_task_id), edge| {
            let recent = edge.last_resume_at > cutoff;
            recent || (live_future_ids.contains(future_id) && live_ids.contains(target_task_id))
        });
    }
}

#[cfg(not(feature = "diagnostics"))]
#[inline(always)]
pub fn cleanup_completed_tasks() {}
