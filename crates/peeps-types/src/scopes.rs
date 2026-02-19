use compact_str::CompactString;
use facet::Facet;
use facet_value::Value;

use crate::{caller_source, next_scope_id, MetaSerializeError, PTime, ScopeId};

/// A scope groups execution context over time (for example process/thread/task/connection).
#[derive(Facet)]
pub struct Scope {
    /// Opaque scope identifier.
    pub id: ScopeId,

    /// When we first started tracking this scope.
    pub birth: PTime,

    /// Creation/discovery site in source code as `{path}:{line}`.
    pub source: CompactString,

    /// Rust crate that created this scope, if known.
    /// Populated explicitly by macros when available, otherwise inferred from `source`
    /// by walking to the nearest `Cargo.toml` at runtime.
    pub krate: Option<CompactString>,

    /// Human-facing name for this scope.
    pub name: CompactString,

    /// More specific info about the scope.
    pub body: ScopeBody,

    /// Extensible metadata for optional, non-canonical context.
    pub meta: Value,
}

impl Scope {
    /// Starts building a scope from required semantic fields.
    pub fn builder(name: impl Into<CompactString>, body: ScopeBody) -> ScopeBuilder {
        ScopeBuilder {
            name: name.into(),
            body,
            source: None,
            krate: None,
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

/// Builder for `Scope` that auto-fills runtime identity and creation metadata.
pub struct ScopeBuilder {
    name: CompactString,
    body: ScopeBody,
    source: Option<CompactString>,
    krate: Option<CompactString>,
}

impl ScopeBuilder {
    pub fn source(mut self, source: impl Into<CompactString>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn krate(mut self, krate: impl Into<CompactString>) -> Self {
        self.krate = Some(krate.into());
        self
    }

    /// Finalizes the scope with typed meta converted into `facet_value::Value`.
    #[track_caller]
    pub fn build<M>(self, meta: &M) -> Result<Scope, MetaSerializeError>
    where
        M: for<'facet> Facet<'facet>,
    {
        let source = self.source.unwrap_or_else(caller_source);
        let krate = self.krate;

        Ok(Scope {
            id: next_scope_id(),
            birth: PTime::now(),
            source,
            krate,
            name: self.name,
            body: self.body,
            meta: facet_value::to_value(meta)?,
        })
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
