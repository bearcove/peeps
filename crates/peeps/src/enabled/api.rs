use compact_str::CompactString;
use peeps_types::{
    CutAck, CutId, Edge, Entity, EntityId, Event, PullChangesResponse, Scope, ScopeId, SeqNo,
    StreamCursor, StreamId,
};

use super::db::{runtime_db, runtime_stream_id};

pub trait SnapshotSink {
    fn entity(&mut self, entity: &Entity);
    fn scope(&mut self, _scope: &Scope) {}
    fn edge(&mut self, edge: &Edge);
    fn event(&mut self, event: &Event);
}

#[track_caller]
pub fn write_snapshot_to<S>(sink: &mut S)
where
    S: SnapshotSink,
{
    let Ok(db) = runtime_db().lock() else {
        return;
    };
    for entity in db.entities.values() {
        sink.entity(entity);
    }
    for scope in db.scopes.values() {
        sink.scope(scope);
    }
    for edge in db.edges.values() {
        sink.edge(edge);
    }
    for event in &db.events {
        sink.event(event);
    }
}

#[track_caller]
pub fn pull_changes_since(from_seq_no: SeqNo, max_changes: u32) -> PullChangesResponse {
    let stream_id = runtime_stream_id();
    let Ok(db) = runtime_db().lock() else {
        return PullChangesResponse {
            stream_id,
            from_seq_no,
            next_seq_no: from_seq_no,
            changes: Vec::new(),
            truncated: false,
            compacted_before_seq_no: None,
        };
    };
    db.pull_changes_since(from_seq_no, max_changes)
}

#[track_caller]
pub fn current_cursor() -> StreamCursor {
    let stream_id = runtime_stream_id();
    let Ok(db) = runtime_db().lock() else {
        return StreamCursor {
            stream_id,
            next_seq_no: SeqNo::ZERO,
        };
    };
    db.current_cursor()
}

#[track_caller]
pub fn ack_cut(cut_id: impl Into<CompactString>) -> CutAck {
    CutAck {
        cut_id: CutId(cut_id.into()),
        cursor: current_cursor(),
    }
}
