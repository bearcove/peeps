use facet::Facet;
use std::fmt;

use crate::{Edge, EdgeKind, Entity, EntityId, Event, Scope, ScopeId, Snapshot};

/// Monotonic sequence number within one process change stream.
///
/// Sequence numbers are append-only and strictly increasing.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
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
#[facet(transparent)]
pub struct StreamId(pub String);

/// Logical barrier identifier used to coordinate multi-process "cuts".
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct CutId(pub String);

impl fmt::Display for CutId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One canonical graph mutation in the append-only stream.
#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum Change {
    /// Insert or replace entity state.
    UpsertEntity(Entity),
    /// Insert or replace scope state.
    UpsertScope(Scope),
    /// Remove entity and any incident edges in materialized state.
    RemoveEntity { id: EntityId },
    /// Remove scope in materialized state.
    RemoveScope { id: ScopeId },
    /// Insert or replace entity-scope membership state.
    UpsertEntityScopeLink {
        entity_id: EntityId,
        scope_id: ScopeId,
    },
    /// Remove entity-scope membership state.
    RemoveEntityScopeLink {
        entity_id: EntityId,
        scope_id: ScopeId,
    },
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
    /// When present, requested history before this cursor was compacted away.
    ///
    /// Consumers should rebuild from a checkpoint and resume from this cursor.
    #[facet(skip_unless_truthy)]
    pub compacted_before_seq_no: Option<SeqNo>,
}

/// Last durable/applied cursor for one stream.
#[derive(Facet)]
pub struct StreamCursor {
    pub stream_id: StreamId,
    pub next_seq_no: SeqNo,
}

/// Server-to-process request to acknowledge current cursor for a cut.
#[derive(Facet)]
pub struct CutRequest {
    pub cut_id: CutId,
}

/// Process-to-server acknowledgement of cursor for a cut barrier.
#[derive(Facet)]
pub struct CutAck {
    pub cut_id: CutId,
    pub cursor: StreamCursor,
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
