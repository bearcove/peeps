use compact_str::CompactString;
use facet::Facet;

use crate::{Edge, EdgeKind, Entity, EntityId, Event, Snapshot};

/// Monotonic sequence number within one process change stream.
///
/// Sequence numbers are append-only and strictly increasing.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeqNo(pub u64);

impl SeqNo {
    pub const ZERO: Self = Self(0);

    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// Identity of one append-only change stream.
///
/// This should come from protocol handshake/session identity and stay stable
/// for the lifetime of that runtime stream.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamId(pub CompactString);

/// One canonical graph mutation in the append-only stream.
#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum Change {
    /// Insert or replace entity state.
    UpsertEntity(Entity),
    /// Remove entity and any incident edges in materialized state.
    RemoveEntity { id: EntityId },
    /// Insert or replace edge state.
    UpsertEdge(Edge),
    /// Remove a specific edge in materialized state.
    RemoveEdge {
        src: EntityId,
        dst: EntityId,
        kind: EdgeKind,
    },
    /// Append event to timeline/event log.
    AppendEvent(Event),
}

/// A sequence-stamped change item.
#[derive(Facet)]
pub struct StampedChange {
    pub seq_no: SeqNo,
    pub change: Change,
}

/// Pull request for change stream deltas.
#[derive(Facet)]
pub struct PullChangesRequest {
    /// Stream identity chosen during process handshake.
    pub stream_id: StreamId,
    /// First sequence number the caller does not yet have.
    pub from_seq_no: SeqNo,
    /// Upper bound on number of changes to return.
    pub max_changes: u32,
}

/// Delta response for one pull step.
#[derive(Facet)]
pub struct PullChangesResponse {
    /// Stream identity chosen during process handshake.
    pub stream_id: StreamId,
    /// Echo of request cursor.
    pub from_seq_no: SeqNo,
    /// Next cursor the caller should request.
    pub next_seq_no: SeqNo,
    /// Returned changes in ascending `seq_no`.
    pub changes: Vec<StampedChange>,
    /// `true` when additional changes are available after this batch.
    pub truncated: bool,
}

/// Last durable/applied cursor for one stream.
#[derive(Facet)]
pub struct StreamCursor {
    pub stream_id: StreamId,
    pub next_seq_no: SeqNo,
}

/// Optional periodic checkpoint to bound replay time.
///
/// Checkpoints are full materialized state; deltas after `at_seq_no`
/// replay on top.
#[derive(Facet)]
pub struct DiffCheckpoint {
    pub stream_id: StreamId,
    pub at_seq_no: SeqNo,
    pub snapshot: Snapshot,
}
