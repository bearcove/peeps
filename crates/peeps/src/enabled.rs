use compact_str::CompactString;
use peeps_types::{
    BufferState, Change, ChannelCloseCause, ChannelClosedEvent, ChannelDetails,
    ChannelEndpointEntity, ChannelEndpointLifecycle, ChannelReceiveEvent, ChannelReceiveOutcome,
    ChannelSendEvent, ChannelSendOutcome, ChannelWaitEndedEvent, ChannelWaitKind,
    ChannelWaitStartedEvent, CutAck, CutId, Edge, EdgeKind, Entity, EntityBody, EntityId, Event,
    EventKind, EventTarget, MpscChannelDetails, PullChangesResponse, Scope, ScopeBody, ScopeId,
    SeqNo, StampedChange, StreamCursor, StreamId,
};
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll};
#[cfg(feature = "dashboard")]
use std::time::Duration;
use std::time::Instant;
#[cfg(feature = "dashboard")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "dashboard")]
use tokio::net::TcpStream;
use tokio::sync::mpsc;
#[cfg(feature = "dashboard")]
use tokio::time::{interval, MissedTickBehavior};

#[cfg(feature = "dashboard")]
use peeps_wire::{
    decode_server_message_default, encode_client_message_default, ClientMessage, ServerMessage,
};

const MAX_EVENTS: usize = 16_384;
const MAX_CHANGES_BEFORE_COMPACT: usize = 65_536;
const COMPACT_TARGET_CHANGES: usize = 8_192;
const DEFAULT_STREAM_ID_PREFIX: &str = "proc";
static PROCESS_SCOPE: OnceLock<ScopeHandle> = OnceLock::new();
#[cfg(feature = "dashboard")]
const DASHBOARD_PUSH_MAX_CHANGES: u32 = 2048;
#[cfg(feature = "dashboard")]
const DASHBOARD_PUSH_INTERVAL_MS: u64 = 100;
#[cfg(feature = "dashboard")]
const DASHBOARD_RECONNECT_DELAY_MS: u64 = 500;

pub fn init(process_name: &str) {
    ensure_process_scope(process_name);

    #[cfg(feature = "dashboard")]
    init_dashboard_push_loop(process_name);

    #[cfg(not(feature = "dashboard"))]
    let _ = process_name;
}

fn ensure_process_scope(process_name: &str) {
    PROCESS_SCOPE.get_or_init(|| ScopeHandle::new(process_name, ScopeBody::Process));
}

fn current_process_scope_id() -> Option<ScopeId> {
    PROCESS_SCOPE
        .get()
        .map(|scope| ScopeId::new(scope.id().as_str()))
}

#[cfg(feature = "dashboard")]
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

#[cfg(feature = "dashboard")]
async fn run_dashboard_push_loop(addr: String, process_name: CompactString) {
    loop {
        let connected = run_dashboard_session(&addr, process_name.clone()).await;
        let _ = connected;
        tokio::time::sleep(Duration::from_millis(DASHBOARD_RECONNECT_DELAY_MS)).await;
    }
}

#[cfg(feature = "dashboard")]
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
    let mut ticker = interval(Duration::from_millis(DASHBOARD_PUSH_INTERVAL_MS));
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
                    ServerMessage::SnapshotRequest(_) => {}
                }
            }
        }
    }
}

#[cfg(feature = "dashboard")]
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

#[cfg(feature = "dashboard")]
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

pub fn spawn_tracked<F>(
    name: impl Into<CompactString>,
    fut: F,
) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(instrument_future_named(name, fut))
}

fn runtime_db() -> &'static Mutex<RuntimeDb> {
    static DB: OnceLock<Mutex<RuntimeDb>> = OnceLock::new();
    DB.get_or_init(|| Mutex::new(RuntimeDb::new(runtime_stream_id(), MAX_EVENTS)))
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
                if let ChannelDetails::Mpsc(details) = &mut endpoint.details {
                    if details.buffer != buffer {
                        details.buffer = buffer;
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
        let key = EdgeKey {
            src: EntityId::new(src.as_str()),
            dst: EntityId::new(dst.as_str()),
            kind,
        };
        if self.edges.contains_key(&key) {
            return;
        }
        let edge = Edge {
            src: EntityId::new(src.as_str()),
            dst: EntityId::new(dst.as_str()),
            kind,
            meta: facet_value::Value::NULL,
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
    pub fn id(&self) -> &EntityId {
        &self.id
    }
}

pub fn entity_ref_from_wire(id: impl Into<CompactString>) -> EntityRef {
    EntityRef {
        id: EntityId::new(id.into()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeRef {
    id: ScopeId,
}

impl ScopeRef {
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
    pub fn new(name: impl Into<CompactString>, body: ScopeBody) -> Self {
        let scope = Scope::builder(name, body)
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

    pub fn id(&self) -> &ScopeId {
        &self.inner.id
    }

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
    pub fn new(name: impl Into<CompactString>, body: EntityBody) -> Self {
        let entity = Entity::builder(name, body)
            .build(&())
            .expect("entity construction with unit meta should be infallible");
        let id = EntityId::new(entity.id.as_str());

        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_entity(entity);
        }

        Self {
            inner: Arc::new(HandleInner { id }),
        }
    }

    pub fn id(&self) -> &EntityId {
        &self.inner.id
    }

    pub fn entity_ref(&self) -> EntityRef {
        EntityRef {
            id: EntityId::new(self.inner.id.as_str()),
        }
    }

    pub fn link_to(&self, target: &EntityRef, kind: EdgeKind) {
        if let Ok(mut db) = runtime_db().lock() {
            db.upsert_edge(self.id(), target.id(), kind);
        }
    }

    pub fn link_to_handle(&self, target: &EntityHandle, kind: EdgeKind) {
        self.link_to(&target.entity_ref(), kind);
    }
}

pub struct Sender<T> {
    inner: mpsc::Sender<T>,
    handle: EntityHandle,
    channel: Arc<Mutex<ChannelRuntimeState>>,
    name: CompactString,
}

pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
    handle: EntityHandle,
    channel: Arc<Mutex<ChannelRuntimeState>>,
    name: CompactString,
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

fn sync_channel_state(
    channel: &Arc<Mutex<ChannelRuntimeState>>,
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

fn apply_channel_state(channel: &Arc<Mutex<ChannelRuntimeState>>) {
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

impl<T> Sender<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn send(&self, value: T) -> Result<(), mpsc::error::SendError<T>> {
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

        let result = instrument_future_on(
            format!("{}.send", self.name),
            &self.handle,
            self.inner.send(value),
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

impl<T> Receiver<T> {
    pub fn handle(&self) -> &EntityHandle {
        &self.handle
    }

    pub async fn recv(&mut self) -> Option<T> {
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

        let result = instrument_future_on(
            format!("{}.recv", self.name),
            &self.handle,
            self.inner.recv(),
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

pub fn channel<T>(name: impl Into<CompactString>, capacity: usize) -> (Sender<T>, Receiver<T>) {
    let name = name.into();
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
    );
    tx_handle.link_to_handle(&rx_handle, EdgeKind::ChannelLink);
    let channel = Arc::new(Mutex::new(ChannelRuntimeState {
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

pub trait SnapshotSink {
    fn entity(&mut self, entity: &Entity);
    fn scope(&mut self, _scope: &Scope) {}
    fn edge(&mut self, edge: &Edge);
    fn event(&mut self, event: &Event);
}

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

pub fn ack_cut(cut_id: impl Into<CompactString>) -> CutAck {
    CutAck {
        cut_id: CutId(cut_id.into()),
        cursor: current_cursor(),
    }
}

pub struct InstrumentedFuture<F> {
    inner: F,
    future_handle: EntityHandle,
    target: Option<EntityRef>,
    current_edge: Option<EdgeKind>,
}

impl<F> InstrumentedFuture<F> {
    fn new(inner: F, future_handle: EntityHandle, target: Option<EntityRef>) -> Self {
        Self {
            inner,
            future_handle,
            target,
            current_edge: None,
        }
    }
}

impl<F> Future for InstrumentedFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = unsafe { self.get_unchecked_mut() };

        if let Some(target) = &this.target {
            if this.current_edge.is_none() {
                if let Ok(mut db) = runtime_db().lock() {
                    db.upsert_edge(this.future_handle.id(), target.id(), EdgeKind::Polls);
                }
                this.current_edge = Some(EdgeKind::Polls);
            }
        }

        let poll = unsafe { Pin::new_unchecked(&mut this.inner) }.poll(cx);
        match poll {
            Poll::Pending => {
                if let Some(target) = &this.target {
                    if this.current_edge != Some(EdgeKind::Needs) {
                        if let Ok(mut db) = runtime_db().lock() {
                            if this.current_edge == Some(EdgeKind::Polls) {
                                db.remove_edge(
                                    this.future_handle.id(),
                                    target.id(),
                                    EdgeKind::Polls,
                                );
                            }
                            db.upsert_edge(this.future_handle.id(), target.id(), EdgeKind::Needs);
                        }
                        this.current_edge = Some(EdgeKind::Needs);
                    }
                }
                Poll::Pending
            }
            Poll::Ready(output) => {
                if let Some(target) = &this.target {
                    if let Ok(mut db) = runtime_db().lock() {
                        if let Some(kind) = this.current_edge {
                            db.remove_edge(this.future_handle.id(), target.id(), kind);
                        }
                    }
                    this.current_edge = None;
                }

                if let Ok(event) = Event::new(
                    EventTarget::Entity(this.future_handle.id().clone()),
                    EventKind::StateChanged,
                    &(),
                ) {
                    if let Ok(mut db) = runtime_db().lock() {
                        db.record_event(event);
                    }
                }

                Poll::Ready(output)
            }
        }
    }
}

pub fn instrument_future_named<F>(name: impl Into<CompactString>, fut: F) -> InstrumentedFuture<F>
where
    F: Future,
{
    let handle = EntityHandle::new(name, EntityBody::Future);
    InstrumentedFuture::new(fut, handle, None)
}

pub fn instrument_future_on<F>(
    name: impl Into<CompactString>,
    on: &EntityHandle,
    fut: F,
) -> InstrumentedFuture<F>
where
    F: Future,
{
    let handle = EntityHandle::new(name, EntityBody::Future);
    InstrumentedFuture::new(fut, handle, Some(on.entity_ref()))
}

#[macro_export]
macro_rules! peeps {
    (name = $name:expr, fut = $fut:expr $(,)?) => {{
        $crate::instrument_future_named($name, $fut)
    }};
    (name = $name:expr, on = $on:expr, fut = $fut:expr $(,)?) => {{
        $crate::instrument_future_on($name, &$on, $fut)
    }};
}

#[macro_export]
macro_rules! peep {
    ($fut:expr, $name:expr $(,)?) => {{
        $crate::instrument_future_named($name, $fut)
    }};
    ($fut:expr, $name:expr, $meta:tt $(,)?) => {{
        let _ = &$meta;
        $crate::instrument_future_named($name, $fut)
    }};
}
