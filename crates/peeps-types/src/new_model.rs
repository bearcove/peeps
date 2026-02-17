//! Core graph nomenclature used across Peep's runtime model.
//!
//! - `Event`: a point-in-time occurrence with a timestamp.
//! - `Entity`: a runtime thing that exists over time (for example a lock,
//!   future, channel, request, or connection).
//! - `Edge`: a causal dependency relationship between entities.
//! - `Scope`: an execution container that groups entities (for example a
//!   process, thread, or task).
//!
//! In short: events happen to entities, entities are connected by edges,
//! and entities live inside scopes.

use compact_str::CompactString;
use facet::Facet;
use std::sync::OnceLock;
use std::time::Instant;

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
// Scopes
////////////////////////////////////////////////////////////////////////////////////

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

    /// Human-facing name for this entity.
    pub name: CompactString,

    /// Instrumentation verbosity level for this entity.
    pub level: EntityLevel,

    /// Creation site in source code as `{absolute_path}:{line}`.
    /// Example: `/Users/amos/bearcove/peeps/crates/peeps/src/sync/channels.rs:1043`
    // [FIXME] Note that this is a good candidate to optimize for later by just keeping a registry of all
    // the files we've ever seen. And then this becomes a tuple of numbers instead of being this
    // very long string.
    pub source: CompactString,

    /// More specific info about the entity (depending on its kind)
    pub body: EntityBody,

    /// Extensible metadata for optional, non-canonical context.
    pub meta: facet_value::Value,
}

/// Opaque textual entity identifier suitable for wire formats and JS runtimes.
#[derive(Facet)]
pub struct EntityId(CompactString);

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum EntityLevel {
    Info,
    Debug,
    Trace,
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
    /// Channel-kind-specific runtime details.
    pub details: ChannelDetails,
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
    /// Whether the oneshot value has been sent.
    pub sent: bool,
    /// Whether the oneshot value has been received.
    pub received: bool,
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
    /// Whether the cell has already been initialized.
    pub initialized: bool,
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

////////////////////////////////////////////////////////////////////////////////////
// Events
////////////////////////////////////////////////////////////////////////////////////

#[derive(Facet)]
pub struct Event {}
