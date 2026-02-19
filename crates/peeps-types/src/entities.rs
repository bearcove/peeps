use compact_str::CompactString;
use facet::Facet;
use facet_value::Value;
use peeps_source::Source;

use crate::{
    caller_source_id, intern_source, next_entity_id, EntityId, MetaSerializeError, PTime, SourceId,
};

/// A: future, a lock, a channel end (tx, rx), a connection leg, a socket, etc.
#[derive(Facet)]
pub struct Entity {
    /// Opaque entity identifier.
    pub id: EntityId,

    /// When we first started tracking this entity
    pub birth: PTime,

    /// Interned source identifier.
    /// Resolves to a `{source, krate}` tuple in the source registry.
    pub source: SourceId,

    /// Human-facing name for this entity.
    pub name: CompactString,

    /// More specific info about the entity (depending on its kind)
    pub body: EntityBody,
}

impl Entity {
    /// Create a new entity: ID and birth time are generated automatically.
    pub fn new(
        source: impl Into<SourceId>,
        name: impl Into<CompactString>,
        body: EntityBody,
    ) -> Result<Entity, MetaSerializeError> {
        Ok(Entity {
            id: next_entity_id(),
            birth: PTime::now(),
            source: source.into(),
            name: name.into(),
            body,
        })
    }
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

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ChannelEndpointLifecycle {
    Open,
    Closed(ChannelCloseCause),
}

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Buffer state when observable for this endpoint.
    pub buffer: Option<BufferState>,
}

#[derive(Facet)]
pub struct BroadcastChannelDetails {
    /// Buffer state when observable for this endpoint.
    pub buffer: Option<BufferState>,
}

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferState {
    /// Current number of buffered items.
    pub occupancy: u32,
    /// Maximum buffered items when bounded; `None` means unbounded.
    pub capacity: Option<u32>,
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

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ResponseStatus {
    Pending,
    Ok,
    Error,
    Cancelled,
}
