use facet::Facet;
use peeps_source::SourceId;

use crate::EntityId;

/// Relationship between two entities.
#[derive(Facet)]
pub struct Edge {
    /// Source entity in the causal relationship.
    pub src: EntityId,

    /// Destination entity in the causal relationship.
    pub dst: EntityId,

    /// Location in source code and crate information.
    pub source: SourceId,

    /// Causal edge kind.
    pub kind: EdgeKind,
}

impl Edge {
    /// Builds a causal edge.
    pub fn new(src: EntityId, dst: EntityId, kind: EdgeKind, source: impl Into<SourceId>) -> Self {
        Self {
            src,
            dst,
            source: source.into(),
            kind,
        }
    }
}

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Poll relationship (task/entity is actively polling another future/resource).
    ///
    /// Example: parent future polls child future during one executor tick.
    Polls,

    /// Waiting relationship (task/entity is blocked on another resource).
    ///
    /// Example: receiver waits on channel `rx.recv().await`.
    WaitingOn,

    /// Pairing relationship between two endpoints that form one logical primitive.
    ///
    /// Example: channel sender paired with its corresponding receiver.
    PairedWith,

    /// Resource ownership/lease relationship (resource -> current holder).
    ///
    /// Example: semaphore points to the holder of an acquired permit.
    Holds,
}

crate::impl_sqlite_json!(EdgeKind);

crate::declare_edge_kind_slots!(
    PollsEdgeKindSlot::Polls,
    WaitingOnEdgeKindSlot::WaitingOn,
    PairedWithEdgeKindSlot::PairedWith,
    HoldsEdgeKindSlot::Holds,
);
