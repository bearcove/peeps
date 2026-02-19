use facet::Facet;

use crate::{next_scope_id, PTime, ScopeId, SourceId};

/// A scope groups execution context over time (for example process/thread/task/connection).
#[derive(Facet)]
pub struct Scope {
    /// Opaque scope identifier.
    pub id: ScopeId,

    /// When we first started tracking this scope.
    pub birth: PTime,

    /// Location in source code and crate information.
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
    Process(ProcessScopeBody),
    Thread(ThreadScopeBody),
    Task(TaskScopeBody),
    Connection(ConnectionScopeBody),
}

#[derive(Facet)]
pub struct ProcessScopeBody {
    pub pid: u32,
}

#[derive(Facet)]
pub struct ThreadScopeBody {
    pub thread_name: Option<String>,
}

#[derive(Facet)]
pub struct TaskScopeBody {
    pub task_key: String,
}

#[derive(Facet)]
pub struct ConnectionScopeBody {
    pub local_addr: Option<String>,
    pub peer_addr: Option<String>,
}

crate::impl_sqlite_json!(ScopeBody);

crate::declare_scope_body_slots!(
    ProcessScopeSlot::Process(ProcessScopeBody),
    ThreadScopeSlot::Thread(ThreadScopeBody),
    TaskScopeSlot::Task(TaskScopeBody),
    ConnectionScopeSlot::Connection(ConnectionScopeBody),
);
