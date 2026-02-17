//! Core graph nomenclature used across Peep's runtime model.
//!
//! - `Event`: a point-in-time occurrence with a timestamp.
//! - `Entity`: a runtime thing that exists over time (for example a lock,
//!   future, channel, request, or connection).
//! - `Edge`: a relationship between entities (causal or structural).
//! - `Scope`: an execution container that groups entities (for example a
//!   process, thread, or task).
//!
//! In short: events happen to entities, entities are connected by edges,
//! and entities live inside scopes.

use compact_str::{CompactString, ToCompactString};
use facet::Facet;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

type MetaSerializeError = facet_format::SerializeError<facet_value::ToValueError>;

////////////////////////////////////////////////////////////////////////////////////
// Timestamps
////////////////////////////////////////////////////////////////////////////////////

/// First-use monotonic anchor for process-relative timestamps.
/// "Process birth" is defined as the first call to `PTime::now()`.
fn ptime_anchor() -> &'static Instant {
    static PTIME_ANCHOR: OnceLock<Instant> = OnceLock::new();
    PTIME_ANCHOR.get_or_init(Instant::now)
}

/// process start time + N milliseconds
#[derive(Facet)]
pub struct PTime(u64);

impl PTime {
    pub fn now() -> Self {
        let elapsed_ms = ptime_anchor().elapsed().as_millis().min(u64::MAX as u128) as u64;
        Self(elapsed_ms)
    }
}

////////////////////////////////////////////////////////////////////////////////////
// Snapshots
////////////////////////////////////////////////////////////////////////////////////

/// A snapshot is a point-in-time process envelope of graph state.
#[derive(Facet)]
pub struct Snapshot {
    /// Runtime entities present in this snapshot.
    pub entities: Vec<Entity>,
    /// Execution scopes present in this snapshot.
    pub scopes: Vec<Scope>,
    /// Entity-to-entity edges present in this snapshot.
    pub edges: Vec<Edge>,
    /// Point-in-time events captured for this snapshot.
    pub events: Vec<Event>,
}

////////////////////////////////////////////////////////////////////////////////////
// Scopes
////////////////////////////////////////////////////////////////////////////////////

/// A scope groups execution context over time (for example process/thread/task).
#[derive(Facet)]
pub struct Scope {
    /// Opaque scope identifier.
    pub id: ScopeId,

    /// When we first started tracking this scope.
    pub birth: PTime,

    /// Creation/discovery site in source code as `{absolute_path}:{line}`.
    pub source: CompactString,

    /// Human-facing name for this scope.
    pub name: CompactString,

    /// More specific info about the scope.
    pub body: ScopeBody,

    /// Extensible metadata for optional, non-canonical context.
    pub meta: facet_value::Value,
}

////////////////////////////////////////////////////////////////////////////////////
// Entities
////////////////////////////////////////////////////////////////////////////////////

/// A: future, a lock, a channel end (tx, rx), a connection leg, a socket, etc.
#[derive(Facet)]
pub struct Entity {
    /// Opaque entity identifier.
    pub id: EntityId,

    /// When we first started tracking this entity
    pub birth: PTime,

    /// Creation site in source code as `{absolute_path}:{line}`.
    /// Example: `/Users/amos/bearcove/peeps/crates/peeps/src/sync/channels.rs:1043`
    // [FIXME] Note that this is a good candidate to optimize for later by just keeping a registry of all
    // the files we've ever seen. And then this becomes a tuple of numbers instead of being this
    // very long string.
    pub source: CompactString,

    /// Human-facing name for this entity.
    pub name: CompactString,

    /// More specific info about the entity (depending on its kind)
    pub body: EntityBody,

    /// Extensible metadata for optional, non-canonical context.
    /// Convention: `meta.level` may be `info`, `debug`, or `trace` for UI filtering.
    pub meta: facet_value::Value,
}

/// Opaque textual entity identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(CompactString);

/// Opaque textual scope identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(CompactString);

impl Scope {
    /// Starts building a scope from required semantic fields.
    pub fn builder(name: impl Into<CompactString>, body: ScopeBody) -> ScopeBuilder {
        ScopeBuilder {
            name: name.into(),
            body,
        }
    }

    /// Convenience constructor that accepts typed meta and builds immediately.
    #[track_caller]
    pub fn new<M>(
        name: impl Into<CompactString>,
        body: ScopeBody,
        meta: &M,
    ) -> Result<Self, MetaSerializeError>
    where
        M: for<'facet> Facet<'facet>,
    {
        Scope::builder(name, body).build(meta)
    }
}

impl Entity {
    /// Starts building an entity from required semantic fields.
    pub fn builder(name: impl Into<CompactString>, body: EntityBody) -> EntityBuilder {
        EntityBuilder {
            name: name.into(),
            body,
        }
    }

    /// Convenience constructor that accepts typed meta and builds immediately.
    #[track_caller]
    pub fn new<M>(
        name: impl Into<CompactString>,
        body: EntityBody,
        meta: &M,
    ) -> Result<Self, MetaSerializeError>
    where
        M: for<'facet> Facet<'facet>,
    {
        Entity::builder(name, body).build(meta)
    }
}

/// Builder for `Entity` that auto-fills runtime identity and creation metadata.
pub struct EntityBuilder {
    name: CompactString,
    body: EntityBody,
}

impl EntityBuilder {
    /// Finalizes the entity with typed meta converted into `facet_value::Value`.
    #[track_caller]
    pub fn build<M>(self, meta: &M) -> Result<Entity, MetaSerializeError>
    where
        M: for<'facet> Facet<'facet>,
    {
        Ok(Entity {
            id: next_entity_id(),
            birth: PTime::now(),
            name: self.name,
            source: caller_source(),
            body: self.body,
            meta: facet_value::to_value(meta)?,
        })
    }
}

/// Builder for `Scope` that auto-fills runtime identity and creation metadata.
pub struct ScopeBuilder {
    name: CompactString,
    body: ScopeBody,
}

impl ScopeBuilder {
    /// Finalizes the scope with typed meta converted into `facet_value::Value`.
    #[track_caller]
    pub fn build<M>(self, meta: &M) -> Result<Scope, MetaSerializeError>
    where
        M: for<'facet> Facet<'facet>,
    {
        Ok(Scope {
            id: next_scope_id(),
            birth: PTime::now(),
            name: self.name,
            source: caller_source(),
            body: self.body,
            meta: facet_value::to_value(meta)?,
        })
    }
}

fn next_entity_id() -> EntityId {
    EntityId(next_opaque_id())
}

fn next_scope_id() -> ScopeId {
    ScopeId(next_opaque_id())
}

fn next_event_id() -> EventId {
    EventId(next_opaque_id())
}

fn next_opaque_id() -> CompactString {
    static PROCESS_PREFIX: OnceLock<u16> = OnceLock::new();
    static COUNTER: AtomicU64 = AtomicU64::new(1);

    let prefix = *PROCESS_PREFIX.get_or_init(|| {
        let pid = std::process::id() as u64;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        ((seed ^ pid) & 0xFFFF) as u16
    });

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed) & 0x0000_FFFF_FFFF_FFFF;
    let raw = ((prefix as u64) << 48) | counter;
    PeepsHex2(raw).to_compact_string()
}

#[track_caller]
fn caller_source() -> CompactString {
    let location = std::panic::Location::caller();
    CompactString::from(format!("{}:{}", location.file(), location.line()))
}

/// `peeps-hex-2` formatter:
/// lowercase hex with `a..f` remapped to `p,e,s,P,E,S`.
struct PeepsHex2(u64);

impl fmt::Display for PeepsHex2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const DIGITS: &[u8; 16] = b"0123456789pesPES";
        let mut out = [0u8; 16];
        for (idx, shift) in (0..16).zip((0..64).step_by(4).rev()) {
            let nibble = ((self.0 >> shift) & 0xF) as usize;
            out[idx] = DIGITS[nibble];
        }
        // SAFETY: DIGITS only contains ASCII bytes.
        f.write_str(unsafe { std::str::from_utf8_unchecked(&out) })
    }
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ScopeBody {
    Process,
    Thread,
    Task,
}

/// Typed payload for each entity kind.
///
/// Keep variant names short and domain-focused. Prefer `NetRead` over
/// `NetReadableEntityBody` style names.
#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum EntityBody {
    // Tokio core and sync primitives
    Future,
    Lock(LockEntity),
    ChannelTx(ChannelEndpointEntity),
    ChannelRx(ChannelEndpointEntity),
    Semaphore(SemaphoreEntity),
    Notify(NotifyEntity),
    OnceCell(OnceCellEntity),

    // System and I/O boundaries
    Command(CommandEntity),
    FileOp(FileOpEntity),

    // Network boundaries
    NetConnect(NetEntity),
    NetAccept(NetEntity),
    NetRead(NetEntity),
    NetWrite(NetEntity),

    // RPC lifecycle
    Request(RequestEntity),
    Response(ResponseEntity),
}

#[derive(Facet)]
pub struct LockEntity {
    /// Kind of lock primitive.
    pub kind: LockKind,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum LockKind {
    Mutex,
    RwLock,
    Other,
}

#[derive(Facet)]
pub struct ChannelEndpointEntity {
    /// Endpoint lifecycle state.
    pub lifecycle: ChannelEndpointLifecycle,
    /// Channel-kind-specific runtime details.
    pub details: ChannelDetails,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ChannelEndpointLifecycle {
    Open,
    Closed(ChannelCloseCause),
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ChannelCloseCause {
    SenderDropped,
    ReceiverDropped,
    ReceiverClosed,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ChannelDetails {
    Mpsc(MpscChannelDetails),
    Broadcast(BroadcastChannelDetails),
    Watch(WatchChannelDetails),
    Oneshot(OneshotChannelDetails),
}

#[derive(Facet)]
pub struct MpscChannelDetails {
    /// Queue capacity for bounded channels; `None` means unbounded.
    pub capacity: Option<u32>,
    /// Current number of messages queued in this endpoint.
    pub queue_len: u32,
}

#[derive(Facet)]
pub struct BroadcastChannelDetails {
    /// Ring-buffer capacity.
    pub capacity: u32,
    /// Current number of messages retained in the ring buffer.
    pub queue_len: u32,
}

#[derive(Facet)]
pub struct WatchChannelDetails {
    /// Last update timestamp observed for this watch channel.
    pub last_update_at: Option<PTime>,
}

#[derive(Facet)]
pub struct OneshotChannelDetails {
    /// Current oneshot lifecycle state.
    pub state: OneshotState,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum OneshotState {
    Pending,
    Sent,
    Received,
    SenderDropped,
    ReceiverDropped,
}

#[derive(Facet)]
pub struct SemaphoreEntity {
    /// Total permits configured for this semaphore.
    pub max_permits: u32,
    /// Current number of permits acquired and not yet released.
    pub handed_out_permits: u32,
}

#[derive(Facet)]
pub struct NotifyEntity {
    /// Number of tasks currently waiting on this notify.
    pub waiter_count: u32,
}

#[derive(Facet)]
pub struct OnceCellEntity {
    /// Number of tasks currently waiting for initialization.
    pub waiter_count: u32,
    /// Current once-cell lifecycle state.
    pub state: OnceCellState,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum OnceCellState {
    Empty,
    Initializing,
    Initialized,
}

#[derive(Facet)]
pub struct CommandEntity {
    /// Executable path or program name.
    pub program: CompactString,
    /// Command-line arguments.
    pub args: Vec<CompactString>,
    /// Environment entries in `KEY=VALUE` form.
    pub env: Vec<CompactString>,
}

#[derive(Facet)]
pub struct FileOpEntity {
    /// File operation type.
    pub op: FileOpKind,
    /// Absolute or process-relative file path.
    pub path: CompactString,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum FileOpKind {
    Open,
    Read,
    Write,
    Sync,
    Metadata,
    Remove,
    Rename,
    Other,
}

#[derive(Facet)]
pub struct NetEntity {
    /// Endpoint address string (for example `127.0.0.1:8080`).
    pub addr: CompactString,
}

/// Correlation token for RPC is the request entity id propagated in metadata.
/// The receiver generates a fresh response entity id and emits `request -> response`.
#[derive(Facet)]
pub struct RequestEntity {
    /// RPC method name.
    pub method: CompactString,
    /// Stable, human-oriented preview of request arguments.
    pub args_preview: CompactString,
}

#[derive(Facet)]
pub struct ResponseEntity {
    /// RPC method name this response belongs to.
    pub method: CompactString,
    /// Canonical response outcome.
    pub status: ResponseStatus,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ResponseStatus {
    Ok,
    Error,
    Cancelled,
}

////////////////////////////////////////////////////////////////////////////////////
// Edges
////////////////////////////////////////////////////////////////////////////////////

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

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Waiting/blocked-on relationship.
    Needs,
    /// Closure/cancellation cause relationship.
    ClosedBy,
    /// Structural channel endpoint pairing (`tx -> rx`).
    ChannelLink,
    /// Structural request/response pairing.
    RpcLink,
}

////////////////////////////////////////////////////////////////////////////////////
// Events
////////////////////////////////////////////////////////////////////////////////////

#[derive(Facet)]
pub struct Event {
    /// Opaque event identifier.
    pub id: EventId,
    /// Event timestamp.
    pub at: PTime,
    /// Event source site as `{absolute_path}:{line}`.
    pub source: CompactString,
    /// Event target (entity or scope).
    pub target: EventTarget,
    /// Event kind.
    pub kind: EventKind,
    /// Extensible metadata for optional event details.
    pub meta: facet_value::Value,
}

impl Event {
    /// Builds an event with typed metadata and auto-generated id/timestamp/source.
    #[track_caller]
    pub fn new<M>(
        target: EventTarget,
        kind: EventKind,
        meta: &M,
    ) -> Result<Self, MetaSerializeError>
    where
        M: for<'facet> Facet<'facet>,
    {
        Ok(Self {
            id: next_event_id(),
            at: PTime::now(),
            source: caller_source(),
            target,
            kind,
            meta: facet_value::to_value(meta)?,
        })
    }

    /// Channel send event with typed payload metadata.
    #[track_caller]
    pub fn channel_sent(
        target: EventTarget,
        meta: &ChannelSendEvent,
    ) -> Result<Self, MetaSerializeError> {
        Self::new(target, EventKind::ChannelSent, meta)
    }

    /// Channel receive event with typed payload metadata.
    #[track_caller]
    pub fn channel_received(
        target: EventTarget,
        meta: &ChannelReceiveEvent,
    ) -> Result<Self, MetaSerializeError> {
        Self::new(target, EventKind::ChannelReceived, meta)
    }

    /// Channel closure event with typed payload metadata.
    #[track_caller]
    pub fn channel_closed(
        target: EventTarget,
        meta: &ChannelClosedEvent,
    ) -> Result<Self, MetaSerializeError> {
        Self::new(target, EventKind::ChannelClosed, meta)
    }

    /// Channel wait-start event with typed payload metadata.
    #[track_caller]
    pub fn channel_wait_started(
        target: EventTarget,
        meta: &ChannelWaitStartedEvent,
    ) -> Result<Self, MetaSerializeError> {
        Self::new(target, EventKind::ChannelWaitStarted, meta)
    }

    /// Channel wait-end event with typed payload metadata.
    #[track_caller]
    pub fn channel_wait_ended(
        target: EventTarget,
        meta: &ChannelWaitEndedEvent,
    ) -> Result<Self, MetaSerializeError> {
        Self::new(target, EventKind::ChannelWaitEnded, meta)
    }
}

#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(CompactString);

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
    /// Wait type being started.
    pub kind: ChannelWaitKind,
}

#[derive(Facet)]
pub struct ChannelWaitEndedEvent {
    /// Wait type that ended.
    pub kind: ChannelWaitKind,
    /// Observed wait duration in nanoseconds.
    pub wait_ns: u64,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ChannelWaitKind {
    Send,
    Receive,
    Change,
}
