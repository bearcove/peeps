use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use moire_trace_types::{FrameId, RelPc};
use moire_types::{
    BacktraceFrameResolved, BacktraceFrameUnresolved, ConnectionId, SnapshotBacktraceFrame,
    SnapshotCutResponse, SnapshotFrameRecord,
};

use crate::db::Db;
use crate::snapshot::repository::load_backtrace_frame_batches;

pub struct SnapshotBacktraceTable {
    pub frames: Vec<SnapshotFrameRecord>,
}

pub fn collect_snapshot_backtrace_pairs(
    snapshot: &SnapshotCutResponse,
) -> Vec<(ConnectionId, u64)> {
    let mut pairs = Vec::new();
    for process in &snapshot.processes {
        for entity in &process.snapshot.entities {
            pairs.push((process.process_id, entity.backtrace.get()));
        }
        for scope in &process.snapshot.scopes {
            pairs.push((process.process_id, scope.backtrace.get()));
        }
        for edge in &process.snapshot.edges {
            pairs.push((process.process_id, edge.backtrace.get()));
        }
        for event in &process.snapshot.events {
            pairs.push((process.process_id, event.backtrace.get()));
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
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
    pairs: &[(ConnectionId, u64)],
) -> SnapshotBacktraceTable {
    if pairs.is_empty() {
        return SnapshotBacktraceTable { frames: vec![] };
    }

    let pairs = pairs.to_vec();
    tokio::task::spawn_blocking(move || load_snapshot_backtrace_table_blocking(&db, &pairs))
        .await
        .unwrap_or_else(|error| panic!("join snapshot backtrace loader: {error}"))
        .unwrap_or_else(|error| panic!("load snapshot backtrace table: {error}"))
}

fn load_snapshot_backtrace_table_blocking(
    db: &Db,
    pairs: &[(ConnectionId, u64)],
) -> Result<SnapshotBacktraceTable, String> {
    // r[impl api.snapshot.frame-catalog]
    let batches = load_backtrace_frame_batches(db, pairs)?;

    let mut frame_id_by_key: BTreeMap<FrameDedupKey, FrameId> = BTreeMap::new();
    let mut frame_by_id: BTreeMap<FrameId, SnapshotBacktraceFrame> = BTreeMap::new();

    for batch in batches {
        for raw in batch.raw_rows {
            let frame = match batch.symbolicated_by_index.get(&raw.frame_index) {
                Some(sym) if sym.status == "resolved" => {
                    match (sym.function_name.as_ref(), sym.source_file_path.as_ref()) {
                        (Some(function_name), Some(source_file)) => {
                            SnapshotBacktraceFrame::Resolved(BacktraceFrameResolved {
                                module_path: sym.module_path.clone(),
                                function_name: function_name.clone(),
                                source_file: source_file.clone(),
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
            match frame_id_by_key.get(&key) {
                Some(existing) => {
                    let existing_frame = frame_by_id.get(existing).cloned().ok_or_else(|| {
                        format!(
                            "invariant violated: frame id {} missing from frame map for backtrace {}",
                            existing.get(),
                            batch.backtrace_id
                        )
                    })?;
                    let merged = merge_frame_state(&existing_frame, &frame, &key)?;
                    frame_by_id.insert(*existing, merged);
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
                            assigned.get(),
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
                }
            };
        }
    }

    let frames = frame_by_id
        .into_iter()
        .map(|(frame_id, frame)| SnapshotFrameRecord { frame_id, frame })
        .collect();

    Ok(SnapshotBacktraceTable { frames })
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

fn stable_frame_id(key: &FrameDedupKey) -> Result<FrameId, String> {
    static FRAME_ID_REGISTRY: OnceLock<Mutex<BTreeMap<FrameDedupKey, FrameId>>> = OnceLock::new();
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
        const JS_SAFE_MAX: u64 = (1u64 << 53) - 1;
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
        assert!(a1.get() > 0 && b.get() > 0, "frame ids must be non-zero");
        assert!(
            a1.get() <= JS_SAFE_MAX && b.get() <= JS_SAFE_MAX,
            "frame ids must remain JS-safe"
        );
    }
}
