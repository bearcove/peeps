use facet::Facet;

use crate::{next_entity_id, EntityId, PTime, SourceId};

/// A: future, a lock, a channel end (tx, rx), a connection leg, a socket, etc.
#[derive(Facet)]
pub struct Entity {
    /// Opaque entity identifier.
    pub id: EntityId,

    /// When we first started tracking this entity
    pub birth: PTime,

    /// Location in source code and crate information.
    pub source: SourceId,

    /// Human-facing name for this entity.
    pub name: String,

    /// More specific info about the entity (depending on its kind)
    pub body: EntityBody,
}

impl Entity {
    /// Create a new entity: ID and birth time are generated automatically.
    pub fn new(source: impl Into<SourceId>, name: impl Into<String>, body: EntityBody) -> Entity {
        Entity {
            id: next_entity_id(),
            birth: PTime::now(),
            source: source.into(),
            name: name.into(),
            body,
        }
    }
}

crate::define_entity_body! {
    pub enum EntityBody {
        // Tokio core and sync primitives
        Future(FutureEntity),
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
}

#[derive(Facet)]
pub struct FutureEntity {}

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
    pub program: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment entries in `KEY=VALUE` form.
    pub env: Vec<String>,
}

#[derive(Facet)]
pub struct FileOpEntity {
    /// File operation type.
    pub op: FileOpKind,
    /// Absolute or process-relative file path.
    pub path: String,
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
    pub addr: String,
}

/// Correlation token for RPC is the request entity id propagated in metadata.
/// The receiver generates a fresh response entity id and emits `request -> response`.
#[derive(Facet)]
pub struct RequestEntity {
    /// RPC method name.
    pub method: String,
    /// Stable, human-oriented preview of request arguments.
    pub args_preview: String,
}

#[derive(Facet)]
pub struct ResponseEntity {
    /// RPC method name this response belongs to.
    pub method: String,
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
