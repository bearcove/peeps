use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use moire_trace_types::{BacktraceId, FrameId, RelPc};
use moire_types::{
    BacktraceFrameResolved, BacktraceFrameUnresolved, SnapshotBacktrace, SnapshotBacktraceFrame,
    SnapshotCutResponse, SnapshotFrameRecord,
};

use crate::db::Db;
use crate::snapshot::repository::load_backtrace_frame_batches;
use crate::util::source_path::resolve_source_path;

pub struct SnapshotBacktraceTable {
    pub backtraces: Vec<SnapshotBacktrace>,
    pub frames: Vec<SnapshotFrameRecord>,
}

pub fn collect_snapshot_backtrace_ids(snapshot: &SnapshotCutResponse) -> Vec<BacktraceId> {
    let mut backtrace_ids = Vec::new();
    for process in &snapshot.processes {
        for entity in &process.snapshot.entities {
            backtrace_ids.push(entity.backtrace);
        }
        for scope in &process.snapshot.scopes {
            backtrace_ids.push(scope.backtrace);
        }
        for edge in &process.snapshot.edges {
            backtrace_ids.push(edge.backtrace);
        }
        for event in &process.snapshot.events {
            backtrace_ids.push(event.backtrace);
        }
    }
    backtrace_ids.sort_unstable();
    backtrace_ids.dedup();
    backtrace_ids
}

pub fn is_pending_frame(frame: &SnapshotBacktraceFrame) -> bool {
    matches!(
        frame,
        SnapshotBacktraceFrame::Unresolved(BacktraceFrameUnresolved { reason, .. })
        if reason == "symbolication pending"
    )
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct FrameDedupKey {
    module_identity: String,
    module_path: String,
    rel_pc: RelPc,
}

pub async fn load_snapshot_backtrace_table(
    db: Arc<Db>,
    backtrace_ids: &[BacktraceId],
) -> SnapshotBacktraceTable {
    if backtrace_ids.is_empty() {
        return SnapshotBacktraceTable {
            backtraces: vec![],
            frames: vec![],
        };
    }

    let backtrace_ids = backtrace_ids.to_vec();
    tokio::task::spawn_blocking(move || load_snapshot_backtrace_table_blocking(&db, &backtrace_ids))
        .await
        .unwrap_or_else(|error| panic!("join snapshot backtrace loader: {error}"))
        .unwrap_or_else(|error| panic!("load snapshot backtrace table: {error}"))
}

fn load_snapshot_backtrace_table_blocking(
    db: &Db,
    backtrace_ids: &[BacktraceId],
) -> Result<SnapshotBacktraceTable, String> {
    // r[impl api.snapshot.frame-catalog]
    let batches = load_backtrace_frame_batches(db, backtrace_ids)?;

    let mut frame_id_by_key: BTreeMap<FrameDedupKey, FrameId> = BTreeMap::new();
    let mut frame_by_id: BTreeMap<FrameId, SnapshotBacktraceFrame> = BTreeMap::new();
    let mut backtrace_entries: Vec<SnapshotBacktrace> = Vec::with_capacity(batches.len());

    for batch in batches {
        let mut this_frame_ids: Vec<FrameId> = Vec::with_capacity(batch.raw_rows.len());
        for raw in batch.raw_rows {
            let frame = match batch.symbolicated_by_index.get(&raw.frame_index) {
                Some(sym) if sym.status == "resolved" => {
                    match (sym.function_name.as_ref(), sym.source_file_path.as_ref()) {
                        (Some(function_name), Some(source_file)) => {
                            SnapshotBacktraceFrame::Resolved(BacktraceFrameResolved {
                                module_path: sym.module_path.clone(),
                                function_name: function_name.clone(),
                                source_file: resolve_source_path(source_file).into_owned(),
                                line: sym.source_line.and_then(|line| u32::try_from(line).ok()),
                            })
                        }
                        _ => SnapshotBacktraceFrame::Unresolved(BacktraceFrameUnresolved {
                            module_path: sym.module_path.clone(),
                            rel_pc: sym.rel_pc,
                            reason: String::from(
                                "resolved symbolication row missing function/source fields",
                            ),
                        }),
                    }
                }
                Some(sym) => SnapshotBacktraceFrame::Unresolved(BacktraceFrameUnresolved {
                    module_path: sym.module_path.clone(),
                    rel_pc: sym.rel_pc,
                    reason: sym
                        .unresolved_reason
                        .clone()
                        .unwrap_or_else(|| String::from("symbolication unresolved")),
                }),
                None => SnapshotBacktraceFrame::Unresolved(BacktraceFrameUnresolved {
                    module_path: raw.module_path.clone(),
                    rel_pc: raw.rel_pc,
                    reason: String::from("symbolication pending"),
                }),
            };
            let key = FrameDedupKey {
                module_identity: raw.module_identity,
                module_path: raw.module_path.clone(),
                rel_pc: raw.rel_pc,
            };
            let frame_id = match frame_id_by_key.get(&key) {
                Some(existing) => {
                    let existing_frame = frame_by_id.get(existing).cloned().ok_or_else(|| {
                        format!(
                            "invariant violated: frame id {} missing from frame map for backtrace {}",
                            existing,
                            batch.backtrace_id
                        )
                    })?;
                    let merged = merge_frame_state(&existing_frame, &frame, &key)?;
                    frame_by_id.insert(*existing, merged);
                    *existing
                }
                None => {
                    let assigned = stable_frame_id(&key)?;
                    if let Some(existing_key) =
                        frame_id_by_key
                            .iter()
                            .find_map(|(existing_key, existing_id)| {
                                if *existing_id == assigned {
                                    Some(existing_key.clone())
                                } else {
                                    None
                                }
                            })
                        && existing_key != key
                    {
                        return Err(format!(
                            "invariant violated: stable frame_id collision id={} existing=({}, {}, {:#x}) incoming=({}, {}, {:#x})",
                            assigned,
                            existing_key.module_identity,
                            existing_key.module_path,
                            existing_key.rel_pc.get(),
                            key.module_identity,
                            key.module_path,
                            key.rel_pc.get()
                        ));
                    }
                    frame_id_by_key.insert(key, assigned);
                    frame_by_id.insert(assigned, frame.clone());
                    assigned
                }
            };
            this_frame_ids.push(frame_id);
        }
        backtrace_entries.push(SnapshotBacktrace {
            backtrace_id: batch.backtrace_id,
            frame_ids: this_frame_ids,
        });
    }

    let frames = frame_by_id
        .into_iter()
        .map(|(frame_id, frame)| SnapshotFrameRecord { frame_id, frame })
        .collect();

    Ok(SnapshotBacktraceTable {
        backtraces: backtrace_entries,
        frames,
    })
}

fn frame_resolution_rank(frame: &SnapshotBacktraceFrame) -> u8 {
    match frame {
        SnapshotBacktraceFrame::Resolved(_) => 2,
        unresolved @ SnapshotBacktraceFrame::Unresolved(_) => {
            if is_pending_frame(unresolved) {
                0
            } else {
                1
            }
        }
    }
}

fn merge_frame_state(
    existing: &SnapshotBacktraceFrame,
    incoming: &SnapshotBacktraceFrame,
    key: &FrameDedupKey,
) -> Result<SnapshotBacktraceFrame, String> {
    if existing == incoming {
        return Ok(existing.clone());
    }

    match (existing, incoming) {
        (SnapshotBacktraceFrame::Resolved(a), SnapshotBacktraceFrame::Resolved(b)) if a != b => {
            return Err(format!(
                "invariant violated: conflicting resolved symbolication for frame key ({}, {}, {:#x})",
                key.module_identity,
                key.module_path,
                key.rel_pc.get()
            ));
        }
        _ => {}
    }

    let existing_rank = frame_resolution_rank(existing);
    let incoming_rank = frame_resolution_rank(incoming);
    if incoming_rank >= existing_rank {
        Ok(incoming.clone())
    } else {
        Ok(existing.clone())
    }
}

static FRAME_ID_REGISTRY: OnceLock<Mutex<BTreeMap<FrameDedupKey, FrameId>>> = OnceLock::new();

/// Given a raw FrameId u64 value, return the `(frame_id, module_identity, rel_pc)` triple
/// that was stored in the global registry when this frame was first seen.
/// Returns `None` if the frame_id is not in the registry.
// r[impl api.source.preview.security]
pub fn lookup_frame_source_by_raw(raw_id: u64) -> Option<(FrameId, String, RelPc)> {
    let registry = FRAME_ID_REGISTRY.get()?;
    let guard = registry.lock().ok()?;
    guard
        .iter()
        .find(|(_, id)| id.as_u64() == raw_id)
        .map(|(key, id)| (*id, key.module_identity.clone(), key.rel_pc))
}

fn stable_frame_id(key: &FrameDedupKey) -> Result<FrameId, String> {
    let registry = FRAME_ID_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut guard = registry
        .lock()
        .map_err(|_| String::from("invariant violated: frame id registry mutex poisoned"))?;
    if let Some(existing) = guard.get(key).copied() {
        return Ok(existing);
    }
    let frame_id = FrameId::next().map_err(|error| format!("invariant violated: {error}"))?;
    guard.insert(key.clone(), frame_id);
    Ok(frame_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // r[verify api.snapshot.frame-id-stable]
    #[test]
    fn stable_frame_id_is_deterministic_and_js_safe() {
        let key_a = FrameDedupKey {
            module_identity: String::from("debug_id:abc"),
            module_path: String::from("/tmp/a"),
            rel_pc: RelPc::new(0x1234).expect("valid rel_pc"),
        };
        let key_b = FrameDedupKey {
            module_identity: String::from("debug_id:abc"),
            module_path: String::from("/tmp/a"),
            rel_pc: RelPc::new(0x5678).expect("valid rel_pc"),
        };

        let a1 = stable_frame_id(&key_a).expect("frame id for key_a");
        let a2 = stable_frame_id(&key_a).expect("frame id for key_a (repeat)");
        let b = stable_frame_id(&key_b).expect("frame id for key_b");

        assert_eq!(a1, a2, "same frame key must map to same frame_id");
        assert_ne!(
            a1, b,
            "different frame keys should map to different frame_id"
        );
        assert!(format!("{a1}").starts_with("FRAME#"));
        assert!(format!("{b}").starts_with("FRAME#"));
    }
}
