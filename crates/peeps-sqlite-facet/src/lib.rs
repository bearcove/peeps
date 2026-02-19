//! SQLite projection helpers for `peeps-types` snapshots.
//!
//! This crate intentionally handles the canonical four snapshot tables:
//! `entities`, `scopes`, `edges`, and `events`.
//!
//! Additional query-oriented relationship tables (for example entity<->scope
//! membership/link tables) should stay normalized at the SQLite layer instead
//! of being modeled as arrays in JSON fields.

use peeps_types::{
    Edge, Entity, EntityId, Event, EventId, PTime, Scope, ScopeId, Snapshot, SourceId,
};
use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, Value as SqlValue, ValueRef};
use std::fmt;

#[derive(Debug)]
pub enum EncodeError {
    Json(String),
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for EncodeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Json(String);

impl Json {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl ToSql for Json {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.0.as_str().into())
    }
}

impl FromSql for Json {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(Self(String::column_result(value)?))
    }
}

pub fn sqlite_value_ref_to_facet(value: ValueRef<'_>) -> facet_value::Value {
    match value {
        ValueRef::Null => facet_value::Value::NULL,
        ValueRef::Integer(v) => v.into(),
        ValueRef::Real(v) => v.into(),
        ValueRef::Text(bytes) => String::from_utf8_lossy(bytes).into_owned().into(),
        ValueRef::Blob(bytes) => bytes.to_vec().into(),
    }
}

pub fn sqlite_value_to_facet(value: SqlValue) -> facet_value::Value {
    match value {
        SqlValue::Null => facet_value::Value::NULL,
        SqlValue::Integer(v) => v.into(),
        SqlValue::Real(v) => v.into(),
        SqlValue::Text(v) => v.into(),
        SqlValue::Blob(v) => v.into(),
    }
}

pub fn row_to_facet_array(row: &rusqlite::Row<'_>) -> rusqlite::Result<facet_value::Value> {
    let mut out = Vec::new();
    for index in 0..row.as_ref().column_count() {
        let value = row.get_ref(index)?;
        out.push(sqlite_value_ref_to_facet(value));
    }
    Ok(out.into_iter().collect())
}

pub fn facet_to_json_text(value: &facet_value::Value) -> Result<String, String> {
    facet_json::to_string(value).map_err(|e| e.to_string())
}

pub fn json_text_to_facet(text: &str) -> Result<facet_value::Value, String> {
    facet_json::from_str(text).map_err(|e| e.to_string())
}

#[derive(Debug, Clone)]
pub struct EncodedEntityRow {
    pub id: EntityId,
    pub birth: PTime,
    pub source_id: SourceId,
    pub name: String,
    pub body_json: Json,
}

#[derive(Debug, Clone)]
pub struct EncodedScopeRow {
    pub id: ScopeId,
    pub birth: PTime,
    pub source_id: SourceId,
    pub name: String,
    pub body_json: Json,
}

#[derive(Debug, Clone)]
pub struct EncodedEdgeRow {
    pub src_id: EntityId,
    pub dst_id: EntityId,
    pub source_id: SourceId,
    pub kind_json: Json,
}

#[derive(Debug, Clone)]
pub struct EncodedEventRow {
    pub id: EventId,
    pub at: PTime,
    pub source_id: SourceId,
    pub target_json: Json,
    pub kind_json: Json,
}

#[derive(Debug, Clone, Default)]
pub struct EncodedSnapshotBatch {
    pub entities: Vec<EncodedEntityRow>,
    pub scopes: Vec<EncodedScopeRow>,
    pub edges: Vec<EncodedEdgeRow>,
    pub events: Vec<EncodedEventRow>,
}

pub fn encode_entity_row(entity: &Entity) -> Result<EncodedEntityRow, EncodeError> {
    Ok(EncodedEntityRow {
        id: EntityId::new(entity.id.as_str()),
        birth: entity.birth,
        source_id: entity.source,
        name: entity.name.clone(),
        body_json: Json::new(
            facet_json::to_string(&entity.body).map_err(|e| EncodeError::Json(e.to_string()))?,
        ),
    })
}

pub fn encode_scope_row(scope: &Scope) -> Result<EncodedScopeRow, EncodeError> {
    Ok(EncodedScopeRow {
        id: ScopeId::new(scope.id.as_str()),
        birth: scope.birth,
        source_id: scope.source,
        name: scope.name.clone(),
        body_json: Json::new(
            facet_json::to_string(&scope.body).map_err(|e| EncodeError::Json(e.to_string()))?,
        ),
    })
}

pub fn encode_edge_row(edge: &Edge) -> Result<EncodedEdgeRow, EncodeError> {
    Ok(EncodedEdgeRow {
        src_id: EntityId::new(edge.src.as_str()),
        dst_id: EntityId::new(edge.dst.as_str()),
        source_id: edge.source,
        kind_json: Json::new(
            facet_json::to_string(&edge.kind).map_err(|e| EncodeError::Json(e.to_string()))?,
        ),
    })
}

pub fn encode_event_row(event: &Event) -> Result<EncodedEventRow, EncodeError> {
    Ok(EncodedEventRow {
        id: EventId::new(event.id.as_str()),
        at: event.at,
        source_id: event.source,
        target_json: Json::new(
            facet_json::to_string(&event.target).map_err(|e| EncodeError::Json(e.to_string()))?,
        ),
        kind_json: Json::new(
            facet_json::to_string(&event.kind).map_err(|e| EncodeError::Json(e.to_string()))?,
        ),
    })
}

pub fn encode_snapshot_batch(snapshot: &Snapshot) -> Result<EncodedSnapshotBatch, EncodeError> {
    let mut out = EncodedSnapshotBatch::default();

    for entity in &snapshot.entities {
        out.entities.push(encode_entity_row(entity)?);
    }
    for scope in &snapshot.scopes {
        out.scopes.push(encode_scope_row(scope)?);
    }
    for edge in &snapshot.edges {
        out.edges.push(encode_edge_row(edge)?);
    }
    for event in &snapshot.events {
        out.events.push(encode_event_row(event)?);
    }

    Ok(out)
}

#[derive(Debug, Clone)]
pub struct SnapshotTableNames {
    // Core snapshot projection tables.
    // Keep this focused on canonical model rows; relationship/index helper
    // tables (entity_scope_links, etc.) are managed separately by callers.
    pub entities: String,
    pub scopes: String,
    pub edges: String,
    pub events: String,
}

impl Default for SnapshotTableNames {
    fn default() -> Self {
        Self {
            entities: String::from("entities"),
            scopes: String::from("scopes"),
            edges: String::from("edges"),
            events: String::from("events"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertMode {
    Insert,
    InsertOrReplace,
}

impl InsertMode {
    fn verb(self) -> &'static str {
        match self {
            Self::Insert => "INSERT INTO",
            Self::InsertOrReplace => "INSERT OR REPLACE INTO",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct InsertCounts {
    pub entities: usize,
    pub scopes: usize,
    pub edges: usize,
    pub events: usize,
}

pub fn insert_encoded_snapshot_batch(
    conn: &mut rusqlite::Connection,
    snapshot_id: i64,
    batch: &EncodedSnapshotBatch,
    tables: &SnapshotTableNames,
    mode: InsertMode,
) -> rusqlite::Result<InsertCounts> {
    let tx = conn.transaction()?;
    let mut counts = InsertCounts::default();

    let entities_sql = format!(
        "{} [{}] (snapshot_id, id, birth_ms, source_id, name, body_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        mode.verb(),
        tables.entities.as_str()
    );
    let scopes_sql = format!(
        "{} [{}] (snapshot_id, id, birth_ms, source_id, name, body_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        mode.verb(),
        tables.scopes.as_str()
    );
    let edges_sql = format!(
        "{} [{}] (snapshot_id, src_id, dst_id, source_id, kind_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        mode.verb(),
        tables.edges.as_str()
    );
    let events_sql = format!(
        "{} [{}] (snapshot_id, id, at_ms, source_id, target_json, kind_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        mode.verb(),
        tables.events.as_str()
    );

    {
        let mut stmt = tx.prepare_cached(&entities_sql)?;
        for row in &batch.entities {
            stmt.execute(rusqlite::params![
                snapshot_id,
                row.id,
                row.birth,
                row.source_id,
                row.name.as_str(),
                row.body_json,
            ])?;
            counts.entities += 1;
        }
    }

    {
        let mut stmt = tx.prepare_cached(&scopes_sql)?;
        for row in &batch.scopes {
            stmt.execute(rusqlite::params![
                snapshot_id,
                row.id,
                row.birth,
                row.source_id,
                row.name.as_str(),
                row.body_json,
            ])?;
            counts.scopes += 1;
        }
    }

    {
        let mut stmt = tx.prepare_cached(&edges_sql)?;
        for row in &batch.edges {
            stmt.execute(rusqlite::params![
                snapshot_id,
                row.src_id,
                row.dst_id,
                row.source_id,
                row.kind_json,
            ])?;
            counts.edges += 1;
        }
    }

    {
        let mut stmt = tx.prepare_cached(&events_sql)?;
        for row in &batch.events {
            stmt.execute(rusqlite::params![
                snapshot_id,
                row.id,
                row.at,
                row.source_id,
                row.target_json,
                row.kind_json,
            ])?;
            counts.events += 1;
        }
    }

    tx.commit()?;
    Ok(counts)
}

pub fn insert_encoded_snapshot_batch_default(
    conn: &mut rusqlite::Connection,
    snapshot_id: i64,
    batch: &EncodedSnapshotBatch,
) -> rusqlite::Result<InsertCounts> {
    insert_encoded_snapshot_batch(
        conn,
        snapshot_id,
        batch,
        &SnapshotTableNames::default(),
        InsertMode::InsertOrReplace,
    )
}
