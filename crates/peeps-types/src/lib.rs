//! Core graph nomenclature used across Peep's runtime model.
//!
//! - `Event`: a point-in-time occurrence with a timestamp.
//! - `Entity`: a runtime thing that exists over time (for example a lock,
//!   future, channel, request, or connection).
//! - `Edge`: a relationship between entities (causal or structural).
//! - `Scope`: an execution container that groups entities (for example a
//!   process, thread, or task).
//!
//! In short: events happen to entities, entities are connected by edges,
//! and entities live inside scopes.

pub(crate) mod diff;
pub(crate) mod edges;
pub(crate) mod entities;
pub(crate) mod primitives;
pub(crate) mod recording;
pub(crate) mod scopes;
pub(crate) mod snapshots;
pub(crate) mod sources;

pub use diff::*;
pub use edges::*;
pub use entities::*;
pub use primitives::*;
pub use recording::*;
pub use scopes::*;
pub use snapshots::*;
pub use sources::*;
