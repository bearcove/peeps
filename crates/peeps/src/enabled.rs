use compact_str::CompactString;
use peeps_types::set_inference_source_root;
use peeps_types::PTime;
use peeps_types::{
    BufferState, Change, ChannelCloseCause, ChannelClosedEvent, ChannelDetails,
    ChannelEndpointEntity, ChannelEndpointLifecycle, ChannelReceiveEvent, ChannelReceiveOutcome,
    ChannelSendEvent, ChannelSendOutcome, ChannelWaitEndedEvent, ChannelWaitKind,
    ChannelWaitStartedEvent, CommandEntity, CutAck, CutId, Edge, EdgeKind, Entity, EntityBody,
    EntityId, Event, EventKind, EventTarget, LockEntity, LockKind, MpscChannelDetails,
    NotifyEntity, OnceCellEntity, OnceCellState, OneshotChannelDetails, OneshotState,
    OperationEdgeMeta, OperationKind, OperationState, PullChangesResponse, RequestEntity,
    ResponseEntity, ResponseStatus, Scope, ScopeBody, ScopeId, SemaphoreEntity, SeqNo,
    StampedChange, StreamCursor, StreamId, WatchChannelDetails,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::future::{Future, IntoFuture};
use std::io;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::process::{ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::time::MissedTickBehavior;

use peeps_wire::{
    decode_server_message_default, encode_client_message_default, ClientMessage, ServerMessage,
};

const MAX_EVENTS: usize = 16_384;
const MAX_CHANGES_BEFORE_COMPACT: usize = 65_536;
const COMPACT_TARGET_CHANGES: usize = 8_192;
const DEFAULT_STREAM_ID_PREFIX: &str = "proc";
static PROCESS_SCOPE: OnceLock<ScopeHandle> = OnceLock::new();
const DASHBOARD_PUSH_MAX_CHANGES: u32 = 2048;
const DASHBOARD_PUSH_INTERVAL_MS: u64 = 100;
const DASHBOARD_RECONNECT_DELAY_MS: u64 = 500;
tokio::task_local! {
    static FUTURE_CAUSAL_STACK: RefCell<Vec<EntityId>>;
}
thread_local! {
    static HELD_MUTEX_STACK: RefCell<Vec<EntityId>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug)]
pub struct Source {
    location: &'static std::panic::Location<'static>,
}

impl Source {
    #[track_caller]
    pub fn caller() -> Self {
        Self {
            location: std::panic::Location::caller(),
        }
    }

    fn into_compact_string(self) -> CompactString {
        CompactString::from(format!("{}:{}", self.location.file(), self.location.line()))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PeepsContext {
    manifest_dir: &'static str,
}

impl PeepsContext {
    pub const fn new(manifest_dir: &'static str) -> Self {
        Self { manifest_dir }
    }

    #[track_caller]
    pub const fn caller(manifest_dir: &'static str) -> Self {
        Self::new(manifest_dir)
    }

    pub const fn manifest_dir(self) -> &'static str {
        self.manifest_dir
    }
}

#[track_caller]
#[doc(hidden)]
pub fn __init_from_macro(manifest_dir: &str) {
    set_inference_source_root(std::path::PathBuf::from(manifest_dir));
    let process_name = process_name_auto();
    ensure_process_scope(&process_name);
    init_dashboard_push_loop(&process_name);
}

fn ensure_process_scope(process_name: &str) {
    PROCESS_SCOPE.get_or_init(|| ScopeHandle::new(process_name, ScopeBody::Process, Source::caller()));
}

fn process_name_auto() -> CompactString {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|name| name.to_owned()))
        .and_then(|name| name.into_string().ok())
        .filter(|name| !name.is_empty())
        .map(CompactString::from)
        .unwrap_or_else(|| CompactString::from("process"))
}

fn current_process_scope_id() -> Option<ScopeId> {
    PROCESS_SCOPE
        .get()
        .map(|scope| ScopeId::new(scope.id().as_str()))
}

fn init_dashboard_push_loop(process_name: &str) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    let Some(addr) = std::env::var("PEEPS_DASHBOARD")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    else {
        return;
    };

    let process_name = CompactString::from(process_name);

    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::spawn(async move {
            run_dashboard_push_loop(addr, process_name).await;
        });
        return;
    }

    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            rt.block_on(async move {
                run_dashboard_push_loop(addr, process_name).await;
            });
        }
    });
}

async fn run_dashboard_push_loop(addr: String, process_name: CompactString) {
    loop {
        let connected = run_dashboard_session(&addr, process_name.clone()).await;
        let _ = connected;
        tokio::time::sleep(Duration::from_millis(DASHBOARD_RECONNECT_DELAY_MS)).await;
    }
}

async fn run_dashboard_session(addr: &str, process_name: CompactString) -> Result<(), String> {
    let stream = TcpStream::connect(addr)
        .await
        .map_err(|e| format!("dashboard connect: {e}"))?;
    let (mut reader, mut writer) = stream.into_split();

    let handshake = ClientMessage::Handshake(peeps_wire::Handshake {
        process_name: process_name.clone(),
        pid: std::process::id(),
    });
    write_client_message(&mut writer, &handshake).await?;

    let mut cursor = SeqNo::ZERO;
    let mut ticker = tokio::time::interval(Duration::from_millis(DASHBOARD_PUSH_INTERVAL_MS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let requested_from = cursor;
                let batch = pull_changes_since(cursor, DASHBOARD_PUSH_MAX_CHANGES);
                let cursor_shifted = batch.from_seq_no > requested_from || batch.next_seq_no > requested_from;
                if !batch.changes.is_empty() || batch.truncated || cursor_shifted {
                    let next = batch.next_seq_no;
                    write_client_message(&mut writer, &ClientMessage::DeltaBatch(batch)).await?;
                    cursor = next.max(cursor);
                } else {
                    cursor = batch.next_seq_no.max(cursor);
                }
            }
            inbound = read_server_message(&mut reader) => {
                let Some(message) = inbound? else {
                    return Ok(());
                };
                match message {
                    ServerMessage::CutRequest(request) => {
                        let ack = ack_cut(request.cut_id.0.clone());
                        write_client_message(&mut writer, &ClientMessage::CutAck(ack)).await?;
                    }
                    ServerMessage::SnapshotRequest(request) => {
                        let reply = build_snapshot_reply(request.snapshot_id);
                        write_client_message(&mut writer, &ClientMessage::SnapshotReply(reply))
                            .await?;
                    }
                }
            }
        }
    }
}

async fn write_client_message(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    message: &ClientMessage,
) -> Result<(), String> {
    let frame = encode_client_message_default(message)
        .map_err(|e| format!("encode client message: {e}"))?;
    writer
        .write_all(&frame)
        .await
        .map_err(|e| format!("write frame: {e}"))?;
    Ok(())
}

fn build_snapshot_reply(snapshot_id: i64) -> peeps_wire::SnapshotReply {
    // Capture process-relative now before locking the db, so the timestamp
    // represents the moment this snapshot was requested.
    let ptime_now_ms = PTime::now().as_millis();

    let (entity_bytes, scope_bytes, edge_bytes, event_bytes): (
        Vec<Vec<u8>>,
        Vec<Vec<u8>>,
        Vec<Vec<u8>>,
        Vec<Vec<u8>>,
    ) = {
        let Ok(db) = runtime_db().lock() else {
            return peeps_wire::SnapshotReply {
                snapshot_id,
                ptime_now_ms,
                snapshot: None,
            };
        };
        (
            db.entities
                .values()
                .filter_map(|e| facet_json::to_vec(e).ok())
                .collect(),
            db.scopes
                .values()
                .filter_map(|s| facet_json::to_vec(s).ok())
                .collect(),
            db.edges
                .values()
                .filter_map(|e| facet_json::to_vec(e).ok())
                .collect(),
            db.events
                .iter()
                .filter_map(|e| facet_json::to_vec(e).ok())
                .collect(),
        )
    };

    let snapshot = peeps_types::Snapshot {
        entities: entity_bytes
            .iter()
            .filter_map(|b| facet_json::from_slice(b).ok())
            .collect(),
        scopes: scope_bytes
            .iter()
            .filter_map(|b| facet_json::from_slice(b).ok())
            .collect(),
        edges: edge_bytes
            .iter()
            .filter_map(|b| facet_json::from_slice(b).ok())
            .collect(),
        events: event_bytes
            .iter()
            .filter_map(|b| facet_json::from_slice(b).ok())
            .collect(),
    };

    peeps_wire::SnapshotReply {
        snapshot_id,
        ptime_now_ms,
        snapshot: Some(snapshot),
    }
}

async fn read_server_message(
    reader: &mut tokio::net::tcp::OwnedReadHalf,
) -> Result<Option<ServerMessage>, String> {
    let mut len_buf = [0u8; 4];
    if let Err(e) = reader.read_exact(&mut len_buf).await {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(format!("read frame len: {e}"));
    }
    let payload_len = u32::from_be_bytes(len_buf) as usize;
    if payload_len > peeps_wire::DEFAULT_MAX_FRAME_BYTES {
        return Err(format!("server frame too large: {payload_len}"));
    }
    let mut payload = vec![0u8; payload_len];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|e| format!("read frame payload: {e}"))?;
    let mut framed = Vec::with_capacity(4 + payload_len);
    framed.extend_from_slice(&len_buf);
    framed.extend_from_slice(&payload);
    let message = decode_server_message_default(&framed)
        .map_err(|e| format!("decode server message: {e}"))?;
    Ok(Some(message))
}
#[track_caller]
pub fn spawn_tracked<F>(
    name: impl Into<CompactString>,
    fut: F,
    source: Source,
) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(
        FUTURE_CAUSAL_STACK.scope(
            RefCell::new(Vec::new()),
            instrument_future_named(name, fut, source),
        ),
    )
}

#[macro_export]
macro_rules! spawn_tracked {
    ($name:expr, $fut:expr $(,)?) => {
        $crate::spawn_tracked($name, $fut, $crate::Source::caller())
    };
}
#[track_caller]
pub fn spawn_blocking_tracked<F, T>(
    name: impl Into<CompactString>,
    f: F,
    source: Source,
) -> tokio::task::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let handle = EntityHandle::new(name, EntityBody::Future, source);
    tokio::task::spawn_blocking(move || {
        let _hold = handle;
        f()
    })
}

#[macro_export]
macro_rules! spawn_blocking_tracked {
    ($name:expr, $f:expr $(,)?) => {
        $crate::spawn_blocking_tracked($name, $f, $crate::Source::caller())
    };
}
#[track_caller]
pub fn sleep(duration: std::time::Duration, _label: impl Into<String>) -> impl Future<Output = ()> {
    // we've decided to stop instrumenting sleeps
    tokio::time::sleep(duration)
}

#[macro_export]
macro_rules! sleep {
    ($duration:expr, $label:expr $(,)?) => {
        $crate::sleep($duration, $label)
    };
}
#[track_caller]
#[allow(clippy::manual_async_fn)]
pub fn timeout<F>(
    duration: std::time::Duration,
    future: F,
    _label: impl Into<String>,
) -> impl Future<Output = Result<F::Output, tokio::time::error::Elapsed>>
where
    F: Future,
{
    // we've decided to stop instrumenting timeouts
    tokio::time::timeout(duration, future)
}

#[macro_export]
macro_rules! timeout {
    ($duration:expr, $future:expr, $label:expr $(,)?) => {
        $crate::timeout($duration, $future, $label)
    };
}

fn runtime_db() -> &'static StdMutex<RuntimeDb> {
    static DB: OnceLock<StdMutex<RuntimeDb>> = OnceLock::new();
    DB.get_or_init(|| StdMutex::new(RuntimeDb::new(runtime_stream_id(), MAX_EVENTS)))
}

fn runtime_stream_id() -> StreamId {
    static STREAM_ID: OnceLock<StreamId> = OnceLock::new();
    STREAM_ID
        .get_or_init(|| {
            StreamId(CompactString::from(format!(
                "{DEFAULT_STREAM_ID_PREFIX}:{}",
                std::process::id()
            )))
        })
        .clone()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct EdgeKey {
    src: EntityId,
    dst: EntityId,
    kind: EdgeKind,
}

struct RuntimeDb {
    stream_id: StreamId,
    next_seq_no: SeqNo,
    compacted_before_seq_no: Option<SeqNo>,
    entities: BTreeMap<EntityId, Entity>,
    scopes: BTreeMap<ScopeId, Scope>,
    entity_scope_links: BTreeMap<(EntityId, ScopeId), ()>,
    edges: BTreeMap<EdgeKey, Edge>,
    events: VecDeque<Event>,
    changes: VecDeque<InternalStampedChange>,
    max_events: usize,
}

impl RuntimeDb {
    fn new(stream_id: StreamId, max_events: usize) -> Self {
        Self {
            stream_id,
            next_seq_no: SeqNo::ZERO,
            compacted_before_seq_no: None,
            entities: BTreeMap::new(),
            scopes: BTreeMap::new(),
            entity_scope_links: BTreeMap::new(),
            edges: BTreeMap::new(),
            events: VecDeque::with_capacity(max_events.min(256)),
            changes: VecDeque::new(),
            max_events,
        }
    }

    fn push_change(&mut self, change: InternalChange) {
        let seq_no = self.next_seq_no;
        self.next_seq_no = self.next_seq_no.next();
        self.changes
            .push_back(InternalStampedChange { seq_no, change });
        if self.changes.len() > MAX_CHANGES_BEFORE_COMPACT {
            self.compact_changes();
        }
    }

    fn compact_changes(&mut self) {
        let old_front = self.changes.front().map(|c| c.seq_no);
        if self.changes.len() <= COMPACT_TARGET_CHANGES {
            return;
        }

        let mut keep_seq: BTreeMap<SeqNo, ()> = BTreeMap::new();
        let mut seen_entities: BTreeMap<EntityId, ()> = BTreeMap::new();
        let mut seen_scopes: BTreeMap<ScopeId, ()> = BTreeMap::new();
        let mut seen_entity_scope_links: BTreeMap<(EntityId, ScopeId), ()> = BTreeMap::new();
        let mut seen_edges: BTreeMap<EdgeKey, ()> = BTreeMap::new();

        for stamped in self.changes.iter().rev() {
            match &stamped.change {
                InternalChange::AppendEvent { .. } => {
                    keep_seq.insert(stamped.seq_no, ());
                }
                InternalChange::UpsertEntity { id, .. } | InternalChange::RemoveEntity { id } => {
                    if !seen_entities.contains_key(id) {
                        seen_entities.insert(EntityId::new(id.as_str()), ());
                        keep_seq.insert(stamped.seq_no, ());
                    }
                }
                InternalChange::UpsertScope { id, .. } | InternalChange::RemoveScope { id } => {
                    if !seen_scopes.contains_key(id) {
                        seen_scopes.insert(ScopeId::new(id.as_str()), ());
                        keep_seq.insert(stamped.seq_no, ());
                    }
                }
                InternalChange::UpsertEntityScopeLink {
                    entity_id,
                    scope_id,
                }
                | InternalChange::RemoveEntityScopeLink {
                    entity_id,
                    scope_id,
                } => {
                    let key = (
                        EntityId::new(entity_id.as_str()),
                        ScopeId::new(scope_id.as_str()),
                    );
                    if !seen_entity_scope_links.contains_key(&key) {
                        seen_entity_scope_links.insert(key, ());
                        keep_seq.insert(stamped.seq_no, ());
                    }
                }
                InternalChange::UpsertEdge { src, dst, kind, .. }
                | InternalChange::RemoveEdge { src, dst, kind } => {
                    let key = EdgeKey {
                        src: EntityId::new(src.as_str()),
                        dst: EntityId::new(dst.as_str()),
                        kind: *kind,
                    };
                    if !seen_edges.contains_key(&key) {
                        seen_edges.insert(key, ());
                        keep_seq.insert(stamped.seq_no, ());
                    }
                }
            }
            if keep_seq.len() >= COMPACT_TARGET_CHANGES {
                break;
            }
        }

        if keep_seq.len() == self.changes.len() {
            return;
        }

        self.changes.retain(|c| keep_seq.contains_key(&c.seq_no));
        let new_front = self.changes.front().map(|c| c.seq_no);
        if let (Some(old_front), Some(new_front)) = (old_front, new_front) {
            if new_front > old_front {
                self.compacted_before_seq_no = Some(
                    self.compacted_before_seq_no
                        .map(|existing| existing.max(new_front))
                        .unwrap_or(new_front),
                );
            }
        }
        // TODO: replace this with checkpoint-aware compaction once we plumb
        // checkpoint materialization and replay handoff.
    }

    fn upsert_entity(&mut self, entity: Entity) {
        let entity_id = EntityId::new(entity.id.as_str());
        let entity_json = facet_json::to_vec(&entity).ok();
        self.entities
            .insert(EntityId::new(entity.id.as_str()), entity);
        if let Some(scope_id) = current_process_scope_id() {
            self.link_entity_to_scope(&entity_id, &scope_id);
        }
        if let Some(entity_json) = entity_json {
            self.push_change(InternalChange::UpsertEntity {
                id: entity_id,
                entity_json,
            });
        }
    }

    fn upsert_scope(&mut self, scope: Scope) {
        let scope_id = ScopeId::new(scope.id.as_str());
        let scope_json = facet_json::to_vec(&scope).ok();
        self.scopes.insert(ScopeId::new(scope.id.as_str()), scope);
        if let Some(scope_json) = scope_json {
            self.push_change(InternalChange::UpsertScope {
                id: scope_id,
                scope_json,
            });
        }
    }

    fn update_channel_endpoint_state(
        &mut self,
        id: &EntityId,
        lifecycle: ChannelEndpointLifecycle,
        buffer: Option<BufferState>,
    ) {
        let Some(entity) = self.entities.get_mut(id) else {
            return;
        };

        let mut changed = false;
        match &mut entity.body {
            EntityBody::ChannelTx(endpoint) | EntityBody::ChannelRx(endpoint) => {
                if endpoint.lifecycle != lifecycle {
                    endpoint.lifecycle = lifecycle;
                    changed = true;
                }
                match &mut endpoint.details {
                    ChannelDetails::Mpsc(details) => {
                        if details.buffer != buffer {
                            details.buffer = buffer;
                            changed = true;
                        }
                    }
                    ChannelDetails::Broadcast(details) => {
                        if details.buffer != buffer {
                            details.buffer = buffer;
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }
            _ => return,
        }

        if !changed {
            return;
        }
        if let Some(entity_json) = facet_json::to_vec(entity).ok() {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }
    }

    fn update_oneshot_endpoint_state(
        &mut self,
        id: &EntityId,
        lifecycle: ChannelEndpointLifecycle,
        state: OneshotState,
    ) {
        let Some(entity) = self.entities.get_mut(id) else {
            return;
        };

        let mut changed = false;
        match &mut entity.body {
            EntityBody::ChannelTx(endpoint) | EntityBody::ChannelRx(endpoint) => {
                if endpoint.lifecycle != lifecycle {
                    endpoint.lifecycle = lifecycle;
                    changed = true;
                }
                if let ChannelDetails::Oneshot(details) = &mut endpoint.details {
                    if details.state != state {
                        details.state = state;
                        changed = true;
                    }
                }
            }
            _ => return,
        }

        if !changed {
            return;
        }
        if let Some(entity_json) = facet_json::to_vec(entity).ok() {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }
    }

    fn update_watch_last_update(
        &mut self,
        id: &EntityId,
        last_update_at: Option<peeps_types::PTime>,
    ) {
        let Some(entity) = self.entities.get_mut(id) else {
            return;
        };
        let mut changed = false;
        match &mut entity.body {
            EntityBody::ChannelTx(endpoint) | EntityBody::ChannelRx(endpoint) => {
                if let ChannelDetails::Watch(details) = &mut endpoint.details {
                    if details.last_update_at != last_update_at {
                        details.last_update_at = last_update_at;
                        changed = true;
                    }
                }
            }
            _ => return,
        }
        if !changed {
            return;
        }
        if let Some(entity_json) = facet_json::to_vec(entity).ok() {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }
    }

    fn update_notify_waiter_count(&mut self, id: &EntityId, waiter_count: u32) {
        let Some(entity) = self.entities.get_mut(id) else {
            return;
        };
        let mut changed = false;
        match &mut entity.body {
            EntityBody::Notify(notify) => {
                if notify.waiter_count != waiter_count {
                    notify.waiter_count = waiter_count;
                    changed = true;
                }
            }
            _ => return,
        }
        if !changed {
            return;
        }
        if let Some(entity_json) = facet_json::to_vec(entity).ok() {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }
    }

    fn update_once_cell_state(&mut self, id: &EntityId, waiter_count: u32, state: OnceCellState) {
        let Some(entity) = self.entities.get_mut(id) else {
            return;
        };
        let mut changed = false;
        match &mut entity.body {
            EntityBody::OnceCell(once_cell) => {
                if once_cell.waiter_count != waiter_count {
                    once_cell.waiter_count = waiter_count;
                    changed = true;
                }
                if once_cell.state != state {
                    once_cell.state = state;
                    changed = true;
                }
            }
            _ => return,
        }
        if !changed {
            return;
        }
        if let Some(entity_json) = facet_json::to_vec(entity).ok() {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }
    }

    fn update_semaphore_state(&mut self, id: &EntityId, max_permits: u32, handed_out_permits: u32) {
        let Some(entity) = self.entities.get_mut(id) else {
            return;
        };
        let mut changed = false;
        match &mut entity.body {
            EntityBody::Semaphore(semaphore) => {
                if semaphore.max_permits != max_permits {
                    semaphore.max_permits = max_permits;
                    changed = true;
                }
                if semaphore.handed_out_permits != handed_out_permits {
                    semaphore.handed_out_permits = handed_out_permits;
                    changed = true;
                }
            }
            _ => return,
        }
        if !changed {
            return;
        }
        if let Some(entity_json) = facet_json::to_vec(entity).ok() {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }
    }

    fn update_response_status(&mut self, id: &EntityId, status: ResponseStatus) -> bool {
        let Some(entity) = self.entities.get_mut(id) else {
            return false;
        };

        let mut changed = false;
        match &mut entity.body {
            EntityBody::Response(response) => {
                if response.status != status {
                    response.status = status;
                    changed = true;
                }
            }
            _ => return false,
        }

        if !changed {
            return false;
        }
        if let Some(entity_json) = facet_json::to_vec(entity).ok() {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }
        true
    }

    fn remove_entity(&mut self, id: &EntityId) {
        if self.entities.remove(id).is_none() {
            return;
        }
        let mut links_to_remove = Vec::new();
        for (entity_scope, _) in &self.entity_scope_links {
            if &entity_scope.0 == id {
                links_to_remove.push((
                    EntityId::new(entity_scope.0.as_str()),
                    ScopeId::new(entity_scope.1.as_str()),
                ));
            }
        }
        for (entity_id, scope_id) in links_to_remove {
            self.unlink_entity_from_scope(&entity_id, &scope_id);
        }
        let mut removed_edges: Vec<(EntityId, EntityId, EdgeKind)> = Vec::new();
        self.edges.retain(|k, _| {
            let remove = &k.src == id || &k.dst == id;
            if remove {
                removed_edges.push((
                    EntityId::new(k.src.as_str()),
                    EntityId::new(k.dst.as_str()),
                    k.kind,
                ));
            }
            !remove
        });
        for (src, dst, kind) in removed_edges {
            self.push_change(InternalChange::RemoveEdge { src, dst, kind });
        }
        self.push_change(InternalChange::RemoveEntity {
            id: EntityId::new(id.as_str()),
        });
    }

    fn remove_scope(&mut self, id: &ScopeId) {
        if self.scopes.remove(id).is_none() {
            return;
        }
        let mut links_to_remove = Vec::new();
        for (entity_scope, _) in &self.entity_scope_links {
            if &entity_scope.1 == id {
                links_to_remove.push((
                    EntityId::new(entity_scope.0.as_str()),
                    ScopeId::new(entity_scope.1.as_str()),
                ));
            }
        }
        for (entity_id, scope_id) in links_to_remove {
            self.unlink_entity_from_scope(&entity_id, &scope_id);
        }
        self.push_change(InternalChange::RemoveScope {
            id: ScopeId::new(id.as_str()),
        });
    }

    fn link_entity_to_scope(&mut self, entity_id: &EntityId, scope_id: &ScopeId) {
        let key = (
            EntityId::new(entity_id.as_str()),
            ScopeId::new(scope_id.as_str()),
        );
        if self.entity_scope_links.contains_key(&key) {
            return;
        }
        self.entity_scope_links.insert(
            (
                EntityId::new(entity_id.as_str()),
                ScopeId::new(scope_id.as_str()),
            ),
            (),
        );
        self.push_change(InternalChange::UpsertEntityScopeLink {
            entity_id: EntityId::new(entity_id.as_str()),
            scope_id: ScopeId::new(scope_id.as_str()),
        });
    }

    fn unlink_entity_from_scope(&mut self, entity_id: &EntityId, scope_id: &ScopeId) {
        let key = (
            EntityId::new(entity_id.as_str()),
            ScopeId::new(scope_id.as_str()),
        );
        if self.entity_scope_links.remove(&key).is_none() {
            return;
        }
        self.push_change(InternalChange::RemoveEntityScopeLink {
            entity_id: EntityId::new(entity_id.as_str()),
            scope_id: ScopeId::new(scope_id.as_str()),
        });
    }

    fn upsert_edge(&mut self, src: &EntityId, dst: &EntityId, kind: EdgeKind) {
        self.upsert_edge_with_meta(src, dst, kind, facet_value::Value::NULL);
    }

    fn upsert_edge_with_meta(
        &mut self,
        src: &EntityId,
        dst: &EntityId,
        kind: EdgeKind,
        meta: facet_value::Value,
    ) {
        let key = EdgeKey {
            src: EntityId::new(src.as_str()),
            dst: EntityId::new(dst.as_str()),
            kind,
        };
        if let Some(existing) = self.edges.get(&key) {
            if existing.meta == meta {
                return;
            }
        }
        let edge = Edge {
            src: EntityId::new(src.as_str()),
            dst: EntityId::new(dst.as_str()),
            kind,
            meta,
        };
        let edge_json = facet_json::to_vec(&edge).ok();
        self.edges.insert(key, edge);
        if let Some(edge_json) = edge_json {
            self.push_change(InternalChange::UpsertEdge {
                src: EntityId::new(src.as_str()),
                dst: EntityId::new(dst.as_str()),
                kind,
                edge_json,
            });
        }
    }

    fn remove_edge(&mut self, src: &EntityId, dst: &EntityId, kind: EdgeKind) {
        let removed = self.edges.remove(&EdgeKey {
            src: EntityId::new(src.as_str()),
            dst: EntityId::new(dst.as_str()),
            kind,
        });
        if removed.is_some() {
            self.push_change(InternalChange::RemoveEdge {
                src: EntityId::new(src.as_str()),
                dst: EntityId::new(dst.as_str()),
                kind,
            });
        }
    }

    fn record_event(&mut self, event: Event) {
        let event_json = facet_json::to_vec(&event).ok();
        self.events.push_back(event);
        while self.events.len() > self.max_events {
            self.events.pop_front();
        }
        if let Some(event_json) = event_json {
            self.push_change(InternalChange::AppendEvent { event_json });
        }
    }

    fn pull_changes_since(&self, from_seq_no: SeqNo, max_changes: u32) -> PullChangesResponse {
        let compacted_before = self.compacted_before_seq_no;
        let effective_from = compacted_before
            .map(|compacted| {
                if from_seq_no < compacted {
                    compacted
                } else {
                    from_seq_no
                }
            })
            .unwrap_or(from_seq_no);
        let mut changes: Vec<StampedChange> = Vec::new();
        let limit = max_changes as usize;
        if limit == 0 {
            let truncated = self.changes.iter().any(|c| c.seq_no >= effective_from);
            return PullChangesResponse {
                stream_id: self.stream_id.clone(),
                from_seq_no: effective_from,
                next_seq_no: effective_from,
                changes,
                truncated,
                compacted_before_seq_no: compacted_before,
            };
        }

        let mut scanned = 0usize;
        let mut truncated = false;
        let mut next_seq_no = effective_from;
        for change in &self.changes {
            if change.seq_no < effective_from {
                continue;
            }
            if scanned >= limit {
                truncated = true;
                break;
            }
            scanned += 1;
            next_seq_no = change.seq_no.next();
            if let Some(decoded) = change.to_change() {
                changes.push(StampedChange {
                    seq_no: change.seq_no,
                    change: decoded,
                });
            }
        }

        PullChangesResponse {
            stream_id: self.stream_id.clone(),
            from_seq_no: effective_from,
            next_seq_no,
            changes,
            truncated,
            compacted_before_seq_no: compacted_before,
        }
    }

    fn current_cursor(&self) -> StreamCursor {
        StreamCursor {
            stream_id: self.stream_id.clone(),
            next_seq_no: self.next_seq_no,
        }
    }
}

enum InternalChange {
    UpsertEntity {
        id: EntityId,
        entity_json: Vec<u8>,
    },
    UpsertScope {
        id: ScopeId,
        scope_json: Vec<u8>,
    },
    RemoveEntity {
        id: EntityId,
    },
    RemoveScope {
        id: ScopeId,
    },
    UpsertEntityScopeLink {
        entity_id: EntityId,
        scope_id: ScopeId,
    },
    RemoveEntityScopeLink {
        entity_id: EntityId,
        scope_id: ScopeId,
    },
    UpsertEdge {
        src: EntityId,
        dst: EntityId,
        kind: EdgeKind,
        edge_json: Vec<u8>,
    },
    RemoveEdge {
        src: EntityId,
        dst: EntityId,
        kind: EdgeKind,
    },
    AppendEvent {
        event_json: Vec<u8>,
    },
}

struct InternalStampedChange {
    seq_no: SeqNo,
    change: InternalChange,
}

impl InternalStampedChange {
    fn to_change(&self) -> Option<Change> {
        match &self.change {
            InternalChange::UpsertEntity { entity_json, .. } => {
                let entity = facet_json::from_slice::<Entity>(entity_json).ok()?;
                Some(Change::UpsertEntity(entity))
            }
            InternalChange::UpsertScope { scope_json, .. } => {
                let scope = facet_json::from_slice::<Scope>(scope_json).ok()?;
                Some(Change::UpsertScope(scope))
            }
            InternalChange::RemoveEntity { id } => Some(Change::RemoveEntity {
                id: EntityId::new(id.as_str()),
            }),
            InternalChange::RemoveScope { id } => Some(Change::RemoveScope {
                id: ScopeId::new(id.as_str()),
            }),
            InternalChange::UpsertEntityScopeLink {
                entity_id,
                scope_id,
            } => Some(Change::UpsertEntityScopeLink {
                entity_id: EntityId::new(entity_id.as_str()),
                scope_id: ScopeId::new(scope_id.as_str()),
            }),
            InternalChange::RemoveEntityScopeLink {
                entity_id,
                scope_id,
            } => Some(Change::RemoveEntityScopeLink {
                entity_id: EntityId::new(entity_id.as_str()),
                scope_id: ScopeId::new(scope_id.as_str()),
            }),
            InternalChange::UpsertEdge { edge_json, .. } => {
                let edge = facet_json::from_slice::<Edge>(edge_json).ok()?;
                Some(Change::UpsertEdge(edge))
            }
            InternalChange::RemoveEdge { src, dst, kind } => Some(Change::RemoveEdge {
                src: EntityId::new(src.as_str()),
                dst: EntityId::new(dst.as_str()),
                kind: *kind,
            }),
            InternalChange::AppendEvent { event_json } => {
                let event = facet_json::from_slice::<Event>(event_json).ok()?;
                Some(Change::AppendEvent(event))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityRef {
    id: EntityId,
}

impl EntityRef {
    #[track_caller]
    pub fn id(&self) -> &EntityId {
        &self.id
    }
}

#[track_caller]
pub fn entity_ref_from_wire(id: impl Into<CompactString>) -> EntityRef {
    EntityRef {
        id: EntityId::new(id.into()),
    }
}

fn current_causal_target() -> Option<EntityRef> {
    FUTURE_CAUSAL_STACK
        .try_with(|stack| {
            stack.borrow().last().map(|id| EntityRef {
                id: EntityId::new(id.as_str()),
            })
        })
        .ok()
        .flatten()
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeRef {
    id: ScopeId,
}

impl ScopeRef {
    #[track_caller]
    pub fn id(&self) -> &ScopeId {
        &self.id
    }
}

struct ScopeHandleInner {
    id: ScopeId,
}

impl Drop for ScopeHandleInner {
    fn drop(&mut self) {
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_scope(&self.id);
        }
    }
}

#[derive(Clone)]
pub struct ScopeHandle {
    inner: Arc<ScopeHandleInner>,
}

impl ScopeHandle {
    pub fn new(name: impl Into<CompactString>, body: ScopeBody, source: Source) -> Self {
        let scope = Scope::builder(name, body)
            .source(source.into_compact_string())
            .build(&())
            .expect("scope construction with unit meta should be infallible");
        let id = ScopeId::new(scope.id.as_str());

        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_scope(scope);
        }

        Self {
            inner: Arc::new(ScopeHandleInner { id }),
        }
    }

    #[track_caller]
    pub fn id(&self) -> &ScopeId {
        &self.inner.id
    }

    #[track_caller]
    pub fn scope_ref(&self) -> ScopeRef {
        ScopeRef {
            id: ScopeId::new(self.inner.id.as_str()),
        }
    }
}

struct HandleInner {
    id: EntityId,
}

impl Drop for HandleInner {
    fn drop(&mut self) {
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_entity(&self.id);
        }
    }
}

#[derive(Clone)]
pub struct EntityHandle {
    inner: Arc<HandleInner>,
}

impl EntityHandle {
    pub fn new(name: impl Into<CompactString>, body: EntityBody, source: Source) -> Self {
        Self::new_with_source(name, body, source)
    }

    pub fn new_with_source(
        name: impl Into<CompactString>,
        body: EntityBody,
        source: Source,
    ) -> Self {
        let entity = Entity::builder(name, body)
            .source(source.into_compact_string())
            .build(&())
            .expect("entity construction with unit meta should be infallible");
        Self::from_entity(entity)
    }

    fn from_entity(entity: Entity) -> Self {
        let id = EntityId::new(entity.id.as_str());

        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_entity(entity);
        }

        Self {
            inner: Arc::new(HandleInner { id }),
        }
    }

    #[track_caller]
    pub fn id(&self) -> &EntityId {
        &self.inner.id
    }

    #[track_caller]
    pub fn entity_ref(&self) -> EntityRef {
        EntityRef {
            id: EntityId::new(self.inner.id.as_str()),
        }
    }

    #[track_caller]
    pub fn link_to(&self, target: &EntityRef, kind: EdgeKind) {
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge(self.id(), target.id(), kind);
        }
    }

    #[track_caller]
    pub fn link_to_handle(&self, target: &EntityHandle, kind: EdgeKind) {
        self.link_to(&target.entity_ref(), kind);
    }

    #[track_caller]
    pub fn link_to_scope(&self, scope: &ScopeRef) {
        if let Ok(mut db) = runtime_db().lock() {
            db.link_entity_to_scope(self.id(), scope.id());
        }
    }

    #[track_caller]
    pub fn link_to_scope_handle(&self, scope: &ScopeHandle) {
        self.link_to_scope(&scope.scope_ref());
    }

    #[track_caller]
    pub fn unlink_from_scope(&self, scope: &ScopeRef) {
        if let Ok(mut db) = runtime_db().lock() {
            db.unlink_entity_from_scope(self.id(), scope.id());
        }
    }

    #[track_caller]
    pub fn unlink_from_scope_handle(&self, scope: &ScopeHandle) {
        self.unlink_from_scope(&scope.scope_ref());
    }
}

/// A type that can be used as the `on =` argument of the `peeps!()` macro.
pub trait AsEntityRef {
    fn as_entity_ref(&self) -> EntityRef;
}

impl AsEntityRef for EntityHandle {
    fn as_entity_ref(&self) -> EntityRef {
        self.entity_ref()
    }
}

impl AsEntityRef for EntityRef {
    fn as_entity_ref(&self) -> EntityRef {
        self.clone()
    }
}

impl<T> AsEntityRef for Sender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for Receiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for UnboundedSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for UnboundedReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for OneshotSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T> AsEntityRef for OneshotReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T: Clone> AsEntityRef for BroadcastSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T: Clone> AsEntityRef for BroadcastReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T: Clone> AsEntityRef for WatchSender<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<T: Clone> AsEntityRef for WatchReceiver<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

pub struct Mutex<T> {
    inner: parking_lot::Mutex<T>,
    handle: EntityHandle,
}

pub struct MutexGuard<'a, T> {
    inner: parking_lot::MutexGuard<'a, T>,
    lock_id: EntityId,
    owner_future_id: Option<EntityId>,
}

impl<'a, T> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T> Mutex<T> {
    pub fn new(name: &'static str, value: T, source: Source) -> Self {
        let handle = EntityHandle::new(
            name,
            EntityBody::Lock(LockEntity {
                kind: LockKind::Mutex,
            }),
            source,
        );
        Self {
            inner: parking_lot::Mutex::new(value),
            handle,
        }
    }

    #[track_caller]
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.lock_with_cx(PeepsContext::caller(env!("CARGO_MANIFEST_DIR")))
    }

    pub fn lock_with_cx(&self, _cx: PeepsContext) -> MutexGuard<'_, T> {
        if let Some(inner) = self.inner.try_lock() {
            return self.wrap_guard(inner);
        }

        let pending_edges = self.record_pending_wait_edges();
        let inner = self.inner.lock();
        self.clear_pending_wait_edges(pending_edges);
        self.wrap_guard(inner)
    }

    #[track_caller]
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.try_lock_with_cx(PeepsContext::caller(env!("CARGO_MANIFEST_DIR")))
    }

    pub fn try_lock_with_cx(&self, _cx: PeepsContext) -> Option<MutexGuard<'_, T>> {
        self.inner.try_lock().map(|inner| self.wrap_guard(inner))
    }

    fn wrap_guard<'a>(&self, inner: parking_lot::MutexGuard<'a, T>) -> MutexGuard<'a, T> {
        let lock_id = EntityId::new(self.handle.id().as_str());
        let owner_future_id =
            current_causal_target().map(|target| EntityId::new(target.id().as_str()));
        if let Some(owner_id) = owner_future_id.as_ref() {
            if let Ok(mut db) = runtime_db().lock() {
                db.upsert_edge(owner_id, &lock_id, EdgeKind::Touches);
                db.upsert_edge(&lock_id, owner_id, EdgeKind::Needs);
            }
        }
        HELD_MUTEX_STACK.with(|stack| {
            stack.borrow_mut().push(EntityId::new(lock_id.as_str()));
        });
        MutexGuard {
            inner,
            lock_id,
            owner_future_id,
        }
    }

    fn record_pending_wait_edges(&self) -> Vec<(EntityId, EntityId)> {
        let dst = EntityId::new(self.handle.id().as_str());
        let mut edges = Vec::<(EntityId, EntityId)>::new();

        if let Some(waiter) = current_causal_target() {
            if waiter.id().as_str() != dst.as_str() {
                edges.push((
                    EntityId::new(waiter.id().as_str()),
                    EntityId::new(dst.as_str()),
                ));
            }
        }

        edges.sort_unstable_by(|(lhs_src, lhs_dst), (rhs_src, rhs_dst)| {
            lhs_src
                .as_str()
                .cmp(rhs_src.as_str())
                .then_with(|| lhs_dst.as_str().cmp(rhs_dst.as_str()))
        });
        edges.dedup();

        if let Ok(mut db) = runtime_db().lock() {
            for (src, dst) in &edges {
                db.upsert_edge(src, dst, EdgeKind::Touches);
                db.upsert_edge(src, dst, EdgeKind::Needs);
            }
        }

        edges
    }

    fn clear_pending_wait_edges(&self, edges: Vec<(EntityId, EntityId)>) {
        if let Ok(mut db) = runtime_db().lock() {
            for (src, dst) in edges {
                db.remove_edge(&src, &dst, EdgeKind::Needs);
            }
        }
    }
}

impl<T> AsEntityRef for Mutex<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        if let Some(owner_id) = self.owner_future_id.as_ref() {
            if let Ok(mut db) = runtime_db().lock() {
                db.remove_edge(&self.lock_id, owner_id, EdgeKind::Needs);
            }
        }
        HELD_MUTEX_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(pos) = stack
                .iter()
                .rposition(|id| id.as_str() == self.lock_id.as_str())
            {
                stack.remove(pos);
            }
        });
    }
}

pub struct RwLock<T> {
    inner: parking_lot::RwLock<T>,
    handle: EntityHandle,
}

impl<T> RwLock<T> {
    pub fn new(name: &'static str, value: T, source: Source) -> Self {
        let handle = EntityHandle::new(
            name,
            EntityBody::Lock(LockEntity {
                kind: LockKind::RwLock,
            }),
            source,
        );
        Self {
            inner: parking_lot::RwLock::new(value),
            handle,
        }
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<'_, T> {
        self.inner.read()
    }

    pub fn write(&self) -> parking_lot::RwLockWriteGuard<'_, T> {
        self.inner.write()
    }

    pub fn try_read(&self) -> Option<parking_lot::RwLockReadGuard<'_, T>> {
        self.inner.try_read()
    }

    pub fn try_write(&self) -> Option<parking_lot::RwLockWriteGuard<'_, T>> {
        self.inner.try_write()
    }
}

impl<T> AsEntityRef for RwLock<T> {
    fn as_entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }
}

#[macro_export]
macro_rules! mutex {
    ($name:expr, $value:expr $(,)?) => {{
        $crate::Mutex::new($name, $value, $crate::Source::caller())
    }};
}

#[macro_export]
macro_rules! rwlock {
    ($name:expr, $value:expr $(,)?) => {{
        $crate::RwLock::new($name, $value, $crate::Source::caller())
    }};
}

#[derive(Clone)]
pub struct RpcRequestHandle {
    handle: EntityHandle,
}

impl RpcRequestHandle {
    #[track_caller]
    pub fn id(&self) -> &EntityId {
        self.handle.id()
    }

    #[track_caller]
    pub fn id_for_wire(&self) -> CompactString {
        CompactString::from(self.handle.id().as_str())
    }

    #[track_caller]
    pub fn entity_ref(&self) -> EntityRef {
        self.handle.entity_ref()
    }

    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }
}

#[derive(Clone)]
pub struct RpcResponseHandle {
    handle: EntityHandle,
}

impl RpcResponseHandle {
    #[track_caller]
    pub fn id(&self) -> &EntityId {
        self.handle.id()
    }

    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn set_status(&self, status: ResponseStatus) {
        let mut changed = false;
        if let Ok(mut db) = runtime_db().lock() {
            changed = db.update_response_status(self.handle.id(), status);
        }
        if !changed {
            return;
        }
        if let Ok(event) = Event::new(
            EventTarget::Entity(self.handle.id().clone()),
            EventKind::StateChanged,
            &status,
        ) {
            if let Ok(mut db) = runtime_db().lock() {
                db.record_event(event);
            }
        }
    }

    #[track_caller]
    pub fn mark_ok(&self) {
        self.set_status(ResponseStatus::Ok);
    }

    #[track_caller]
    pub fn mark_error(&self) {
        self.set_status(ResponseStatus::Error);
    }

    #[track_caller]
    pub fn mark_cancelled(&self) {
        self.set_status(ResponseStatus::Cancelled);
    }
}
#[track_caller]
pub fn rpc_request(
    method: impl Into<CompactString>,
    args_preview: impl Into<CompactString>,
    source: Source,
) -> RpcRequestHandle {
    let method = method.into();
    let body = EntityBody::Request(RequestEntity {
        method: method.clone(),
        args_preview: args_preview.into(),
    });
    RpcRequestHandle {
        handle: EntityHandle::new(method, body, source),
    }
}

#[macro_export]
macro_rules! rpc_request {
    ($method:expr, $args_preview:expr $(,)?) => {
        $crate::rpc_request($method, $args_preview, $crate::Source::caller())
    };
}
pub fn rpc_response(method: impl Into<CompactString>, source: Source) -> RpcResponseHandle {
    let method = method.into();
    let body = EntityBody::Response(ResponseEntity {
        method: method.clone(),
        status: ResponseStatus::Pending,
    });
    RpcResponseHandle {
        handle: EntityHandle::new(format!("{method}"), body, source),
    }
}

#[macro_export]
macro_rules! rpc_response {
    ($method:expr $(,)?) => {
        $crate::rpc_response($method, $crate::Source::caller())
    };
}
pub fn rpc_response_for(
    method: impl Into<CompactString>,
    request: &EntityRef,
    source: Source,
) -> RpcResponseHandle {
    #[allow(deprecated)]
    let response = rpc_response(method, source);
    if let Ok(mut db) = runtime_db().lock() {
        db.upsert_edge(request.id(), response.id(), EdgeKind::RpcLink);
    }
    response
}

#[macro_export]
macro_rules! rpc_response_for {
    ($method:expr, $request:expr $(,)?) => {
        $crate::rpc_response_for($method, $request, $crate::Source::caller())
    };
}

pub struct Sender<T> {
    inner: mpsc::Sender<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct UnboundedSender<T> {
    inner: mpsc::UnboundedSender<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct UnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct OneshotSender<T> {
    inner: Option<oneshot::Sender<T>>,
    handle: EntityHandle,
    channel: Arc<StdMutex<OneshotRuntimeState>>,
}

pub struct OneshotReceiver<T> {
    inner: Option<oneshot::Receiver<T>>,
    handle: EntityHandle,
    channel: Arc<StdMutex<OneshotRuntimeState>>,
    name: CompactString,
}

pub struct BroadcastSender<T> {
    inner: broadcast::Sender<T>,
    handle: EntityHandle,
    receiver_handle: EntityHandle,
    channel: Arc<StdMutex<BroadcastRuntimeState>>,
    name: CompactString,
}

pub struct BroadcastReceiver<T> {
    inner: broadcast::Receiver<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<BroadcastRuntimeState>>,
    name: CompactString,
}

pub struct WatchSender<T> {
    inner: watch::Sender<T>,
    handle: EntityHandle,
    receiver_handle: EntityHandle,
    channel: Arc<StdMutex<WatchRuntimeState>>,
    name: CompactString,
}

pub struct WatchReceiver<T> {
    inner: watch::Receiver<T>,
    handle: EntityHandle,
    channel: Arc<StdMutex<WatchRuntimeState>>,
    name: CompactString,
}

#[derive(Clone)]
pub struct Notify {
    inner: Arc<tokio::sync::Notify>,
    handle: EntityHandle,
    waiter_count: Arc<AtomicU32>,
}

pub struct DiagnosticInterval {
    inner: tokio::time::Interval,
    handle: EntityHandle,
}

pub type Interval = DiagnosticInterval;

pub struct OnceCell<T> {
    inner: tokio::sync::OnceCell<T>,
    handle: EntityHandle,
    waiter_count: AtomicU32,
}

#[derive(Clone)]
pub struct Semaphore {
    inner: Arc<tokio::sync::Semaphore>,
    handle: EntityHandle,
    max_permits: Arc<AtomicU32>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityId, u32>>>,
}

pub struct SemaphorePermit<'a> {
    inner: Option<tokio::sync::SemaphorePermit<'a>>,
    semaphore_id: EntityId,
    holder_future_id: Option<EntityId>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityId, u32>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    max_permits: Arc<AtomicU32>,
}

pub struct OwnedSemaphorePermit {
    inner: Option<tokio::sync::OwnedSemaphorePermit>,
    semaphore_id: EntityId,
    holder_future_id: Option<EntityId>,
    holder_counts: Arc<StdMutex<BTreeMap<EntityId, u32>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    max_permits: Arc<AtomicU32>,
}

pub struct Command {
    inner: tokio::process::Command,
    program: CompactString,
    args: Vec<CompactString>,
    env: Vec<CompactString>,
}

#[derive(Clone, Debug)]
pub struct CommandDiagnostics {
    pub program: CompactString,
    pub args: Vec<CompactString>,
    pub env: Vec<CompactString>,
}

pub struct Child {
    inner: Option<tokio::process::Child>,
    handle: EntityHandle,
}

pub struct JoinSet<T> {
    inner: tokio::task::JoinSet<T>,
    handle: EntityHandle,
}

struct ChannelRuntimeState {
    tx_id: EntityId,
    rx_id: EntityId,
    tx_ref_count: u32,
    rx_state: ReceiverState,
    queue_len: u32,
    capacity: Option<u32>,
    tx_close_cause: Option<ChannelCloseCause>,
    rx_close_cause: Option<ChannelCloseCause>,
}

struct OneshotRuntimeState {
    tx_id: EntityId,
    rx_id: EntityId,
    tx_lifecycle: ChannelEndpointLifecycle,
    rx_lifecycle: ChannelEndpointLifecycle,
    state: OneshotState,
}

struct BroadcastRuntimeState {
    tx_id: EntityId,
    rx_id: EntityId,
    tx_ref_count: u32,
    rx_ref_count: u32,
    capacity: u32,
    tx_close_cause: Option<ChannelCloseCause>,
    rx_close_cause: Option<ChannelCloseCause>,
}

struct WatchRuntimeState {
    tx_id: EntityId,
    rx_id: EntityId,
    tx_ref_count: u32,
    rx_ref_count: u32,
    tx_close_cause: Option<ChannelCloseCause>,
    rx_close_cause: Option<ChannelCloseCause>,
    last_update_at: Option<peeps_types::PTime>,
}

enum ReceiverState {
    Alive,
    Dropped,
}

impl ChannelRuntimeState {
    fn tx_lifecycle(&self) -> ChannelEndpointLifecycle {
        match self.tx_close_cause {
            Some(cause) => ChannelEndpointLifecycle::Closed(cause),
            None => ChannelEndpointLifecycle::Open,
        }
    }

    fn rx_lifecycle(&self) -> ChannelEndpointLifecycle {
        match self.rx_close_cause {
            Some(cause) => ChannelEndpointLifecycle::Closed(cause),
            None => ChannelEndpointLifecycle::Open,
        }
    }

    fn is_send_full(&self) -> bool {
        self.capacity
            .map(|capacity| self.queue_len >= capacity)
            .unwrap_or(false)
    }

    fn is_receive_empty(&self) -> bool {
        self.queue_len == 0
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T> Clone for BroadcastSender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            receiver_handle: self.receiver_handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T: Clone> Clone for BroadcastReceiver<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.resubscribe(),
            handle: self.handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T> Clone for WatchSender<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            receiver_handle: self.receiver_handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T> Clone for WatchReceiver<T> {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_add(1);
        }
        Self {
            inner: self.inner.clone(),
            handle: self.handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

fn sync_channel_state(
    channel: &Arc<StdMutex<ChannelRuntimeState>>,
) -> Option<(
    EntityId,
    EntityId,
    Option<BufferState>,
    ChannelEndpointLifecycle,
    ChannelEndpointLifecycle,
)> {
    let state = channel.lock().ok()?;
    Some((
        EntityId::new(state.tx_id.as_str()),
        EntityId::new(state.rx_id.as_str()),
        Some(BufferState {
            occupancy: state.queue_len,
            capacity: state.capacity,
        }),
        state.tx_lifecycle(),
        state.rx_lifecycle(),
    ))
}

fn apply_channel_state(channel: &Arc<StdMutex<ChannelRuntimeState>>) {
    let Some((tx_id, rx_id, buffer, tx_lifecycle, rx_lifecycle)) = sync_channel_state(channel)
    else {
        return;
    };
    if let Ok(mut db) = runtime_db().lock() {
        db.update_channel_endpoint_state(&tx_id, tx_lifecycle, buffer);
        db.update_channel_endpoint_state(&rx_id, rx_lifecycle, buffer);
    }
}

fn emit_channel_wait_started(target: &EntityId, kind: ChannelWaitKind) {
    if let Ok(event) = Event::channel_wait_started(
        EventTarget::Entity(target.clone()),
        &ChannelWaitStartedEvent { kind },
    ) {
        if let Ok(mut db) = runtime_db().lock() {
            db.record_event(event);
        }
    }
}

fn emit_channel_wait_ended(target: &EntityId, kind: ChannelWaitKind, started: Instant) {
    let wait_ns = started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
    if let Ok(event) = Event::channel_wait_ended(
        EventTarget::Entity(target.clone()),
        &ChannelWaitEndedEvent { kind, wait_ns },
    ) {
        if let Ok(mut db) = runtime_db().lock() {
            db.record_event(event);
        }
    }
}

fn emit_channel_closed(target: &EntityId, cause: ChannelCloseCause) {
    if let Ok(event) = Event::channel_closed(
        EventTarget::Entity(target.clone()),
        &ChannelClosedEvent { cause },
    ) {
        if let Ok(mut db) = runtime_db().lock() {
            db.record_event(event);
        }
    }
}

fn sync_oneshot_state(
    channel: &Arc<StdMutex<OneshotRuntimeState>>,
) -> Option<(
    EntityId,
    EntityId,
    OneshotState,
    ChannelEndpointLifecycle,
    ChannelEndpointLifecycle,
)> {
    let state = channel.lock().ok()?;
    Some((
        EntityId::new(state.tx_id.as_str()),
        EntityId::new(state.rx_id.as_str()),
        state.state,
        state.tx_lifecycle,
        state.rx_lifecycle,
    ))
}

fn apply_oneshot_state(channel: &Arc<StdMutex<OneshotRuntimeState>>) {
    let Some((tx_id, rx_id, state, tx_lifecycle, rx_lifecycle)) = sync_oneshot_state(channel)
    else {
        return;
    };
    if let Ok(mut db) = runtime_db().lock() {
        db.update_oneshot_endpoint_state(&tx_id, tx_lifecycle, state);
        db.update_oneshot_endpoint_state(&rx_id, rx_lifecycle, state);
    }
}

fn sync_broadcast_state(
    channel: &Arc<StdMutex<BroadcastRuntimeState>>,
) -> Option<(
    EntityId,
    EntityId,
    Option<BufferState>,
    ChannelEndpointLifecycle,
    ChannelEndpointLifecycle,
)> {
    let state = channel.lock().ok()?;
    let tx_lifecycle = match state.tx_close_cause {
        Some(cause) => ChannelEndpointLifecycle::Closed(cause),
        None => ChannelEndpointLifecycle::Open,
    };
    let rx_lifecycle = match state.rx_close_cause {
        Some(cause) => ChannelEndpointLifecycle::Closed(cause),
        None => ChannelEndpointLifecycle::Open,
    };
    Some((
        EntityId::new(state.tx_id.as_str()),
        EntityId::new(state.rx_id.as_str()),
        Some(BufferState {
            occupancy: 0,
            capacity: Some(state.capacity),
        }),
        tx_lifecycle,
        rx_lifecycle,
    ))
}

fn apply_broadcast_state(channel: &Arc<StdMutex<BroadcastRuntimeState>>) {
    let Some((tx_id, rx_id, buffer, tx_lifecycle, rx_lifecycle)) = sync_broadcast_state(channel)
    else {
        return;
    };
    if let Ok(mut db) = runtime_db().lock() {
        db.update_channel_endpoint_state(&tx_id, tx_lifecycle, buffer);
        db.update_channel_endpoint_state(&rx_id, rx_lifecycle, buffer);
    }
}

fn sync_watch_state(
    channel: &Arc<StdMutex<WatchRuntimeState>>,
) -> Option<(
    EntityId,
    EntityId,
    ChannelEndpointLifecycle,
    ChannelEndpointLifecycle,
    Option<peeps_types::PTime>,
)> {
    let state = channel.lock().ok()?;
    let tx_lifecycle = match state.tx_close_cause {
        Some(cause) => ChannelEndpointLifecycle::Closed(cause),
        None => ChannelEndpointLifecycle::Open,
    };
    let rx_lifecycle = match state.rx_close_cause {
        Some(cause) => ChannelEndpointLifecycle::Closed(cause),
        None => ChannelEndpointLifecycle::Open,
    };
    Some((
        EntityId::new(state.tx_id.as_str()),
        EntityId::new(state.rx_id.as_str()),
        tx_lifecycle,
        rx_lifecycle,
        state.last_update_at,
    ))
}

fn apply_watch_state(channel: &Arc<StdMutex<WatchRuntimeState>>) {
    let Some((tx_id, rx_id, tx_lifecycle, rx_lifecycle, last_update_at)) =
        sync_watch_state(channel)
    else {
        return;
    };
    if let Ok(mut db) = runtime_db().lock() {
        db.update_channel_endpoint_state(&tx_id, tx_lifecycle, None);
        db.update_channel_endpoint_state(&rx_id, rx_lifecycle, None);
        db.update_watch_last_update(&tx_id, last_update_at);
        db.update_watch_last_update(&rx_id, last_update_at);
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut emit_for_rx = None;
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_sub(1);
            if state.tx_ref_count == 0 {
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                    emit_for_rx = Some(EntityId::new(state.rx_id.as_str()));
                }
            }
        }
        apply_channel_state(&self.channel);
        if let Some(rx_id) = emit_for_rx {
            emit_channel_closed(&rx_id, ChannelCloseCause::SenderDropped);
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut emit_for_tx = None;
        if let Ok(mut state) = self.channel.lock() {
            if matches!(state.rx_state, ReceiverState::Alive) {
                state.rx_state = ReceiverState::Dropped;
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    emit_for_tx = Some(EntityId::new(state.tx_id.as_str()));
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                }
            }
        }
        apply_channel_state(&self.channel);
        if let Some(tx_id) = emit_for_tx {
            emit_channel_closed(&tx_id, ChannelCloseCause::ReceiverDropped);
        }
    }
}

impl<T> Drop for UnboundedSender<T> {
    fn drop(&mut self) {
        let mut emit_for_rx = None;
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_sub(1);
            if state.tx_ref_count == 0 {
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                    emit_for_rx = Some(EntityId::new(state.rx_id.as_str()));
                }
            }
        }
        apply_channel_state(&self.channel);
        if let Some(rx_id) = emit_for_rx {
            emit_channel_closed(&rx_id, ChannelCloseCause::SenderDropped);
        }
    }
}

impl<T> Drop for UnboundedReceiver<T> {
    fn drop(&mut self) {
        let mut emit_for_tx = None;
        if let Ok(mut state) = self.channel.lock() {
            if matches!(state.rx_state, ReceiverState::Alive) {
                state.rx_state = ReceiverState::Dropped;
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    emit_for_tx = Some(EntityId::new(state.tx_id.as_str()));
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                }
            }
        }
        apply_channel_state(&self.channel);
        if let Some(tx_id) = emit_for_tx {
            emit_channel_closed(&tx_id, ChannelCloseCause::ReceiverDropped);
        }
    }
}

impl<T> Sender<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn try_send(&self, value: T) -> Result<(), mpsc::error::TrySendError<T>> {
        self.inner.try_send(value)
    }

    #[track_caller]
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn send(
        &self,
        value: T,
    ) -> impl Future<Output = Result<(), mpsc::error::SendError<T>>> + '_ {
        let source = Source::caller();
        async move {
            let wait_kind = self.channel.lock().ok().and_then(|state| {
                if state.is_send_full() {
                    if let Ok(event) = Event::channel_sent(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelSendEvent {
                            outcome: ChannelSendOutcome::Full,
                            queue_len: Some(state.queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Some(ChannelWaitKind::SendFull)
                } else {
                    None
                }
            });
            let wait_started = wait_kind.map(|kind| {
                emit_channel_wait_started(self.handle.id(), kind);
                Instant::now()
            });

            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Send,
                self.inner.send(value),
                source,
            )
            .await;

            if let (Some(kind), Some(started)) = (wait_kind, wait_started) {
                emit_channel_wait_ended(self.handle.id(), kind, started);
            }

            match result {
                Ok(()) => {
                    let queue_len = if let Ok(mut state) = self.channel.lock() {
                        state.queue_len = state.queue_len.saturating_add(1);
                        state.queue_len
                    } else {
                        0
                    };
                    apply_channel_state(&self.channel);
                    if let Ok(event) = Event::channel_sent(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelSendEvent {
                            outcome: ChannelSendOutcome::Ok,
                            queue_len: Some(queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Ok(())
                }
                Err(err) => {
                    let (queue_len, close_cause) = if let Ok(mut state) = self.channel.lock() {
                        if state.tx_close_cause.is_none() {
                            state.tx_close_cause = Some(ChannelCloseCause::ReceiverClosed);
                        }
                        if state.rx_close_cause.is_none() {
                            state.rx_close_cause = Some(ChannelCloseCause::ReceiverClosed);
                        }
                        (
                            state.queue_len,
                            state
                                .tx_close_cause
                                .unwrap_or(ChannelCloseCause::ReceiverClosed),
                        )
                    } else {
                        (0, ChannelCloseCause::ReceiverClosed)
                    };
                    apply_channel_state(&self.channel);
                    if let Ok(event) = Event::channel_sent(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelSendEvent {
                            outcome: ChannelSendOutcome::Closed,
                            queue_len: Some(queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    if let Ok(event) = Event::channel_closed(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelClosedEvent { cause: close_cause },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Err(err)
                }
            }
        }
    }
}

impl<T> Receiver<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn recv(&mut self) -> impl Future<Output = Option<T>> + '_ {
        let source = Source::caller();
        async move {
            let wait_kind = self.channel.lock().ok().and_then(|state| {
                if state.is_receive_empty() {
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Empty,
                            queue_len: Some(state.queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Some(ChannelWaitKind::ReceiveEmpty)
                } else {
                    None
                }
            });
            let wait_started = wait_kind.map(|kind| {
                emit_channel_wait_started(self.handle.id(), kind);
                Instant::now()
            });

            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Recv,
                self.inner.recv(),
                source,
            )
            .await;

            if let (Some(kind), Some(started)) = (wait_kind, wait_started) {
                emit_channel_wait_ended(self.handle.id(), kind, started);
            }

            match result {
                Some(value) => {
                    let queue_len = if let Ok(mut state) = self.channel.lock() {
                        state.queue_len = state.queue_len.saturating_sub(1);
                        state.queue_len
                    } else {
                        0
                    };
                    apply_channel_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Ok,
                            queue_len: Some(queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Some(value)
                }
                None => {
                    let (queue_len, close_cause) = if let Ok(mut state) = self.channel.lock() {
                        if state.tx_close_cause.is_none() {
                            state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                        }
                        if state.rx_close_cause.is_none() {
                            state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                        }
                        (
                            state.queue_len,
                            state
                                .rx_close_cause
                                .unwrap_or(ChannelCloseCause::SenderDropped),
                        )
                    } else {
                        (0, ChannelCloseCause::SenderDropped)
                    };
                    apply_channel_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Closed,
                            queue_len: Some(queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    if let Ok(event) = Event::channel_closed(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelClosedEvent { cause: close_cause },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    None
                }
            }
        }
    }
}

impl<T> UnboundedSender<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
        match self.inner.send(value) {
            Ok(()) => {
                let queue_len = if let Ok(mut state) = self.channel.lock() {
                    state.queue_len = state.queue_len.saturating_add(1);
                    state.queue_len
                } else {
                    0
                };
                apply_channel_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Ok,
                        queue_len: Some(queue_len),
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                Ok(())
            }
            Err(err) => {
                let (queue_len, close_cause) = if let Ok(mut state) = self.channel.lock() {
                    if state.tx_close_cause.is_none() {
                        state.tx_close_cause = Some(ChannelCloseCause::ReceiverClosed);
                    }
                    if state.rx_close_cause.is_none() {
                        state.rx_close_cause = Some(ChannelCloseCause::ReceiverClosed);
                    }
                    (
                        state.queue_len,
                        state
                            .tx_close_cause
                            .unwrap_or(ChannelCloseCause::ReceiverClosed),
                    )
                } else {
                    (0, ChannelCloseCause::ReceiverClosed)
                };
                apply_channel_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Closed,
                        queue_len: Some(queue_len),
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                if let Ok(event) = Event::channel_closed(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelClosedEvent { cause: close_cause },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                Err(err)
            }
        }
    }

    #[track_caller]
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

impl<T> UnboundedReceiver<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn recv(&mut self) -> impl Future<Output = Option<T>> + '_ {
        let source = Source::caller();
        async move {
            let wait_kind = self.channel.lock().ok().and_then(|state| {
                if state.is_receive_empty() {
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Empty,
                            queue_len: Some(state.queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Some(ChannelWaitKind::ReceiveEmpty)
                } else {
                    None
                }
            });
            let wait_started = wait_kind.map(|kind| {
                emit_channel_wait_started(self.handle.id(), kind);
                Instant::now()
            });

            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Recv,
                self.inner.recv(),
                source,
            )
            .await;

            if let (Some(kind), Some(started)) = (wait_kind, wait_started) {
                emit_channel_wait_ended(self.handle.id(), kind, started);
            }

            match result {
                Some(value) => {
                    let queue_len = if let Ok(mut state) = self.channel.lock() {
                        state.queue_len = state.queue_len.saturating_sub(1);
                        state.queue_len
                    } else {
                        0
                    };
                    apply_channel_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Ok,
                            queue_len: Some(queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Some(value)
                }
                None => {
                    let (queue_len, close_cause) = if let Ok(mut state) = self.channel.lock() {
                        if state.tx_close_cause.is_none() {
                            state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                        }
                        if state.rx_close_cause.is_none() {
                            state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                        }
                        (
                            state.queue_len,
                            state
                                .rx_close_cause
                                .unwrap_or(ChannelCloseCause::SenderDropped),
                        )
                    } else {
                        (0, ChannelCloseCause::SenderDropped)
                    };
                    apply_channel_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Closed,
                            queue_len: Some(queue_len),
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    if let Ok(event) = Event::channel_closed(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelClosedEvent { cause: close_cause },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    None
                }
            }
        }
    }
}
pub fn channel<T>(name: impl Into<String>, capacity: usize, source: Source) -> (Sender<T>, Receiver<T>) {
    let name: CompactString = name.into().into();
    let (tx, rx) = mpsc::channel(capacity);
    let capacity_u32 = capacity.min(u32::MAX as usize) as u32;

    let details = ChannelDetails::Mpsc(MpscChannelDetails {
        buffer: Some(BufferState {
            occupancy: 0,
            capacity: Some(capacity_u32),
        }),
    });
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    let details = ChannelDetails::Mpsc(MpscChannelDetails {
        buffer: Some(BufferState {
            occupancy: 0,
            capacity: Some(capacity_u32),
        }),
    });
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    let channel = Arc::new(StdMutex::new(ChannelRuntimeState {
        tx_id: tx_handle.id().clone(),
        rx_id: rx_handle.id().clone(),
        tx_ref_count: 1,
        rx_state: ReceiverState::Alive,
        queue_len: 0,
        capacity: Some(capacity_u32),
        tx_close_cause: None,
        rx_close_cause: None,
    }));

    (
        Sender {
            inner: tx,
            handle: tx_handle,
            channel: channel.clone(),
            name: name.clone(),
        },
        Receiver {
            inner: rx,
            handle: rx_handle,
            channel,
            name,
        },
    )
}

#[macro_export]
macro_rules! channel {
    ($name:expr, $capacity:expr $(,)?) => {
        $crate::channel($name, $capacity, $crate::Source::caller())
    };
}
pub fn unbounded_channel<T>(
    name: impl Into<String>,
    source: Source,
) -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let name: CompactString = name.into().into();
    let (tx, rx) = mpsc::unbounded_channel();
    let details = ChannelDetails::Mpsc(MpscChannelDetails {
        buffer: Some(BufferState {
            occupancy: 0,
            capacity: None,
        }),
    });
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    let details = ChannelDetails::Mpsc(MpscChannelDetails {
        buffer: Some(BufferState {
            occupancy: 0,
            capacity: None,
        }),
    });
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    let channel = Arc::new(StdMutex::new(ChannelRuntimeState {
        tx_id: tx_handle.id().clone(),
        rx_id: rx_handle.id().clone(),
        tx_ref_count: 1,
        rx_state: ReceiverState::Alive,
        queue_len: 0,
        capacity: None,
        tx_close_cause: None,
        rx_close_cause: None,
    }));
    (
        UnboundedSender {
            inner: tx,
            handle: tx_handle,
            channel: channel.clone(),
            name: name.clone(),
        },
        UnboundedReceiver {
            inner: rx,
            handle: rx_handle,
            channel,
            name,
        },
    )
}

#[macro_export]
macro_rules! unbounded_channel {
    ($name:expr $(,)?) => {
        $crate::unbounded_channel($name, $crate::Source::caller())
    };
}

impl<T> Drop for OneshotSender<T> {
    fn drop(&mut self) {
        if self.inner.is_none() {
            return;
        }
        let mut emit_for_rx = None;
        if let Ok(mut state) = self.channel.lock() {
            if matches!(state.state, OneshotState::Pending) {
                state.state = OneshotState::SenderDropped;
                state.tx_lifecycle =
                    ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                state.rx_lifecycle =
                    ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                emit_for_rx = Some(EntityId::new(state.rx_id.as_str()));
            }
        }
        apply_oneshot_state(&self.channel);
        if let Some(rx_id) = emit_for_rx {
            emit_channel_closed(&rx_id, ChannelCloseCause::SenderDropped);
        }
    }
}

impl<T> Drop for OneshotReceiver<T> {
    fn drop(&mut self) {
        if self.inner.is_none() {
            return;
        }
        let mut emit_for_tx = None;
        if let Ok(mut state) = self.channel.lock() {
            if matches!(state.state, OneshotState::Pending | OneshotState::Sent) {
                state.state = OneshotState::ReceiverDropped;
                state.tx_lifecycle =
                    ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                state.rx_lifecycle =
                    ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                emit_for_tx = Some(EntityId::new(state.tx_id.as_str()));
            }
        }
        apply_oneshot_state(&self.channel);
        if let Some(tx_id) = emit_for_tx {
            emit_channel_closed(&tx_id, ChannelCloseCause::ReceiverDropped);
        }
    }
}

impl<T> Drop for BroadcastSender<T> {
    fn drop(&mut self) {
        let mut emit_for_rx = None;
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_sub(1);
            if state.tx_ref_count == 0 {
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                    emit_for_rx = Some(EntityId::new(state.rx_id.as_str()));
                }
            }
        }
        apply_broadcast_state(&self.channel);
        if let Some(rx_id) = emit_for_rx {
            emit_channel_closed(&rx_id, ChannelCloseCause::SenderDropped);
        }
    }
}

impl<T> Drop for BroadcastReceiver<T> {
    fn drop(&mut self) {
        let mut emit_for_tx = None;
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_sub(1);
            if state.rx_ref_count == 0 {
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    emit_for_tx = Some(EntityId::new(state.tx_id.as_str()));
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                }
            }
        }
        apply_broadcast_state(&self.channel);
        if let Some(tx_id) = emit_for_tx {
            emit_channel_closed(&tx_id, ChannelCloseCause::ReceiverDropped);
        }
    }
}

impl<T> Drop for WatchSender<T> {
    fn drop(&mut self) {
        let mut emit_for_rx = None;
        if let Ok(mut state) = self.channel.lock() {
            state.tx_ref_count = state.tx_ref_count.saturating_sub(1);
            if state.tx_ref_count == 0 {
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                    emit_for_rx = Some(EntityId::new(state.rx_id.as_str()));
                }
            }
        }
        apply_watch_state(&self.channel);
        if let Some(rx_id) = emit_for_rx {
            emit_channel_closed(&rx_id, ChannelCloseCause::SenderDropped);
        }
    }
}

impl<T> Drop for WatchReceiver<T> {
    fn drop(&mut self) {
        let mut emit_for_tx = None;
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_sub(1);
            if state.rx_ref_count == 0 {
                if state.tx_close_cause.is_none() {
                    state.tx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    emit_for_tx = Some(EntityId::new(state.tx_id.as_str()));
                }
                if state.rx_close_cause.is_none() {
                    state.rx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                }
            }
        }
        apply_watch_state(&self.channel);
        if let Some(tx_id) = emit_for_tx {
            emit_channel_closed(&tx_id, ChannelCloseCause::ReceiverDropped);
        }
    }
}

impl<T> OneshotSender<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn send(mut self, value: T) -> Result<(), T> {
        let Some(inner) = self.inner.take() else {
            return Err(value);
        };
        match inner.send(value) {
            Ok(()) => {
                if let Ok(mut state) = self.channel.lock() {
                    state.state = OneshotState::Sent;
                    state.tx_lifecycle =
                        ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                }
                apply_oneshot_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Ok,
                        queue_len: None,
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                Ok(())
            }
            Err(value) => {
                if let Ok(mut state) = self.channel.lock() {
                    state.state = OneshotState::ReceiverDropped;
                    state.tx_lifecycle =
                        ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                    state.rx_lifecycle =
                        ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                }
                apply_oneshot_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Closed,
                        queue_len: None,
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                if let Ok(event) = Event::channel_closed(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelClosedEvent {
                        cause: ChannelCloseCause::ReceiverDropped,
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                Err(value)
            }
        }
    }
}

impl<T> OneshotReceiver<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn recv(mut self) -> impl Future<Output = Result<T, oneshot::error::RecvError>> {
        let source = Source::caller();
        async move {
            let inner = self.inner.take().expect("oneshot receiver consumed");
            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Recv,
                inner,
                source,
            )
            .await;
            match result {
                Ok(value) => {
                    if let Ok(mut state) = self.channel.lock() {
                        state.state = OneshotState::Received;
                        state.rx_lifecycle =
                            ChannelEndpointLifecycle::Closed(ChannelCloseCause::ReceiverDropped);
                    }
                    apply_oneshot_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Ok,
                            queue_len: None,
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Ok(value)
                }
                Err(err) => {
                    if let Ok(mut state) = self.channel.lock() {
                        state.state = OneshotState::SenderDropped;
                        state.tx_lifecycle =
                            ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                        state.rx_lifecycle =
                            ChannelEndpointLifecycle::Closed(ChannelCloseCause::SenderDropped);
                    }
                    apply_oneshot_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Closed,
                            queue_len: None,
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    if let Ok(event) = Event::channel_closed(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelClosedEvent {
                            cause: ChannelCloseCause::SenderDropped,
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Err(err)
                }
            }
        }
    }
}
pub fn oneshot<T>(name: impl Into<String>, source: Source) -> (OneshotSender<T>, OneshotReceiver<T>) {
    let name: CompactString = name.into().into();
    let (tx, rx) = oneshot::channel();
    let details = ChannelDetails::Oneshot(OneshotChannelDetails {
        state: OneshotState::Pending,
    });
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    let details = ChannelDetails::Oneshot(OneshotChannelDetails {
        state: OneshotState::Pending,
    });
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    let channel = Arc::new(StdMutex::new(OneshotRuntimeState {
        tx_id: tx_handle.id().clone(),
        rx_id: rx_handle.id().clone(),
        tx_lifecycle: ChannelEndpointLifecycle::Open,
        rx_lifecycle: ChannelEndpointLifecycle::Open,
        state: OneshotState::Pending,
    }));
    (
        OneshotSender {
            inner: Some(tx),
            handle: tx_handle,
            channel: channel.clone(),
        },
        OneshotReceiver {
            inner: Some(rx),
            handle: rx_handle,
            channel,
            name,
        },
    )
}
pub fn oneshot_channel<T>(
    name: impl Into<String>,
    source: Source,
) -> (OneshotSender<T>, OneshotReceiver<T>) {
    #[allow(deprecated)]
    oneshot(name, source)
}

#[macro_export]
macro_rules! oneshot {
    ($name:expr $(,)?) => {
        $crate::oneshot($name, $crate::Source::caller())
    };
}

impl<T: Clone> BroadcastSender<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_add(1);
        }
        BroadcastReceiver {
            inner: self.inner.subscribe(),
            handle: self.receiver_handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }

    #[track_caller]
    pub fn send(&self, value: T) -> Result<usize, broadcast::error::SendError<T>> {
        match self.inner.send(value) {
            Ok(receivers) => {
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Ok,
                        queue_len: None,
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                Ok(receivers)
            }
            Err(err) => {
                if let Ok(mut state) = self.channel.lock() {
                    if state.tx_close_cause.is_none() {
                        state.tx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    }
                    if state.rx_close_cause.is_none() {
                        state.rx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    }
                }
                apply_broadcast_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Closed,
                        queue_len: None,
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                if let Ok(event) = Event::channel_closed(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelClosedEvent {
                        cause: ChannelCloseCause::ReceiverDropped,
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                Err(err)
            }
        }
    }
}

impl<T: Clone> BroadcastReceiver<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn recv(&mut self) -> impl Future<Output = Result<T, broadcast::error::RecvError>> + '_ {
        let source = Source::caller();
        async move {
            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Recv,
                self.inner.recv(),
                source,
            )
            .await;
            match result {
                Ok(value) => {
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Ok,
                            queue_len: None,
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Ok(value)
                }
                Err(err) => {
                    if let broadcast::error::RecvError::Closed = err {
                        if let Ok(mut state) = self.channel.lock() {
                            if state.tx_close_cause.is_none() {
                                state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                            }
                            if state.rx_close_cause.is_none() {
                                state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                            }
                        }
                        apply_broadcast_state(&self.channel);
                        if let Ok(event) = Event::channel_closed(
                            EventTarget::Entity(self.handle.id().clone()),
                            &ChannelClosedEvent {
                                cause: ChannelCloseCause::SenderDropped,
                            },
                        ) {
                            if let Ok(mut db) = runtime_db().lock() {
                                db.record_event(event);
                            }
                        }
                    }
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Empty,
                            queue_len: None,
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Err(err)
                }
            }
        }
    }
}

impl<T: Clone> WatchSender<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    pub fn send(&self, value: T) -> Result<(), watch::error::SendError<T>> {
        match self.inner.send(value) {
            Ok(()) => {
                let now = peeps_types::PTime::now();
                if let Ok(mut state) = self.channel.lock() {
                    state.last_update_at = Some(now);
                }
                apply_watch_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Ok,
                        queue_len: None,
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                Ok(())
            }
            Err(err) => {
                if let Ok(mut state) = self.channel.lock() {
                    if state.tx_close_cause.is_none() {
                        state.tx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    }
                    if state.rx_close_cause.is_none() {
                        state.rx_close_cause = Some(ChannelCloseCause::ReceiverDropped);
                    }
                }
                apply_watch_state(&self.channel);
                if let Ok(event) = Event::channel_sent(
                    EventTarget::Entity(self.handle.id().clone()),
                    &ChannelSendEvent {
                        outcome: ChannelSendOutcome::Closed,
                        queue_len: None,
                    },
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }
                Err(err)
            }
        }
    }

    #[track_caller]
    pub fn send_replace(&self, value: T) -> T {
        let old = self.inner.send_replace(value);
        let now = peeps_types::PTime::now();
        if let Ok(mut state) = self.channel.lock() {
            state.last_update_at = Some(now);
        }
        apply_watch_state(&self.channel);
        old
    }

    #[track_caller]
    pub fn subscribe(&self) -> WatchReceiver<T> {
        if let Ok(mut state) = self.channel.lock() {
            state.rx_ref_count = state.rx_ref_count.saturating_add(1);
        }
        WatchReceiver {
            inner: self.inner.subscribe(),
            handle: self.receiver_handle.clone(),
            channel: self.channel.clone(),
            name: self.name.clone(),
        }
    }
}

impl<T: Clone> WatchReceiver<T> {
    #[doc(hidden)]
    #[track_caller]
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn changed(&mut self) -> impl Future<Output = Result<(), watch::error::RecvError>> + '_ {
        let source = Source::caller();
        async move {
            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Recv,
                self.inner.changed(),
                source,
            )
            .await;
            match result {
                Ok(()) => {
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Ok,
                            queue_len: None,
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Ok(())
                }
                Err(err) => {
                    if let Ok(mut state) = self.channel.lock() {
                        if state.tx_close_cause.is_none() {
                            state.tx_close_cause = Some(ChannelCloseCause::SenderDropped);
                        }
                        if state.rx_close_cause.is_none() {
                            state.rx_close_cause = Some(ChannelCloseCause::SenderDropped);
                        }
                    }
                    apply_watch_state(&self.channel);
                    if let Ok(event) = Event::channel_received(
                        EventTarget::Entity(self.handle.id().clone()),
                        &ChannelReceiveEvent {
                            outcome: ChannelReceiveOutcome::Closed,
                            queue_len: None,
                        },
                    ) {
                        if let Ok(mut db) = runtime_db().lock() {
                            db.record_event(event);
                        }
                    }
                    Err(err)
                }
            }
        }
    }

    #[track_caller]
    pub fn borrow(&self) -> watch::Ref<'_, T> {
        self.inner.borrow()
    }

    #[track_caller]
    pub fn borrow_and_update(&mut self) -> watch::Ref<'_, T> {
        self.inner.borrow_and_update()
    }

    #[track_caller]
    pub fn has_changed(&self) -> Result<bool, watch::error::RecvError> {
        self.inner.has_changed()
    }
}
#[track_caller]
pub fn broadcast<T: Clone>(
    name: impl Into<CompactString>,
    capacity: usize,
    source: Source,
) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
    let name = name.into();
    let (tx, rx) = broadcast::channel(capacity);
    let capacity_u32 = capacity.min(u32::MAX as usize) as u32;
    let details = ChannelDetails::Broadcast(peeps_types::BroadcastChannelDetails {
        buffer: Some(BufferState {
            occupancy: 0,
            capacity: Some(capacity_u32),
        }),
    });
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    let details = ChannelDetails::Broadcast(peeps_types::BroadcastChannelDetails {
        buffer: Some(BufferState {
            occupancy: 0,
            capacity: Some(capacity_u32),
        }),
    });
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    let channel = Arc::new(StdMutex::new(BroadcastRuntimeState {
        tx_id: tx_handle.id().clone(),
        rx_id: rx_handle.id().clone(),
        tx_ref_count: 1,
        rx_ref_count: 1,
        capacity: capacity_u32,
        tx_close_cause: None,
        rx_close_cause: None,
    }));
    (
        BroadcastSender {
            inner: tx,
            handle: tx_handle,
            receiver_handle: rx_handle.clone(),
            channel: channel.clone(),
            name: name.clone(),
        },
        BroadcastReceiver {
            inner: rx,
            handle: rx_handle,
            channel,
            name,
        },
    )
}

#[macro_export]
macro_rules! broadcast {
    ($name:expr, $capacity:expr $(,)?) => {
        $crate::broadcast($name, $capacity, $crate::Source::caller())
    };
}
pub fn watch<T: Clone>(
    name: impl Into<CompactString>,
    initial: T,
    source: Source,
) -> (WatchSender<T>, WatchReceiver<T>) {
    let name = name.into();
    let (tx, rx) = watch::channel(initial);
    let details = ChannelDetails::Watch(WatchChannelDetails {
        last_update_at: None,
    });
    let tx_handle = EntityHandle::new(
        format!("{name}:tx"),
        EntityBody::ChannelTx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    let details = ChannelDetails::Watch(WatchChannelDetails {
        last_update_at: None,
    });
    let rx_handle = EntityHandle::new(
        format!("{name}:rx"),
        EntityBody::ChannelRx(ChannelEndpointEntity {
            lifecycle: ChannelEndpointLifecycle::Open,
            details,
        }),
        source,
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    let channel = Arc::new(StdMutex::new(WatchRuntimeState {
        tx_id: tx_handle.id().clone(),
        rx_id: rx_handle.id().clone(),
        tx_ref_count: 1,
        rx_ref_count: 1,
        tx_close_cause: None,
        rx_close_cause: None,
        last_update_at: None,
    }));
    (
        WatchSender {
            inner: tx,
            handle: tx_handle,
            receiver_handle: rx_handle.clone(),
            channel: channel.clone(),
            name: name.clone(),
        },
        WatchReceiver {
            inner: rx,
            handle: rx_handle,
            channel,
            name,
        },
    )
}
pub fn watch_channel<T: Clone>(
    name: impl Into<CompactString>,
    initial: T,
    source: Source,
) -> (WatchSender<T>, WatchReceiver<T>) {
    #[allow(deprecated)]
    watch(name, initial, source)
}

#[macro_export]
macro_rules! watch {
    ($name:expr, $initial:expr $(,)?) => {
        $crate::watch($name, $initial, $crate::Source::caller())
    };
}

impl Notify {
    pub fn new(name: impl Into<String>, source: Source) -> Self {
        let name = name.into();
        let handle = EntityHandle::new(
            name,
            EntityBody::Notify(NotifyEntity { waiter_count: 0 }),
            source,
        );
        Self {
            inner: Arc::new(tokio::sync::Notify::new()),
            handle,
            waiter_count: Arc::new(AtomicU32::new(0)),
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn notified(&self) -> impl Future<Output = ()> + '_ {
        let source = Source::caller();
        async move {
            let waiters = self
                .waiter_count
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            if let Ok(mut db) = runtime_db().lock() {
                db.update_notify_waiter_count(self.handle.id(), waiters);
            }

            instrument_operation_on_with_source(
                &self.handle,
                OperationKind::NotifyWait,
                self.inner.notified(),
                source,
            )
            .await;

            let waiters = self
                .waiter_count
                .fetch_sub(1, Ordering::Relaxed)
                .saturating_sub(1);
            if let Ok(mut db) = runtime_db().lock() {
                db.update_notify_waiter_count(self.handle.id(), waiters);
            }
        }
    }

    #[track_caller]
    pub fn notify_one(&self) {
        self.inner.notify_one();
    }

    #[track_caller]
    pub fn notify_waiters(&self) {
        self.inner.notify_waiters();
    }
}

#[macro_export]
macro_rules! notify {
    ($name:expr $(,)?) => {
        $crate::Notify::new($name, $crate::Source::caller())
    };
}

impl<T> OnceCell<T> {
    pub fn new(name: impl Into<String>, source: Source) -> Self {
        let handle = EntityHandle::new(
            name.into(),
            EntityBody::OnceCell(OnceCellEntity {
                waiter_count: 0,
                state: OnceCellState::Empty,
            }),
            source,
        );
        Self {
            inner: tokio::sync::OnceCell::new(),
            handle,
            waiter_count: AtomicU32::new(0),
        }
    }

    #[track_caller]
    pub fn get(&self) -> Option<&T> {
        self.inner.get()
    }

    #[track_caller]
    pub fn initialized(&self) -> bool {
        self.inner.initialized()
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn get_or_init<'a, F, Fut>(&'a self, f: F) -> impl Future<Output = &'a T> + 'a
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = T> + 'a,
    {
        let source = Source::caller();
        async move {
            let waiters = self
                .waiter_count
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            if let Ok(mut db) = runtime_db().lock() {
                db.update_once_cell_state(self.handle.id(), waiters, OnceCellState::Initializing);
            }

            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::OncecellWait,
                self.inner.get_or_init(f),
                source,
            )
            .await;

            let waiters = self
                .waiter_count
                .fetch_sub(1, Ordering::Relaxed)
                .saturating_sub(1);
            let state = if self.inner.initialized() {
                OnceCellState::Initialized
            } else if waiters > 0 {
                OnceCellState::Initializing
            } else {
                OnceCellState::Empty
            };
            if let Ok(mut db) = runtime_db().lock() {
                db.update_once_cell_state(self.handle.id(), waiters, state);
            }

            result
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn get_or_try_init<'a, F, Fut, E>(
        &'a self,
        f: F,
    ) -> impl Future<Output = Result<&'a T, E>> + 'a
    where
        F: FnOnce() -> Fut + 'a,
        Fut: Future<Output = Result<T, E>> + 'a,
    {
        let source = Source::caller();
        async move {
            let waiters = self
                .waiter_count
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
            if let Ok(mut db) = runtime_db().lock() {
                db.update_once_cell_state(self.handle.id(), waiters, OnceCellState::Initializing);
            }

            let result = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::OncecellWait,
                self.inner.get_or_try_init(f),
                source,
            )
            .await;

            let waiters = self
                .waiter_count
                .fetch_sub(1, Ordering::Relaxed)
                .saturating_sub(1);
            let state = if self.inner.initialized() {
                OnceCellState::Initialized
            } else if waiters > 0 {
                OnceCellState::Initializing
            } else {
                OnceCellState::Empty
            };
            if let Ok(mut db) = runtime_db().lock() {
                db.update_once_cell_state(self.handle.id(), waiters, state);
            }

            result
        }
    }

    #[track_caller]
    pub fn set(&self, value: T) -> Result<(), T> {
        let result = self.inner.set(value).map_err(|e| match e {
            tokio::sync::SetError::AlreadyInitializedError(v) => v,
            tokio::sync::SetError::InitializingError(v) => v,
        });
        let state = if self.inner.initialized() {
            OnceCellState::Initialized
        } else if self.waiter_count.load(Ordering::Relaxed) > 0 {
            OnceCellState::Initializing
        } else {
            OnceCellState::Empty
        };
        if let Ok(mut db) = runtime_db().lock() {
            db.update_once_cell_state(
                self.handle.id(),
                self.waiter_count.load(Ordering::Relaxed),
                state,
            );
        }
        result
    }
}

#[macro_export]
macro_rules! once_cell {
    ($name:expr $(,)?) => {
        $crate::OnceCell::new($name, $crate::Source::caller())
    };
}

impl Semaphore {
    pub fn new(name: impl Into<String>, permits: usize, source: Source) -> Self {
        let max_permits = permits.min(u32::MAX as usize) as u32;
        let handle = EntityHandle::new(
            name.into(),
            EntityBody::Semaphore(SemaphoreEntity {
                max_permits,
                handed_out_permits: 0,
            }),
            source,
        );
        Self {
            inner: Arc::new(tokio::sync::Semaphore::new(permits)),
            handle,
            max_permits: Arc::new(AtomicU32::new(max_permits)),
            holder_counts: Arc::new(StdMutex::new(BTreeMap::new())),
        }
    }

    #[track_caller]
    pub fn available_permits(&self) -> usize {
        self.inner.available_permits()
    }

    #[track_caller]
    pub fn close(&self) {
        self.inner.close();
    }

    #[track_caller]
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    #[track_caller]
    pub fn add_permits(&self, n: usize) {
        self.inner.add_permits(n);
        let delta = n.min(u32::MAX as usize) as u32;
        let max = self
            .max_permits
            .fetch_add(delta, Ordering::Relaxed)
            .saturating_add(delta);
        self.sync_state(max);
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn acquire(
        &self,
    ) -> impl Future<Output = Result<SemaphorePermit<'_>, tokio::sync::AcquireError>> + '_ {
        let source = Source::caller();
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        async move {
            let permit = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Acquire,
                self.inner.acquire(),
                source,
            )
            .await?;
            if let Some(holder_id) = holder_future_id.as_ref() {
                self.note_holder_acquired(holder_id);
            }
            self.sync_state(self.max_permits.load(Ordering::Relaxed));
            Ok(SemaphorePermit {
                inner: Some(permit),
                semaphore_id: self.handle.id().clone(),
                holder_future_id,
                holder_counts: Arc::clone(&self.holder_counts),
                semaphore: Arc::clone(&self.inner),
                max_permits: Arc::clone(&self.max_permits),
            })
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn acquire_many(
        &self,
        n: u32,
    ) -> impl Future<Output = Result<SemaphorePermit<'_>, tokio::sync::AcquireError>> + '_ {
        let source = Source::caller();
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        async move {
            let permit = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Acquire,
                self.inner.acquire_many(n),
                source,
            )
            .await?;
            if let Some(holder_id) = holder_future_id.as_ref() {
                self.note_holder_acquired(holder_id);
            }
            self.sync_state(self.max_permits.load(Ordering::Relaxed));
            Ok(SemaphorePermit {
                inner: Some(permit),
                semaphore_id: self.handle.id().clone(),
                holder_future_id,
                holder_counts: Arc::clone(&self.holder_counts),
                semaphore: Arc::clone(&self.inner),
                max_permits: Arc::clone(&self.max_permits),
            })
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn acquire_owned(
        &self,
    ) -> impl Future<Output = Result<OwnedSemaphorePermit, tokio::sync::AcquireError>> + '_ {
        let source = Source::caller();
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        async move {
            let permit = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Acquire,
                Arc::clone(&self.inner).acquire_owned(),
                source,
            )
            .await?;
            if let Some(holder_id) = holder_future_id.as_ref() {
                self.note_holder_acquired(holder_id);
            }
            self.sync_state(self.max_permits.load(Ordering::Relaxed));
            Ok(OwnedSemaphorePermit {
                inner: Some(permit),
                semaphore_id: self.handle.id().clone(),
                holder_future_id,
                holder_counts: Arc::clone(&self.holder_counts),
                semaphore: Arc::clone(&self.inner),
                max_permits: Arc::clone(&self.max_permits),
            })
        }
    }

    #[track_caller]
    #[allow(clippy::manual_async_fn)]
    pub fn acquire_many_owned(
        &self,
        n: u32,
    ) -> impl Future<Output = Result<OwnedSemaphorePermit, tokio::sync::AcquireError>> + '_ {
        let source = Source::caller();
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        async move {
            let permit = instrument_operation_on_with_source(
                &self.handle,
                OperationKind::Acquire,
                Arc::clone(&self.inner).acquire_many_owned(n),
                source,
            )
            .await?;
            if let Some(holder_id) = holder_future_id.as_ref() {
                self.note_holder_acquired(holder_id);
            }
            self.sync_state(self.max_permits.load(Ordering::Relaxed));
            Ok(OwnedSemaphorePermit {
                inner: Some(permit),
                semaphore_id: self.handle.id().clone(),
                holder_future_id,
                holder_counts: Arc::clone(&self.holder_counts),
                semaphore: Arc::clone(&self.inner),
                max_permits: Arc::clone(&self.max_permits),
            })
        }
    }

    #[track_caller]
    pub fn try_acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire()?;
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        if let Some(holder_id) = holder_future_id.as_ref() {
            self.note_holder_acquired(holder_id);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(SemaphorePermit {
            inner: Some(permit),
            semaphore_id: self.handle.id().clone(),
            holder_future_id,
            holder_counts: Arc::clone(&self.holder_counts),
            semaphore: Arc::clone(&self.inner),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[track_caller]
    pub fn try_acquire_many(
        &self,
        n: u32,
    ) -> Result<SemaphorePermit<'_>, tokio::sync::TryAcquireError> {
        let permit = self.inner.try_acquire_many(n)?;
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        if let Some(holder_id) = holder_future_id.as_ref() {
            self.note_holder_acquired(holder_id);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(SemaphorePermit {
            inner: Some(permit),
            semaphore_id: self.handle.id().clone(),
            holder_future_id,
            holder_counts: Arc::clone(&self.holder_counts),
            semaphore: Arc::clone(&self.inner),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[track_caller]
    pub fn try_acquire_owned(&self) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let permit = Arc::clone(&self.inner).try_acquire_owned()?;
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        if let Some(holder_id) = holder_future_id.as_ref() {
            self.note_holder_acquired(holder_id);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(OwnedSemaphorePermit {
            inner: Some(permit),
            semaphore_id: self.handle.id().clone(),
            holder_future_id,
            holder_counts: Arc::clone(&self.holder_counts),
            semaphore: Arc::clone(&self.inner),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    #[track_caller]
    pub fn try_acquire_many_owned(
        &self,
        n: u32,
    ) -> Result<OwnedSemaphorePermit, tokio::sync::TryAcquireError> {
        let permit = Arc::clone(&self.inner).try_acquire_many_owned(n)?;
        let holder_future_id = current_causal_target().map(|target| target.id().clone());
        if let Some(holder_id) = holder_future_id.as_ref() {
            self.note_holder_acquired(holder_id);
        }
        self.sync_state(self.max_permits.load(Ordering::Relaxed));
        Ok(OwnedSemaphorePermit {
            inner: Some(permit),
            semaphore_id: self.handle.id().clone(),
            holder_future_id,
            holder_counts: Arc::clone(&self.holder_counts),
            semaphore: Arc::clone(&self.inner),
            max_permits: Arc::clone(&self.max_permits),
        })
    }

    fn sync_state(&self, max_permits: u32) {
        sync_semaphore_state(self.handle.id(), &self.inner, max_permits);
    }

    fn note_holder_acquired(&self, holder_id: &EntityId) {
        let should_insert = if let Ok(mut holder_counts) = self.holder_counts.lock() {
            let count = holder_counts.entry(holder_id.clone()).or_insert(0);
            *count = count.saturating_add(1);
            *count == 1
        } else {
            false
        };
        if should_insert {
            if let Ok(mut db) = runtime_db().lock() {
                db.upsert_edge(self.handle.id(), holder_id, EdgeKind::Holds);
            }
        }
    }
}

fn sync_semaphore_state(
    semaphore_id: &EntityId,
    semaphore: &Arc<tokio::sync::Semaphore>,
    max_permits: u32,
) {
    let available = semaphore.available_permits().min(u32::MAX as usize) as u32;
    let handed_out_permits = max_permits.saturating_sub(available);
    if let Ok(mut db) = runtime_db().lock() {
        db.update_semaphore_state(semaphore_id, max_permits, handed_out_permits);
    }
}

fn release_semaphore_holder_edge(
    semaphore_id: &EntityId,
    holder_future_id: &mut Option<EntityId>,
    holder_counts: &Arc<StdMutex<BTreeMap<EntityId, u32>>>,
) {
    let Some(holder_id) = holder_future_id.take() else {
        return;
    };

    let should_remove = if let Ok(mut counts) = holder_counts.lock() {
        match counts.get_mut(&holder_id) {
            None => false,
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            Some(_) => {
                counts.remove(&holder_id);
                true
            }
        }
    } else {
        false
    };

    if should_remove {
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_edge(semaphore_id, &holder_id, EdgeKind::Holds);
        }
    }
}

impl<'a> Deref for SemaphorePermit<'a> {
    type Target = tokio::sync::SemaphorePermit<'a>;

    fn deref(&self) -> &Self::Target {
        self.inner
            .as_ref()
            .expect("semaphore permit accessed after drop")
    }
}

impl<'a> DerefMut for SemaphorePermit<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
            .as_mut()
            .expect("semaphore permit accessed after drop")
    }
}

impl<'a> Drop for SemaphorePermit<'a> {
    fn drop(&mut self) {
        let _ = self.inner.take();
        release_semaphore_holder_edge(
            &self.semaphore_id,
            &mut self.holder_future_id,
            &self.holder_counts,
        );
        sync_semaphore_state(
            &self.semaphore_id,
            &self.semaphore,
            self.max_permits.load(Ordering::Relaxed),
        );
    }
}

impl Deref for OwnedSemaphorePermit {
    type Target = tokio::sync::OwnedSemaphorePermit;

    fn deref(&self) -> &Self::Target {
        self.inner
            .as_ref()
            .expect("owned semaphore permit accessed after drop")
    }
}

impl DerefMut for OwnedSemaphorePermit {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
            .as_mut()
            .expect("owned semaphore permit accessed after drop")
    }
}

impl Drop for OwnedSemaphorePermit {
    fn drop(&mut self) {
        let _ = self.inner.take();
        release_semaphore_holder_edge(
            &self.semaphore_id,
            &mut self.holder_future_id,
            &self.holder_counts,
        );
        sync_semaphore_state(
            &self.semaphore_id,
            &self.semaphore,
            self.max_permits.load(Ordering::Relaxed),
        );
    }
}

#[macro_export]
macro_rules! semaphore {
    ($name:expr, $permits:expr $(,)?) => {
        $crate::Semaphore::new($name, $permits, $crate::Source::caller())
    };
}

impl Command {
    #[track_caller]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        let program = CompactString::from(program.as_ref().to_string_lossy().as_ref());
        Self {
            inner: tokio::process::Command::new(program.as_str()),
            program,
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    #[track_caller]
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        let arg = arg.as_ref().to_owned();
        self.args
            .push(CompactString::from(arg.to_string_lossy().as_ref()));
        self.inner.arg(&arg);
        self
    }

    #[track_caller]
    pub fn args(&mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> &mut Self {
        let args: Vec<OsString> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        for arg in &args {
            self.args
                .push(CompactString::from(arg.to_string_lossy().as_ref()));
        }
        self.inner.args(args);
        self
    }

    #[track_caller]
    pub fn env(&mut self, key: impl AsRef<OsStr>, val: impl AsRef<OsStr>) -> &mut Self {
        let key = key.as_ref().to_owned();
        let val = val.as_ref().to_owned();
        self.env.push(CompactString::from(format!(
            "{}={}",
            key.to_string_lossy(),
            val.to_string_lossy()
        )));
        self.inner.env(&key, &val);
        self
    }

    #[track_caller]
    pub fn envs(
        &mut self,
        vars: impl IntoIterator<Item = (impl AsRef<OsStr>, impl AsRef<OsStr>)>,
    ) -> &mut Self {
        let vars: Vec<(OsString, OsString)> = vars
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_owned(), v.as_ref().to_owned()))
            .collect();
        for (k, v) in &vars {
            self.env.push(CompactString::from(format!(
                "{}={}",
                k.to_string_lossy(),
                v.to_string_lossy()
            )));
        }
        self.inner.envs(vars);
        self
    }

    #[track_caller]
    pub fn env_clear(&mut self) -> &mut Self {
        self.env.clear();
        self.inner.env_clear();
        self
    }

    #[track_caller]
    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        let key = key.as_ref().to_owned();
        let key_prefix = format!("{}=", key.to_string_lossy());
        self.env
            .retain(|entry| !entry.as_str().starts_with(&key_prefix));
        self.inner.env_remove(&key);
        self
    }

    #[track_caller]
    pub fn current_dir(&mut self, dir: impl AsRef<std::path::Path>) -> &mut Self {
        self.inner.current_dir(dir);
        self
    }

    #[track_caller]
    pub fn stdin(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stdin(cfg);
        self
    }

    #[track_caller]
    pub fn stdout(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stdout(cfg);
        self
    }

    #[track_caller]
    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.inner.stderr(cfg);
        self
    }

    #[track_caller]
    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.inner.kill_on_drop(kill_on_drop);
        self
    }

    #[track_caller]
    pub fn spawn(&mut self) -> io::Result<Child> {
        let child = self.inner.spawn()?;
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), Source::caller());
        Ok(Child {
            inner: Some(child),
            handle,
        })
    }

    #[track_caller]
    pub fn status(&mut self) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), Source::caller());
        instrument_future_on("command.status", &handle, self.inner.status(), Source::caller())
    }

    #[track_caller]
    pub fn output(&mut self) -> impl Future<Output = io::Result<Output>> + '_ {
        let handle = EntityHandle::new(self.entity_name(), self.entity_body(), Source::caller());
        instrument_future_on("command.output", &handle, self.inner.output(), Source::caller())
    }

    #[track_caller]
    pub fn as_std(&self) -> &std::process::Command {
        self.inner.as_std()
    }

    #[cfg(unix)]
    pub unsafe fn pre_exec<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut() -> io::Result<()> + Send + Sync + 'static,
    {
        self.inner.pre_exec(f);
        self
    }

    #[track_caller]
    pub fn into_inner(self) -> tokio::process::Command {
        self.inner
    }

    #[track_caller]
    pub fn into_inner_with_diagnostics(self) -> (tokio::process::Command, CommandDiagnostics) {
        let diag = CommandDiagnostics {
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
        };
        (self.inner, diag)
    }

    fn entity_name(&self) -> CompactString {
        CompactString::from(format!("command.{}", self.program))
    }

    fn entity_body(&self) -> EntityBody {
        EntityBody::Command(CommandEntity {
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.clone(),
        })
    }
}

impl Child {
    #[track_caller]
    pub fn from_tokio_with_diagnostics(
        child: tokio::process::Child,
        diag: CommandDiagnostics,
    ) -> Self {
        let body = EntityBody::Command(CommandEntity {
            program: diag.program.clone(),
            args: diag.args.clone(),
            env: diag.env.clone(),
        });
        let name = CompactString::from(format!("command.{}", diag.program));
        let handle = EntityHandle::new(name, body, Source::caller());
        Self {
            inner: Some(child),
            handle,
        }
    }

    fn inner(&self) -> &tokio::process::Child {
        self.inner.as_ref().expect("child already consumed")
    }

    fn inner_mut(&mut self) -> &mut tokio::process::Child {
        self.inner.as_mut().expect("child already consumed")
    }

    #[track_caller]
    pub fn id(&self) -> Option<u32> {
        self.inner().id()
    }

    #[track_caller]
    pub fn wait(&mut self) -> impl Future<Output = io::Result<ExitStatus>> + '_ {
        let handle = self.handle.clone();
        let wait_fut = self.inner_mut().wait();
        instrument_future_on("command.wait", &handle, wait_fut, Source::caller())
    }

    #[track_caller]
    pub fn wait_with_output(mut self) -> impl Future<Output = io::Result<Output>> {
        let child = self.inner.take().expect("child already consumed");
        instrument_future_on(
            "command.wait_with_output",
            &self.handle,
            child.wait_with_output(),
            Source::caller(),
        )
    }

    #[track_caller]
    pub fn start_kill(&mut self) -> io::Result<()> {
        self.inner_mut().start_kill()
    }

    #[track_caller]
    pub fn kill(&mut self) -> io::Result<()> {
        self.start_kill()
    }

    #[track_caller]
    pub fn stdin(&mut self) -> &mut Option<tokio::process::ChildStdin> {
        &mut self.inner_mut().stdin
    }

    #[track_caller]
    pub fn stdout(&mut self) -> &mut Option<tokio::process::ChildStdout> {
        &mut self.inner_mut().stdout
    }

    #[track_caller]
    pub fn stderr(&mut self) -> &mut Option<tokio::process::ChildStderr> {
        &mut self.inner_mut().stderr
    }

    #[track_caller]
    pub fn take_stdin(&mut self) -> Option<tokio::process::ChildStdin> {
        self.inner_mut().stdin.take()
    }

    #[track_caller]
    pub fn take_stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.inner_mut().stdout.take()
    }

    #[track_caller]
    pub fn take_stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.inner_mut().stderr.take()
    }
}

impl<T> JoinSet<T>
where
    T: Send + 'static,
{
    pub fn named(name: impl Into<String>, source: Source) -> Self {
        let name = name.into();
        let handle = EntityHandle::new(format!("joinset.{name}"), EntityBody::Future, source);
        Self {
            inner: tokio::task::JoinSet::new(),
            handle,
        }
    }
    pub fn with_name(name: impl Into<String>, source: Source) -> Self {
        #[allow(deprecated)]
        Self::named(name, source)
    }

    #[track_caller]
    pub fn spawn<F>(&mut self, label: &'static str, future: F)
    where
        F: Future<Output = T> + Send + 'static,
    {
        let joinset_handle = self.handle.clone();
        let source = Source::caller();
        self.inner.spawn(
            FUTURE_CAUSAL_STACK.scope(RefCell::new(Vec::new()), async move {
                instrument_future_on_with_source(label, &joinset_handle, future, source).await
            }),
        );
    }

    #[track_caller]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[track_caller]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[track_caller]
    pub fn abort_all(&mut self) {
        self.inner.abort_all();
    }

    #[track_caller]
    pub fn join_next(
        &mut self,
    ) -> impl Future<Output = Option<Result<T, tokio::task::JoinError>>> + '_ {
        let handle = self.handle.clone();
        let fut = self.inner.join_next();
        instrument_future_on("joinset.join_next", &handle, fut, Source::caller())
    }
}

#[macro_export]
macro_rules! join_set {
    ($name:expr $(,)?) => {
        $crate::JoinSet::named($name, $crate::Source::caller())
    };
}

impl DiagnosticInterval {
    #[track_caller]
    pub fn tick(&mut self) -> impl Future<Output = tokio::time::Instant> + '_ {
        instrument_future_on("interval.tick", &self.handle, self.inner.tick(), Source::caller())
    }

    #[track_caller]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[track_caller]
    pub fn period(&self) -> Duration {
        self.inner.period()
    }

    #[track_caller]
    pub fn set_missed_tick_behavior(&mut self, behavior: tokio::time::MissedTickBehavior) {
        self.inner.set_missed_tick_behavior(behavior);
    }
}

pub fn interval(period: Duration, source: Source) -> DiagnosticInterval {
    let label = format!("interval({}ms)", period.as_millis());
    DiagnosticInterval {
        inner: tokio::time::interval(period),
        handle: EntityHandle::new(label, EntityBody::Future, source),
    }
}

pub fn interval_at(start: tokio::time::Instant, period: Duration, source: Source) -> DiagnosticInterval {
    let label = format!("interval({}ms)", period.as_millis());
    DiagnosticInterval {
        inner: tokio::time::interval_at(start, period),
        handle: EntityHandle::new(label, EntityBody::Future, source),
    }
}

pub trait SnapshotSink {
    fn entity(&mut self, entity: &Entity);
    fn scope(&mut self, _scope: &Scope) {}
    fn edge(&mut self, edge: &Edge);
    fn event(&mut self, event: &Event);
}

#[track_caller]
pub fn write_snapshot_to<S>(sink: &mut S)
where
    S: SnapshotSink,
{
    let Ok(db) = runtime_db().lock() else {
        return;
    };
    for entity in db.entities.values() {
        sink.entity(entity);
    }
    for scope in db.scopes.values() {
        sink.scope(scope);
    }
    for edge in db.edges.values() {
        sink.edge(edge);
    }
    for event in &db.events {
        sink.event(event);
    }
}

#[track_caller]
pub fn pull_changes_since(from_seq_no: SeqNo, max_changes: u32) -> PullChangesResponse {
    let stream_id = runtime_stream_id();
    let Ok(db) = runtime_db().lock() else {
        return PullChangesResponse {
            stream_id,
            from_seq_no,
            next_seq_no: from_seq_no,
            changes: Vec::new(),
            truncated: false,
            compacted_before_seq_no: None,
        };
    };
    db.pull_changes_since(from_seq_no, max_changes)
}

#[track_caller]
pub fn current_cursor() -> StreamCursor {
    let stream_id = runtime_stream_id();
    let Ok(db) = runtime_db().lock() else {
        return StreamCursor {
            stream_id,
            next_seq_no: SeqNo::ZERO,
        };
    };
    db.current_cursor()
}

#[track_caller]
pub fn ack_cut(cut_id: impl Into<CompactString>) -> CutAck {
    CutAck {
        cut_id: CutId(cut_id.into()),
        cursor: current_cursor(),
    }
}

struct OperationFuture<F> {
    inner: F,
    actor_id: Option<EntityId>,
    resource_id: EntityId,
    op_kind: OperationKind,
    source: CompactString,
    krate: Option<CompactString>,
    poll_count: u64,
    pending_since_ptime_ms: Option<u64>,
    has_edge: bool,
}

impl<F> OperationFuture<F> {
    fn new(
        inner: F,
        resource_id: EntityId,
        op_kind: OperationKind,
        source: CompactString,
        krate: Option<CompactString>,
    ) -> Self {
        let actor_id = current_causal_target().map(|target| target.id().clone());
        if let Some(actor_id) = actor_id.as_ref() {
            if let Ok(mut db) = runtime_db().lock() {
                db.upsert_edge(actor_id, &resource_id, EdgeKind::Touches);
            }
        }
        Self {
            inner,
            actor_id,
            resource_id,
            op_kind,
            source,
            krate,
            poll_count: 0,
            pending_since_ptime_ms: None,
            has_edge: false,
        }
    }

    fn edge_meta(&self, state: OperationState) -> facet_value::Value {
        let meta = OperationEdgeMeta {
            op_kind: self.op_kind,
            state,
            pending_since_ptime_ms: self.pending_since_ptime_ms,
            last_change_ptime_ms: PTime::now().as_millis(),
            source: CompactString::from(self.source.as_str()),
            krate: self.krate.as_ref().map(|k| CompactString::from(k.as_str())),
            poll_count: Some(self.poll_count),
            details: None,
        };
        facet_value::to_value(&meta).unwrap_or(facet_value::Value::NULL)
    }

    fn upsert_edge(&mut self, state: OperationState) {
        let Some(actor_id) = self.actor_id.as_ref() else {
            return;
        };
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge_with_meta(
                actor_id,
                &self.resource_id,
                EdgeKind::Needs,
                self.edge_meta(state),
            );
            self.has_edge = true;
        }
    }

    fn clear_edge(&mut self) {
        if !self.has_edge {
            return;
        }
        let Some(actor_id) = self.actor_id.as_ref() else {
            return;
        };
        if let Ok(mut db) = runtime_db().lock() {
            db.remove_edge(actor_id, &self.resource_id, EdgeKind::Needs);
            self.has_edge = false;
        }
    }
}

impl<F> Future for OperationFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        this.poll_count = this.poll_count.saturating_add(1);
        if !this.has_edge {
            this.upsert_edge(OperationState::Active);
        }

        match unsafe { Pin::new_unchecked(&mut this.inner) }.poll(cx) {
            Poll::Pending => {
                if this.pending_since_ptime_ms.is_none() {
                    this.pending_since_ptime_ms = Some(PTime::now().as_millis());
                }
                this.upsert_edge(OperationState::Pending);
                Poll::Pending
            }
            Poll::Ready(output) => {
                this.clear_edge();
                Poll::Ready(output)
            }
        }
    }
}

impl<F> Drop for OperationFuture<F> {
    fn drop(&mut self) {
        self.clear_edge();
    }
}

fn instrument_operation_on_with_source<F>(
    on: &EntityHandle,
    op_kind: OperationKind,
    fut: F,
    source: Source,
) -> OperationFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    OperationFuture::new(
        fut.into_future(),
        EntityId::new(on.id().as_str()),
        op_kind,
        source.into_compact_string(),
        None,
    )
}

pub struct InstrumentedFuture<F> {
    inner: F,
    future_handle: EntityHandle,
    awaited_by: Option<FutureEdgeRelation>,
    waits_on: Option<FutureEdgeRelation>,
}

#[derive(Clone, Copy)]
enum FutureEdgeDirection {
    ParentToChild,
    ChildToTarget,
}

struct FutureEdgeRelation {
    target: EntityRef,
    direction: FutureEdgeDirection,
    current_edge: Option<EdgeKind>,
}

impl FutureEdgeRelation {
    fn new(target: EntityRef, direction: FutureEdgeDirection) -> Self {
        Self {
            target,
            direction,
            current_edge: None,
        }
    }
}

impl<F> InstrumentedFuture<F> {
    fn new(inner: F, future_handle: EntityHandle, target: Option<EntityRef>) -> Self {
        let awaited_by = current_causal_target().and_then(|parent| {
            if parent.id().as_str() == future_handle.id().as_str() {
                None
            } else {
                Some(FutureEdgeRelation::new(
                    parent,
                    FutureEdgeDirection::ParentToChild,
                ))
            }
        });
        let waits_on = target
            .map(|target| FutureEdgeRelation::new(target, FutureEdgeDirection::ChildToTarget));
        Self {
            inner,
            future_handle,
            awaited_by,
            waits_on,
        }
    }
}

fn future_relation_endpoints(
    future_id: &EntityId,
    relation: &FutureEdgeRelation,
) -> (EntityId, EntityId) {
    match relation.direction {
        FutureEdgeDirection::ParentToChild => (
            EntityId::new(relation.target.id().as_str()),
            EntityId::new(future_id.as_str()),
        ),
        FutureEdgeDirection::ChildToTarget => (
            EntityId::new(future_id.as_str()),
            EntityId::new(relation.target.id().as_str()),
        ),
    }
}

fn ensure_relation_polls_edge(future_id: &EntityId, relation: &mut FutureEdgeRelation) {
    if relation.current_edge.is_some() {
        return;
    }
    let (src, dst) = future_relation_endpoints(future_id, relation);
    if let Ok(mut db) = runtime_db().lock() {
        db.upsert_edge(&src, &dst, EdgeKind::Polls);
    }
    relation.current_edge = Some(EdgeKind::Polls);
}

fn ensure_relation_needs_edge(future_id: &EntityId, relation: &mut FutureEdgeRelation) {
    if relation.current_edge == Some(EdgeKind::Needs) {
        return;
    }
    let (src, dst) = future_relation_endpoints(future_id, relation);
    if let Ok(mut db) = runtime_db().lock() {
        if relation.current_edge == Some(EdgeKind::Polls) {
            db.remove_edge(&src, &dst, EdgeKind::Polls);
        }
        db.upsert_edge(&src, &dst, EdgeKind::Needs);
    }
    relation.current_edge = Some(EdgeKind::Needs);
}

fn clear_relation_edge(future_id: &EntityId, relation: &mut FutureEdgeRelation) {
    let Some(kind) = relation.current_edge else {
        return;
    };
    let (src, dst) = future_relation_endpoints(future_id, relation);
    if let Ok(mut db) = runtime_db().lock() {
        db.remove_edge(&src, &dst, kind);
    }
    relation.current_edge = None;
}

impl<F> Future for InstrumentedFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };
        let future_id = EntityId::new(this.future_handle.id().as_str());
        let pushed = FUTURE_CAUSAL_STACK
            .try_with(|stack| {
                stack.borrow_mut().push(EntityId::new(future_id.as_str()));
            })
            .is_ok();

        if let Some(relation) = this.awaited_by.as_mut() {
            ensure_relation_polls_edge(&future_id, relation);
        }
        if let Some(relation) = this.waits_on.as_mut() {
            ensure_relation_polls_edge(&future_id, relation);
        }

        let poll = unsafe { Pin::new_unchecked(&mut this.inner) }.poll(cx);
        if pushed {
            let _ = FUTURE_CAUSAL_STACK.try_with(|stack| {
                stack.borrow_mut().pop();
            });
        }
        match poll {
            Poll::Pending => {
                if let Some(relation) = this.awaited_by.as_mut() {
                    ensure_relation_needs_edge(&future_id, relation);
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    ensure_relation_needs_edge(&future_id, relation);
                }
                Poll::Pending
            }
            Poll::Ready(output) => {
                if let Some(relation) = this.awaited_by.as_mut() {
                    clear_relation_edge(&future_id, relation);
                }
                if let Some(relation) = this.waits_on.as_mut() {
                    clear_relation_edge(&future_id, relation);
                }

                if let Ok(event) =
                    Event::new(EventTarget::Entity(future_id), EventKind::StateChanged, &())
                {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }

                Poll::Ready(output)
            }
        }
    }
}

impl<F> Drop for InstrumentedFuture<F> {
    fn drop(&mut self) {
        let future_id = EntityId::new(self.future_handle.id().as_str());
        if let Some(relation) = self.awaited_by.as_mut() {
            clear_relation_edge(&future_id, relation);
        }
        if let Some(relation) = self.waits_on.as_mut() {
            clear_relation_edge(&future_id, relation);
        }
    }
}

pub fn instrument_future_named<F>(
    name: impl Into<CompactString>,
    fut: F,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    instrument_future_named_with_source(name, fut, source)
}

pub fn instrument_future_named_with_source<F>(
    name: impl Into<CompactString>,
    fut: F,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let fut = fut.into_future();
    let handle = EntityHandle::new_with_source(name, EntityBody::Future, source);
    InstrumentedFuture::new(fut, handle, None)
}

pub fn instrument_future_on<F>(
    name: impl Into<CompactString>,
    on: &impl AsEntityRef,
    fut: F,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    instrument_future_on_with_source(name, on, fut, source)
}

pub fn instrument_future_on_with_source<F>(
    name: impl Into<CompactString>,
    on: &impl AsEntityRef,
    fut: F,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let fut = fut.into_future();
    let on_ref = on.as_entity_ref();
    let handle = EntityHandle::new_with_source(name, EntityBody::Future, source);
    InstrumentedFuture::new(fut, handle, Some(on_ref))
}

#[doc(hidden)]
pub fn instrument_future_named_with_meta<F>(
    name: impl Into<CompactString>,
    fut: F,
    meta: &facet_value::Value,
    source: Source,
) -> InstrumentedFuture<F::IntoFuture>
where
    F: IntoFuture,
{
    let fut = fut.into_future();
    let mut entity = Entity::builder(name, EntityBody::Future)
        .source(source.into_compact_string())
        .build(&())
        .expect("entity construction with unit meta should be infallible");
    entity.meta = meta.clone();
    let handle = EntityHandle::from_entity(entity);
    InstrumentedFuture::new(fut, handle, None)
}

#[macro_export]
macro_rules! peeps {
    (name = $name:expr, fut = $fut:expr $(,)?) => {{
        $crate::instrument_future_named($name, $fut, $crate::Source::caller())
    }};
    (name = $name:expr, on = $on:expr, fut = $fut:expr $(,)?) => {{
        $crate::instrument_future_on($name, &$on, $fut, $crate::Source::caller())
    }};
}

#[macro_export]
macro_rules! peep {
    ($fut:expr, $name:expr $(,)?) => {{
        $crate::instrument_future_named($name, $fut, $crate::Source::caller())
    }};
    ($fut:expr, $name:expr, {$($k:literal => $v:expr),* $(,)?} $(,)?) => {{
        let mut __peeps_meta_pairs: Vec<(&'static str, $crate::facet_value::Value)> = Vec::new();
        $(
            __peeps_meta_pairs.push((
                $k,
                $crate::facet_value::to_value(&$v)
                    .expect("`peep!` metadata value must be Facet-serializable"),
            ));
        )*
        let __peeps_meta: $crate::facet_value::Value = __peeps_meta_pairs.into_iter().collect();
        $crate::instrument_future_named_with_meta(
            $name,
            $fut,
            &__peeps_meta,
            $crate::Source::caller(),
        )
    }};
    ($fut:expr, $name:expr, level = $($rest:tt)*) => {{
        compile_error!("`level=` is deprecated");
    }};
    ($fut:expr, $name:expr, kind = $($rest:tt)*) => {{
        compile_error!("`kind=` is deprecated");
    }};
    ($fut:expr, $name:expr, $($rest:tt)+) => {{
        compile_error!("invalid `peep!` arguments");
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex as StdMutex, OnceLock};
    use std::task::{Context, Poll, Wake, Waker};
    use std::time::Duration;

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    struct PendingOnceThenReady {
        pending: bool,
    }

    impl Future for PendingOnceThenReady {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.pending {
                self.pending = false;
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        }
    }

    struct AlwaysPending;

    impl Future for AlwaysPending {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: OnceLock<StdMutex<()>> = OnceLock::new();
        GUARD
            .get_or_init(|| StdMutex::new(()))
            .lock()
            .expect("test guard mutex poisoned")
    }

    fn reset_runtime_db_for_test() {
        let mut db = runtime_db()
            .lock()
            .expect("runtime db lock should be available");
        *db = RuntimeDb::new(runtime_stream_id(), MAX_EVENTS);
        HELD_MUTEX_STACK.with(|stack| stack.borrow_mut().clear());
    }

    fn edge_exists(src: &EntityId, dst: &EntityId, kind: EdgeKind) -> bool {
        let db = runtime_db()
            .lock()
            .expect("runtime db lock should be available");
        db.edges.contains_key(&EdgeKey {
            src: EntityId::new(src.as_str()),
            dst: EntityId::new(dst.as_str()),
            kind,
        })
    }

    fn edge_exists_any(src: &EntityId, dst: &EntityId) -> bool {
        edge_exists(src, dst, EdgeKind::Needs) || edge_exists(src, dst, EdgeKind::Polls)
    }

    fn entity_exists(id: &EntityId) -> bool {
        let db = runtime_db()
            .lock()
            .expect("runtime db lock should be available");
        db.entities.contains_key(id)
    }

    fn entity_id_by_name(name: &str) -> Option<EntityId> {
        let db = runtime_db()
            .lock()
            .expect("runtime db lock should be available");
        db.entities
            .values()
            .find(|entity| entity.name.as_str() == name)
            .map(|entity| EntityId::new(entity.id.as_str()))
    }

    fn entity_source_by_name(name: &str) -> Option<CompactString> {
        let db = runtime_db()
            .lock()
            .expect("runtime db lock should be available");
        db.entities
            .values()
            .find(|entity| entity.name.as_str() == name)
            .map(|entity| entity.source.clone())
    }

    #[test]
    fn instrument_future_named_uses_caller_source() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let marker_line = line!() + 1;
        let fut = instrument_future_named(
            "test.future.source",
            std::future::ready(()),
            Source::caller(),
        );
        let fut_id = EntityId::new(fut.future_handle.id().as_str());
        let source = {
            let db = runtime_db()
                .lock()
                .expect("runtime db lock should be available");
            db.entities
                .get(&fut_id)
                .expect("future entity should exist")
                .source
                .clone()
        };

        assert!(
            source.ends_with(&format!(":{}", marker_line)),
            "expected caller line {}, got source {}",
            marker_line,
            source
        );
    }

    #[test]
    fn peeps_macro_tracks_caller_source() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let marker_line = line!() + 1;
        let fut = crate::peeps!(
            name = "test.future.macro_source",
            fut = std::future::ready(())
        );
        let fut_id = EntityId::new(fut.future_handle.id().as_str());
        let source = {
            let db = runtime_db()
                .lock()
                .expect("runtime db lock should be available");
            db.entities
                .get(&fut_id)
                .expect("future entity should exist")
                .source
                .clone()
        };

        assert!(
            source.ends_with(&format!(":{}", marker_line)),
            "expected caller line {}, got source {}",
            marker_line,
            source
        );
    }

    #[test]
    fn peep_macro_records_meta_fields() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let fut = crate::peep!(
            std::future::ready(()),
            "test.future.meta_fields",
            {
                "method" => "Store.put_chunk",
                "chunk.bytes" => 42u64,
            }
        );
        let fut_id = EntityId::new(fut.future_handle.id().as_str());
        let meta = {
            let db = runtime_db()
                .lock()
                .expect("runtime db lock should be available");
            db.entities
                .get(&fut_id)
                .expect("future entity should exist")
                .meta
                .clone()
        };

        let meta_obj = meta.as_object().expect("future meta should be an object");
        assert_eq!(
            meta_obj
                .get("method")
                .and_then(|v| v.as_string())
                .map(|s| s.as_str()),
            Some("Store.put_chunk")
        );
        assert_eq!(
            meta_obj
                .get("chunk.bytes")
                .and_then(|v| v.as_number())
                .and_then(|n| n.to_u64()),
            Some(42)
        );
    }

    #[test]
    fn instrumented_future_promotes_polls_to_needs_and_clears_on_ready() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let target = EntityHandle::new("test.target.transition", EntityBody::Future);
        let fut = instrument_future_on(
            "test.future.transition",
            &target,
            PendingOnceThenReady { pending: true },
            Source::caller(),
        );
        let fut_id = EntityId::new(fut.future_handle.id().as_str());

        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        let mut fut = Box::pin(fut);

        assert!(matches!(fut.as_mut().poll(&mut cx), Poll::Pending));
        assert!(edge_exists(&fut_id, target.id(), EdgeKind::Needs));
        assert!(!edge_exists(&fut_id, target.id(), EdgeKind::Polls));

        assert!(matches!(fut.as_mut().poll(&mut cx), Poll::Ready(())));
        assert!(!edge_exists(&fut_id, target.id(), EdgeKind::Needs));
        assert!(!edge_exists(&fut_id, target.id(), EdgeKind::Polls));
    }

    #[test]
    fn dropping_pending_instrumented_future_clears_edge_without_entity_teardown() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let target = EntityHandle::new("test.target.drop", EntityBody::Future);
        let fut = instrument_future_on(
            "test.future.drop",
            &target,
            AlwaysPending,
            Source::caller(),
        );
        let fut_handle = fut.future_handle.clone();
        let fut_id = EntityId::new(fut_handle.id().as_str());

        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        let mut fut = Box::pin(fut);

        assert!(matches!(fut.as_mut().poll(&mut cx), Poll::Pending));
        assert!(edge_exists(&fut_id, target.id(), EdgeKind::Needs));
        assert!(entity_exists(&fut_id));

        drop(fut);
        assert!(entity_exists(&fut_id));
        assert!(!edge_exists(&fut_id, target.id(), EdgeKind::Needs));
        assert!(!edge_exists(&fut_id, target.id(), EdgeKind::Polls));
    }

    #[test]
    fn dropping_pending_operation_future_clears_needs_but_keeps_touches() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let sem = crate::semaphore!("test.semaphore.touch.resource", 0);
        let sem_id = entity_id_by_name("test.semaphore.touch.resource")
            .expect("semaphore entity should exist");

        let fut = crate::peep!(sem.acquire_owned(), "test.semaphore.touch.acquire");
        let fut_handle = fut.future_handle.clone();
        let fut_id = EntityId::new(fut_handle.id().as_str());

        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        let mut fut = Box::pin(fut);

        assert!(matches!(fut.as_mut().poll(&mut cx), Poll::Pending));
        assert!(edge_exists(&fut_id, &sem_id, EdgeKind::Needs));
        assert!(edge_exists(&fut_id, &sem_id, EdgeKind::Touches));

        drop(fut);
        assert!(entity_exists(&fut_id));
        assert!(!edge_exists(&fut_id, &sem_id, EdgeKind::Needs));
        assert!(edge_exists(&fut_id, &sem_id, EdgeKind::Touches));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn peep_child_future_links_to_current_parent_future() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let parent = spawn_tracked("test.parent.future", async {
            crate::peep!(std::future::pending::<()>(), "test.child.future").await;
        });

        let mut found = false;
        for _ in 0..64 {
            tokio::task::yield_now().await;
            let Some(parent_id) = entity_id_by_name("test.parent.future") else {
                continue;
            };
            let Some(child_id) = entity_id_by_name("test.child.future") else {
                continue;
            };
            if edge_exists(&parent_id, &child_id, EdgeKind::Needs) {
                found = true;
                break;
            }
        }

        parent.abort();
        let _ = parent.await;

        assert!(
            found,
            "expected child future to link to parent future via needs edge"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn peep_with_on_keeps_parent_and_resource_chain() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let target = EntityHandle::new("test.resource.target", EntityBody::Future);
        let target_id = EntityId::new(target.id().as_str());
        let parent = spawn_tracked("test.parent.with_on", async move {
            crate::peeps!(
                name = "test.child.with_on",
                on = target,
                fut = std::future::pending::<()>()
            )
            .await;
        });

        let mut chain_found = false;
        for _ in 0..64 {
            tokio::task::yield_now().await;
            let Some(parent_id) = entity_id_by_name("test.parent.with_on") else {
                continue;
            };
            let Some(child_id) = entity_id_by_name("test.child.with_on") else {
                continue;
            };
            if edge_exists_any(&parent_id, &child_id) && edge_exists_any(&child_id, &target_id) {
                chain_found = true;
                break;
            }
        }

        parent.abort();
        let _ = parent.await;

        assert!(
            chain_found,
            "expected parent->child and child->target await chain edges"
        );
    }

    #[test]
    fn mutex_creates_lock_entity() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let _lock = crate::mutex!("test.lock.entity", ());
        let lock_id = entity_id_by_name("test.lock.entity").expect("lock entity should exist");
        let db = runtime_db()
            .lock()
            .expect("runtime db lock should be available");
        let body = &db
            .entities
            .get(&lock_id)
            .expect("lock entity should be persisted")
            .body;
        match body {
            EntityBody::Lock(lock) => match &lock.kind {
                LockKind::Mutex => {}
                _ => panic!("expected mutex lock entity kind"),
            },
            _ => panic!("expected lock entity body"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn contended_mutex_lock_connects_waiter_and_holder_through_lock() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let lock = Arc::new(crate::mutex!("test.lock.shared.async", ()));
        let barrier = Arc::new(std::sync::Barrier::new(2));

        let lock_for_holder = Arc::clone(&lock);
        let barrier_for_holder = Arc::clone(&barrier);
        let holder = crate::spawn_tracked!("test.lock.holder.async", async move {
            let _guard = lock_for_holder.lock();
            barrier_for_holder.wait();
            std::thread::sleep(Duration::from_millis(150));
        });

        let lock_for_waiter = Arc::clone(&lock);
        let barrier_for_waiter = Arc::clone(&barrier);
        let waiter = crate::spawn_tracked!("test.lock.waiter.async", async move {
            barrier_for_waiter.wait();
            let _guard = lock_for_waiter.lock();
        });

        let mut saw_expected_edges = false;
        for _ in 0..60 {
            let Some(holder_id) = entity_id_by_name("test.lock.holder.async") else {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            };
            let Some(waiter_id) = entity_id_by_name("test.lock.waiter.async") else {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            };
            let Some(lock_id) = entity_id_by_name("test.lock.shared.async") else {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            };

            if edge_exists(&waiter_id, &lock_id, EdgeKind::Needs)
                && edge_exists(&lock_id, &holder_id, EdgeKind::Needs)
            {
                saw_expected_edges = true;
                break;
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let _ = holder.await;
        let _ = waiter.await;

        assert!(
            saw_expected_edges,
            "expected waiter->lock and lock->holder needs edges while contention is active"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn contended_semaphore_connects_waiter_and_holder_through_holds_edge() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let sem = Arc::new(crate::semaphore!("test.semaphore.shared.async", 1));
        let barrier = Arc::new(std::sync::Barrier::new(2));

        let sem_for_holder = Arc::clone(&sem);
        let barrier_for_holder = Arc::clone(&barrier);
        let holder = crate::spawn_tracked!("test.semaphore.holder.async", async move {
            let _permit = sem_for_holder
                .acquire_owned()
                .await
                .expect("holder should acquire permit");
            barrier_for_holder.wait();
            std::thread::sleep(Duration::from_millis(150));
        });

        let sem_for_waiter = Arc::clone(&sem);
        let barrier_for_waiter = Arc::clone(&barrier);
        let waiter = crate::spawn_tracked!("test.semaphore.waiter.async", async move {
            barrier_for_waiter.wait();
            let _permit = crate::peep!(
                sem_for_waiter.acquire_owned(),
                "test.semaphore.waiter.acquire"
            )
            .await
            .expect("waiter should eventually acquire permit");
        });

        let mut saw_expected_edges = false;
        for _ in 0..60 {
            let Some(holder_id) = entity_id_by_name("test.semaphore.holder.async") else {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            };
            let Some(waiter_acquire_id) = entity_id_by_name("test.semaphore.waiter.acquire") else {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            };
            let Some(sem_id) = entity_id_by_name("test.semaphore.shared.async") else {
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            };

            if edge_exists(&waiter_acquire_id, &sem_id, EdgeKind::Needs)
                && edge_exists(&sem_id, &holder_id, EdgeKind::Holds)
            {
                saw_expected_edges = true;
                break;
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let _ = holder.await;
        let _ = waiter.await;

        assert!(
            saw_expected_edges,
            "expected waiter->semaphore needs edge and semaphore->holder holds edge while contention is active"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn semaphore_acquire_owned_uses_caller_source() {
        let _guard = test_guard();
        reset_runtime_db_for_test();

        let sem = crate::semaphore!("test.semaphore.source", 0);
        let marker_line = line!() + 1;
        let waiter = tokio::spawn(async move {
            let _ = sem.acquire_owned().await;
        });

        let mut source = None;
        for _ in 0..64 {
            tokio::task::yield_now().await;
            source = entity_source_by_name("semaphore.acquire_owned");
            if source.is_some() {
                break;
            }
        }

        waiter.abort();
        let _ = waiter.await;

        let source = source.expect("semaphore.acquire_owned future should be tracked");
        assert!(
            source.ends_with(&format!(":{}", marker_line))
                || source.ends_with(&format!(":{}", marker_line + 1)),
            "expected caller line {} (or {}), got source {}",
            marker_line,
            marker_line + 1,
            source
        );
    }
}
