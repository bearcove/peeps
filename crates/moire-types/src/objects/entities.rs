use facet::Facet;

use crate::{BacktraceId, EntityId, Json, PTime, next_entity_id};

// r[impl model.entity.fields]
/// A: future, a lock, a channel end (tx, rx), a connection leg, a socket, etc.
#[derive(Facet)]
pub struct Entity {
    /// Opaque entity identifier.
    pub id: EntityId,

    /// When we first started tracking this entity
    pub birth: PTime,

    /// When this entity was logically removed (deferred removal).
    /// Present means the entity is dead but kept alive for event references.
    #[facet(skip_unless_truthy)]
    pub removed_at: Option<PTime>,

    /// Backtrace when this edge was created
    pub backtrace: BacktraceId,

    /// Human-facing name for this entity.
    pub name: String,

    /// More specific info about the entity (depending on its kind)
    pub body: EntityBody,
}

impl Entity {
    /// Create a new entity: ID and birth time are generated automatically.
    pub fn new(backtrace: BacktraceId, name: impl Into<String>, body: EntityBody) -> Entity {
        Entity {
            id: next_entity_id(),
            birth: PTime::now(),
            removed_at: None,
            backtrace,
            name: name.into(),
            body,
        }
    }
}

// r[impl model.entity.kinds]
crate::define_entity_body! {
    pub enum EntityBody {
        // Tokio core and sync primitives
        Future(FutureEntity),
        Lock(LockEntity),
        MpscTx(MpscTxEntity),
        MpscRx(MpscRxEntity),
        BroadcastTx(BroadcastTxEntity),
        BroadcastRx(BroadcastRxEntity),
        WatchTx(WatchTxEntity),
        WatchRx(WatchRxEntity),
        OneshotTx(OneshotTxEntity),
        OneshotRx(OneshotRxEntity),
        Semaphore(SemaphoreEntity),
        Notify(NotifyEntity),
        OnceCell(OnceCellEntity),

        // System and I/O boundaries
        Command(CommandEntity),
        FileOp(FileOpEntity),

        // Network boundaries
        NetConnect(NetConnectEntity),
        NetAccept(NetAcceptEntity),
        NetRead(NetReadEntity),
        NetWrite(NetWriteEntity),

        // RPC lifecycle
        Request(RequestEntity),
        Response(ResponseEntity),

        // User-defined
        Custom(CustomEntity),
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
pub struct MpscTxEntity {
    /// Current queue length.
    pub queue_len: u32,
    /// Configured capacity (`None` for unbounded).
    pub capacity: Option<u32>,
}

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MpscRxEntity {}

#[derive(Facet)]
pub struct BroadcastTxEntity {
    pub capacity: u32,
}

#[derive(Facet)]
pub struct BroadcastRxEntity {
    pub lag: u32,
}

#[derive(Facet)]
pub struct WatchTxEntity {
    pub last_update_at: Option<PTime>,
}

#[derive(Facet)]
pub struct WatchRxEntity {}

#[derive(Facet)]
pub struct OneshotTxEntity {
    pub sent: bool,
}

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct OneshotRxEntity {}

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
pub struct NetConnectEntity {
    /// Endpoint address string (for example `127.0.0.1:8080`).
    pub addr: String,
}

#[derive(Facet)]
pub struct NetAcceptEntity {
    /// Endpoint address string (for example `127.0.0.1:8080`).
    pub addr: String,
}

#[derive(Facet)]
pub struct NetReadEntity {
    /// Endpoint address string (for example `127.0.0.1:8080`).
    pub addr: String,
}

#[derive(Facet)]
pub struct NetWriteEntity {
    /// Endpoint address string (for example `127.0.0.1:8080`).
    pub addr: String,
}

/// Correlation token for RPC is the request entity id propagated in metadata.
/// The receiver generates a fresh response entity id and emits `request -> response`.
#[derive(Facet)]
pub struct RequestEntity {
    /// Service name portion of the RPC endpoint.
    ///
    /// Example: for `vfs.lookupItem`, this is `vfs`.
    pub service_name: String,
    /// Method name portion of the RPC endpoint.
    ///
    /// Example: for `vfs.lookupItem`, this is `lookupItem`.
    pub method_name: String,
    /// JSON-encoded request arguments.
    ///
    /// This is always valid JSON and should be `[]` when the method has no args.
    pub args_json: Json,
}

#[derive(Facet)]
pub struct ResponseEntity {
    /// Service name portion of the RPC endpoint.
    pub service_name: String,
    /// Method name portion of the RPC endpoint.
    pub method_name: String,
    /// Response status and payload/error details.
    pub status: ResponseStatus,
}

#[derive(Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ResponseStatus {
    /// Response has not completed yet.
    Pending,
    /// Handler completed successfully with a JSON result payload.
    Ok(Json),
    /// Handler failed with either internal or user-level JSON error data.
    Error(ResponseError),
    /// Request was cancelled before completion.
    Cancelled,
}

#[derive(Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ResponseError {
    /// Runtime/transport/internal error rendered as text.
    Internal(String),
    /// Application/user error represented as JSON.
    UserJson(Json),
}

/// A user-defined entity kind with arbitrary metadata.
///
/// Library consumers can create custom entity kinds without modifying moire source.
/// All fields are user-controlled; the runtime treats them opaquely.
#[derive(Facet)]
pub struct CustomEntity {
    /// Canonical kind identifier (e.g. "database_pool"). snake_case, non-empty.
    pub kind: String,
    /// Human-readable display name (e.g. "Database Pool").
    pub display_name: String,
    /// Category for UI grouping ("async"/"sync"/"channel"/"rpc"/"net"/"fs"/"time"/"meta").
    pub category: String,
    /// Phosphor icon name (e.g. "Database", "Cpu"). Empty string = default icon.
    pub icon: String,
    /// Arbitrary structured metadata as a JSON object string.
    pub attrs: Json,
}
