use compact_str::CompactString;
use facet::Facet;
use peeps_source::SourceId;

use crate::{
    next_event_id, ChannelCloseCause, EntityId, EventId, MetaSerializeError, PTime, ScopeId,
};

/// Relationship between two entities.
#[derive(Facet)]
pub struct Edge {
    /// Source entity in the causal relationship.
    pub src: EntityId,

    /// Destination entity in the causal relationship.
    pub dst: EntityId,

    /// Causal edge kind.
    pub kind: EdgeKind,

    /// Extensible metadata for optional edge context.
    pub meta: facet_value::Value,
}

impl Edge {
    /// Builds a causal edge with typed metadata.
    pub fn new<M>(
        src: EntityId,
        dst: EntityId,
        kind: EdgeKind,
        meta: &M,
    ) -> Result<Self, MetaSerializeError>
    where
        M: for<'facet> Facet<'facet>,
    {
        Ok(Self {
            src,
            dst,
            kind,
            meta: facet_value::to_value(meta)?,
        })
    }
}

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Contextual resource-touch relationship (actor has interacted with resource).
    Touches,

    /// Polled relationship (non-blocking observation of dependency).
    Polls,

    /// Waiting/blocked-on relationship.
    Needs,

    /// Resource ownership relationship (resource -> current holder).
    Holds,

    /// Closure/cancellation cause relationship.
    ClosedBy,

    /// Structural channel endpoint pairing (`tx -> rx`).
    ChannelLink,

    /// Structural request/response pairing.
    RpcLink,
}

/// Primitive operation represented on an operation edge.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum OperationKind {
    Send,
    Recv,
    Acquire,
    Lock,
    NotifyWait,
    OncecellWait,
}

/// Runtime state for a primitive operation edge.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum OperationState {
    Active,
    Pending,
    Done,
    Failed,
    Cancelled,
}

/// Metadata payload for operation edges (`EdgeKind::Needs` + `op_kind`).
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct OperationEdgeMeta {
    pub op_kind: OperationKind,
    pub state: OperationState,
    pub pending_since_ptime_ms: Option<u64>,
    pub last_change_ptime_ms: u64,
    pub source: CompactString,
    pub krate: Option<CompactString>,
    pub poll_count: Option<u64>,
    pub details: Option<facet_value::Value>,
}

#[derive(Facet)]
pub struct Event {
    /// Opaque event identifier.
    pub id: EventId,

    /// Event timestamp.
    pub at: PTime,

    /// Event source
    pub source: SourceId,

    /// Event target (entity or scope).
    pub target: EventTarget,

    /// Event kind.
    pub kind: EventKind,
}

impl Event {
    /// Builds an event with typed metadata and explicit source context.
    pub fn new_with_source<M>(
        target: EventTarget,
        kind: EventKind,
        source: impl Into<SourceId>,
    ) -> Self {
        Self {
            id: next_event_id(),
            at: PTime::now(),
            source: source.into(),
            target,
            kind,
        }
    }
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum EventTarget {
    Entity(EntityId),
    Scope(ScopeId),
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum EventKind {
    StateChanged,
    ChannelSent,
    ChannelReceived,
    ChannelClosed,
    ChannelWaitStarted,
    ChannelWaitEnded,
}

#[derive(Facet)]
pub struct ChannelSendEvent {
    /// Send attempt outcome.
    pub outcome: ChannelSendOutcome,
    /// Queue length after the operation, when observable.
    pub queue_len: Option<u32>,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ChannelSendOutcome {
    Ok,
    Full,
    Closed,
}

#[derive(Facet)]
pub struct ChannelReceiveEvent {
    /// Receive attempt outcome.
    pub outcome: ChannelReceiveOutcome,
    /// Queue length after the operation, when observable.
    pub queue_len: Option<u32>,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ChannelReceiveOutcome {
    Ok,
    Empty,
    Closed,
}

#[derive(Facet)]
pub struct ChannelClosedEvent {
    /// Reason the endpoint transitioned to closed.
    pub cause: ChannelCloseCause,
}

#[derive(Facet)]
pub struct ChannelWaitStartedEvent {
    /// Wait reason being started.
    pub kind: ChannelWaitKind,
}

#[derive(Facet)]
pub struct ChannelWaitEndedEvent {
    /// Wait reason that ended.
    pub kind: ChannelWaitKind,
    /// Observed wait duration in nanoseconds.
    pub wait_ns: u64,
}

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ChannelWaitKind {
    SendFull,
    ReceiveEmpty,
    Change,
}
