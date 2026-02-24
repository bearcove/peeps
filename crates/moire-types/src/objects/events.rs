use facet::Facet;
use moire_trace_types::BacktraceId;

use crate::{EntityId, EventId, Json, PTime, ScopeId, next_event_id};

// r[impl model.event.fields]
#[derive(Facet)]
pub struct Event {
    /// Opaque event identifier.
    pub id: EventId,

    /// Event timestamp.
    pub at: PTime,

    /// Event source.
    pub backtrace: BacktraceId,

    /// Event target (entity or scope).
    pub target: EventTarget,

    /// Event kind.
    pub kind: EventKind,
}

impl Event {
    /// Builds an event with explicit source context.
    pub fn new(target: EventTarget, kind: EventKind, backtrace: BacktraceId) -> Self {
        Self {
            id: next_event_id(),
            at: PTime::now(),
            backtrace,
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

// r[impl model.event.kinds]
#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum EventKind {
    StateChanged,
    ChannelSent,
    ChannelReceived,
    Custom(CustomEventKind),
}

/// A user-defined event kind with arbitrary payload.
///
/// Library consumers can emit custom events on any entity without modifying moire source.
#[derive(Facet)]
pub struct CustomEventKind {
    /// Event kind identifier (e.g. "query_executed").
    pub kind: String,
    /// Human-readable display name (e.g. "Query Executed").
    pub display_name: String,
    /// Arbitrary structured payload as JSON.
    pub payload: Json,
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
);

#[derive(Facet)]
pub struct ChannelSentEvent {
    /// Observed wait duration in nanoseconds, if this operation suspended.
    pub wait_ns: Option<u64>,
    /// True if the send failed because the other side was gone.
    pub closed: bool,
}

#[derive(Facet)]
pub struct ChannelReceivedEvent {
    /// Observed wait duration in nanoseconds, if this operation suspended.
    pub wait_ns: Option<u64>,
    /// True if the recv failed because the other side was gone.
    pub closed: bool,
}
