use facet::Facet;

use crate::{next_scope_id, PTime, ScopeId, SourceId};

/// A scope groups execution context over time (for example process/thread/task/connection).
#[derive(Facet)]
pub struct Scope {
    /// Opaque scope identifier.
    pub id: ScopeId,

    /// When we first started tracking this scope.
    pub birth: PTime,

    /// Interned source identifier.
    ///
    /// Resolves to a `{source, krate}` tuple in the source registry.
    pub source: SourceId,

    /// Human-facing name for this scope.
    pub name: String,

    /// More specific info about the scope.
    pub body: ScopeBody,
}

impl Scope {
    /// Create a new scope: ID and birth time are generated automatically.
    pub fn new(source: impl Into<SourceId>, name: impl Into<String>, body: ScopeBody) -> Scope {
        Scope {
            id: next_scope_id(),
            birth: PTime::now(),
            source: source.into(),
            name: name.into(),
            body,
        }
    }
}

#[derive(Facet)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ScopeBody {
    Process,
    Thread,
    Task,
    Connection,
}

crate::impl_sqlite_json!(ScopeBody);
