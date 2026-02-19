use facet::Facet;
use peeps_source::SourceId;

use crate::{next_event_id, ChannelCloseCause, EntityId, EventId, PTime, ScopeId};

#[derive(Facet)]
pub struct Event {
    /// Opaque event identifier.
    pub id: EventId,

    /// Event timestamp.
    pub at: PTime,

    /// Event source.
    pub source: SourceId,

    /// Event target (entity or scope).
    pub target: EventTarget,

    /// Event kind.
    pub kind: EventKind,
}

impl Event {
    /// Builds an event with explicit source context.
    pub fn new_with_source(
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

crate::impl_sqlite_json!(EventTarget);
crate::impl_sqlite_json!(EventKind);

crate::declare_event_target_slots!(
    EntityTargetSlot::Entity(EntityId),
    ScopeTargetSlot::Scope(ScopeId),
);

crate::declare_event_kind_slots!(
    StateChangedKindSlot::StateChanged,
    ChannelSentKindSlot::ChannelSent,
    ChannelReceivedKindSlot::ChannelReceived,
    ChannelClosedKindSlot::ChannelClosed,
    ChannelWaitStartedKindSlot::ChannelWaitStarted,
    ChannelWaitEndedKindSlot::ChannelWaitEnded,
);

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
