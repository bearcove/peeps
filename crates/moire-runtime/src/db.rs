use facet::Facet;
use moire_trace_types::BacktraceId;
use moire_types::{
    Change, Edge, EdgeKind, Entity, EntityBody, EntityId, Event, EventTarget, PTime,
    PullChangesResponse, Scope, ScopeBody, ScopeId, SeqNo, StampedChange, StreamCursor, StreamId,
    TaskScopeBody,
};
use std::collections::{BTreeMap, VecDeque, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex as StdMutex, OnceLock};

use super::{
    COMPACT_TARGET_CHANGES, MAX_CHANGES_BEFORE_COMPACT, current_process_scope_id,
    current_tokio_task_key,
};

pub(crate) fn runtime_db() -> &'static StdMutex<RuntimeDb> {
    static DB: OnceLock<StdMutex<RuntimeDb>> = OnceLock::new();
    DB.get_or_init(|| StdMutex::new(RuntimeDb::new(runtime_stream_id(), super::MAX_EVENTS)))
}

pub(crate) fn runtime_stream_id() -> StreamId {
    StreamId(super::runtime_process_id().as_str().to_owned())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct EdgeKey {
    pub(super) src: EntityId,
    pub(super) dst: EntityId,
    pub(super) kind: EdgeKind,
}

pub(crate) struct RuntimeDb {
    stream_id: StreamId,
    next_seq_no: SeqNo,
    compacted_before_seq_no: Option<SeqNo>,
    pub(super) entities: BTreeMap<EntityId, Entity>,
    pub(super) scopes: BTreeMap<ScopeId, Scope>,
    task_scope_ids: BTreeMap<String, ScopeId>,
    pub(super) entity_scope_links: BTreeMap<(EntityId, ScopeId), ()>,
    pub(super) edges: BTreeMap<EdgeKey, Edge>,
    pub(super) events: VecDeque<Event>,
    changes: VecDeque<InternalStampedChange>,
    max_events: usize,
    /// Reference counts for entity IDs referenced by events in the ring buffer.
    event_entity_refs: BTreeMap<EntityId, usize>,
}

impl RuntimeDb {
    pub(crate) fn new(stream_id: StreamId, max_events: usize) -> Self {
        Self {
            stream_id,
            next_seq_no: SeqNo::ZERO,
            compacted_before_seq_no: None,
            entities: BTreeMap::new(),
            scopes: BTreeMap::new(),
            task_scope_ids: BTreeMap::new(),
            entity_scope_links: BTreeMap::new(),
            edges: BTreeMap::new(),
            events: VecDeque::with_capacity(max_events.min(256)),
            changes: VecDeque::new(),
            max_events,
            event_entity_refs: BTreeMap::new(),
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
                    if let std::collections::btree_map::Entry::Vacant(e) =
                        seen_entity_scope_links.entry(key)
                    {
                        e.insert(());
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
                    if let std::collections::btree_map::Entry::Vacant(e) = seen_edges.entry(key) {
                        e.insert(());
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
        if let (Some(old_front), Some(new_front)) = (old_front, new_front)
            && new_front > old_front
        {
            self.compacted_before_seq_no = Some(
                self.compacted_before_seq_no
                    .map(|existing| existing.max(new_front))
                    .unwrap_or(new_front),
            );
        }
        // TODO: replace this with checkpoint-aware compaction once we plumb
        // checkpoint materialization and replay handoff.
    }

    pub(crate) fn upsert_entity(&mut self, entity: Entity) {
        let entity_id = EntityId::new(entity.id.as_str());
        let should_link_task_scope = matches!(&entity.body, EntityBody::Future(_));
        let entity_json = facet_json::to_vec(&entity).ok();
        self.entities
            .insert(EntityId::new(entity.id.as_str()), entity);
        if let Some(scope_id) = current_process_scope_id() {
            self.link_entity_to_scope(&entity_id, &scope_id);
        }
        if should_link_task_scope && let Some(scope_id) = self.ensure_current_task_scope_id() {
            self.link_entity_to_scope(&entity_id, &scope_id);
        }
        if let Some(entity_json) = entity_json {
            self.push_change(InternalChange::UpsertEntity {
                id: entity_id,
                entity_json,
            });
        }
    }

    pub(crate) fn upsert_scope(&mut self, scope: Scope) {
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

    pub(crate) fn register_task_scope_id(&mut self, task_key: &str, scope_id: &ScopeId) {
        self.task_scope_ids
            .insert(String::from(task_key), ScopeId::new(scope_id.as_str()));
    }

    pub(crate) fn unregister_task_scope_id(&mut self, task_key: &str, scope_id: &ScopeId) {
        if self
            .task_scope_ids
            .get(task_key)
            .is_some_and(|registered| registered == scope_id)
        {
            self.task_scope_ids.remove(task_key);
        }
    }

    fn ensure_current_task_scope_id(&mut self) -> Option<ScopeId> {
        let task_key = current_tokio_task_key()?;
        if let Some(existing_scope_id) = self.task_scope_ids.get(&task_key).cloned() {
            if self.scopes.contains_key(&existing_scope_id) {
                return Some(existing_scope_id);
            }
            self.task_scope_ids.remove(&task_key);
        }

        let source = current_process_scope_id()
            .and_then(|process_scope_id| {
                self.scopes
                    .get(&process_scope_id)
                    .map(|scope| scope.backtrace)
            })
            .expect("task scope creation requires a process scope source");
        let scope = Scope::new(
            source,
            format!("task.{task_key}"),
            ScopeBody::Task(TaskScopeBody {
                task_key: task_key.clone(),
            }),
        );
        let scope_id = ScopeId::new(scope.id.as_str());
        self.upsert_scope(scope);
        self.task_scope_ids
            .insert(task_key, ScopeId::new(scope_id.as_str()));
        Some(scope_id)
    }

    pub(crate) fn link_entity_to_current_task_scope(&mut self, entity_id: &EntityId) -> Option<()> {
        let scope_id = self.ensure_current_task_scope_id()?;
        self.link_entity_to_scope(entity_id, &scope_id);
        Some(())
    }

    fn body_fingerprint(body: &EntityBody) -> u64 {
        let bytes = facet_json::to_vec(body).expect("entity body serialization must succeed");
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        hasher.finish()
    }

    pub(crate) fn rename_entity_and_maybe_upsert(
        &mut self,
        id: &EntityId,
        name: impl Into<String>,
    ) -> bool {
        let name = name.into();
        let entity_json = {
            let Some(entity) = self.entities.get_mut(id) else {
                return false;
            };
            if entity.removed_at.is_some() {
                return false;
            }
            if entity.name == name {
                return false;
            }
            entity.name = name;
            facet_json::to_vec(entity).ok()
        };

        if let Some(entity_json) = entity_json {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }
        true
    }

    pub(crate) fn mutate_entity_body_and_maybe_upsert(
        &mut self,
        id: &EntityId,
        mutate: impl FnOnce(&mut EntityBody),
    ) -> bool {
        let entity_json = {
            let Some(entity) = self.entities.get_mut(id) else {
                return false;
            };
            if entity.removed_at.is_some() {
                return false;
            }
            let before = Self::body_fingerprint(&entity.body);
            mutate(&mut entity.body);
            let after = Self::body_fingerprint(&entity.body);
            if before == after {
                return false;
            }
            facet_json::to_vec(entity).expect("entity serialization must succeed")
        };
        self.push_change(InternalChange::UpsertEntity {
            id: EntityId::new(id.as_str()),
            entity_json,
        });
        true
    }

    pub(crate) fn remove_entity(&mut self, id: &EntityId) {
        let Some(entity) = self.entities.get_mut(id) else {
            return;
        };
        // Idempotent: skip if already marked dead.
        if entity.removed_at.is_some() {
            return;
        }
        entity.removed_at = Some(PTime::now());

        // Emit UpsertEntity with removed_at set so clients see the death.
        let entity_json = facet_json::to_vec(entity).ok();

        // Still remove edges and scope links immediately (graph structure).
        let mut links_to_remove = Vec::new();
        for entity_scope in self.entity_scope_links.keys() {
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

        if let Some(entity_json) = entity_json {
            self.push_change(InternalChange::UpsertEntity {
                id: EntityId::new(id.as_str()),
                entity_json,
            });
        }

        // If no events reference this entity, sweep immediately.
        if !self.event_entity_refs.contains_key(id) {
            self.entities.remove(id);
            self.push_change(InternalChange::RemoveEntity {
                id: EntityId::new(id.as_str()),
            });
        }
    }

    pub(crate) fn remove_scope(&mut self, id: &ScopeId) {
        if self.scopes.remove(id).is_none() {
            return;
        }
        self.task_scope_ids.retain(|_, scope_id| scope_id != id);
        let mut links_to_remove = Vec::new();
        for entity_scope in self.entity_scope_links.keys() {
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

    pub(crate) fn link_entity_to_scope(&mut self, entity_id: &EntityId, scope_id: &ScopeId) {
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

    pub(crate) fn unlink_entity_from_scope(&mut self, entity_id: &EntityId, scope_id: &ScopeId) {
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

    pub(crate) fn upsert_edge(
        &mut self,
        src: &EntityId,
        dst: &EntityId,
        kind: EdgeKind,
        backtrace: BacktraceId,
    ) {
        // Skip if either endpoint is dead.
        if self
            .entities
            .get(src)
            .is_some_and(|e| e.removed_at.is_some())
            || self
                .entities
                .get(dst)
                .is_some_and(|e| e.removed_at.is_some())
        {
            return;
        }
        if let Some(process_scope_id) = current_process_scope_id() {
            if self.entities.contains_key(src) {
                self.link_entity_to_scope(src, &process_scope_id);
            }
            if self.entities.contains_key(dst) {
                self.link_entity_to_scope(dst, &process_scope_id);
            }
        }
        let key = EdgeKey {
            src: EntityId::new(src.as_str()),
            dst: EntityId::new(dst.as_str()),
            kind,
        };
        if self.edges.contains_key(&key) {
            return;
        }
        let edge = Edge::new(
            EntityId::new(src.as_str()),
            EntityId::new(dst.as_str()),
            kind,
            backtrace,
        );
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

    pub(crate) fn remove_edge(&mut self, src: &EntityId, dst: &EntityId, kind: EdgeKind) {
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

    pub(crate) fn record_event(&mut self, event: Event) {
        // Increment ref count for entity-targeted events.
        if let EventTarget::Entity(ref id) = event.target {
            *self
                .event_entity_refs
                .entry(EntityId::new(id.as_str()))
                .or_insert(0) += 1;
        }
        let event_json = facet_json::to_vec(&event).ok();
        self.events.push_back(event);
        // Evict old events and decrement ref counts.
        while self.events.len() > self.max_events {
            if let Some(evicted) = self.events.pop_front()
                && let EventTarget::Entity(ref id) = evicted.target
            {
                let entity_id = EntityId::new(id.as_str());
                if let Some(count) = self.event_entity_refs.get_mut(&entity_id) {
                    *count -= 1;
                    if *count == 0 {
                        self.event_entity_refs.remove(&entity_id);
                        // If entity is dead and no longer referenced, sweep it.
                        self.sweep_dead_entity(&entity_id);
                    }
                }
            }
        }
        if let Some(event_json) = event_json {
            self.push_change(InternalChange::AppendEvent { event_json });
        }
    }

    /// Remove a dead entity from the map and emit a `RemoveEntity` change.
    /// Only acts if the entity exists and has `removed_at` set.
    fn sweep_dead_entity(&mut self, id: &EntityId) {
        let should_sweep = self
            .entities
            .get(id)
            .is_some_and(|e| e.removed_at.is_some());
        if should_sweep {
            self.entities.remove(id);
            self.push_change(InternalChange::RemoveEntity {
                id: EntityId::new(id.as_str()),
            });
        }
    }

    pub(crate) fn pull_changes_since(
        &self,
        from_seq_no: SeqNo,
        max_changes: u32,
    ) -> PullChangesResponse {
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

    pub(crate) fn current_cursor(&self) -> StreamCursor {
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

#[derive(Facet)]
struct SnapshotRef<'a> {
    entities: Vec<&'a Entity>,
    scopes: Vec<&'a Scope>,
    edges: Vec<&'a Edge>,
    events: Vec<&'a Event>,
}

#[derive(Facet)]
struct SnapshotReplyRef<'a> {
    snapshot_id: i64,
    /// Process-relative milliseconds at the moment this snapshot was assembled.
    ptime_now_ms: u64,
    #[facet(skip_unless_truthy)]
    snapshot: Option<SnapshotRef<'a>>,
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
#[allow(dead_code)]
enum SnapshotClientMessageRef<'a> {
    SnapshotReply(SnapshotReplyRef<'a>),
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

pub(crate) fn encode_snapshot_reply_frame(snapshot_id: i64) -> Result<Vec<u8>, String> {
    // Capture process-relative now before locking the db, so the timestamp
    // represents the moment this snapshot was requested.
    let ptime_now_ms = PTime::now().as_millis();
    let Ok(db) = runtime_db().lock() else {
        let payload =
            facet_json::to_vec(&SnapshotClientMessageRef::SnapshotReply(SnapshotReplyRef {
                snapshot_id,
                ptime_now_ms,
                snapshot: None,
            }))
            .map_err(|e| format!("encode snapshot reply json: {e}"))?;
        return moire_wire::encode_frame_default(&payload)
            .map_err(|e| format!("encode snapshot reply frame: {e}"));
    };

    let message = SnapshotClientMessageRef::SnapshotReply(SnapshotReplyRef {
        snapshot_id,
        ptime_now_ms,
        snapshot: Some(SnapshotRef {
            entities: db.entities.values().collect(),
            scopes: db.scopes.values().collect(),
            edges: db.edges.values().collect(),
            events: db.events.iter().collect(),
        }),
    });
    let payload =
        facet_json::to_vec(&message).map_err(|e| format!("encode snapshot reply json: {e}"))?;
    moire_wire::encode_frame_default(&payload)
        .map_err(|e| format!("encode snapshot reply frame: {e}"))
}
