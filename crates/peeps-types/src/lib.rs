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

#[macro_export]
macro_rules! impl_sqlite_json {
    ($ty:ty) => {
        #[cfg(feature = "rusqlite")]
        impl ::rusqlite::types::ToSql for $ty {
            fn to_sql(&self) -> ::rusqlite::Result<::rusqlite::types::ToSqlOutput<'_>> {
                let json = ::facet_json::to_string(self).map_err(|err| {
                    ::rusqlite::Error::ToSqlConversionFailure(Box::new(::std::io::Error::new(
                        ::std::io::ErrorKind::InvalidData,
                        err.to_string(),
                    )))
                })?;
                Ok(json.into())
            }
        }

        #[cfg(feature = "rusqlite")]
        impl ::rusqlite::types::FromSql for $ty {
            fn column_result(
                value: ::rusqlite::types::ValueRef<'_>,
            ) -> ::rusqlite::types::FromSqlResult<Self> {
                let json = <String as ::rusqlite::types::FromSql>::column_result(value)?;
                ::facet_json::from_str(&json).map_err(|err| {
                    ::rusqlite::types::FromSqlError::Other(Box::new(::std::io::Error::new(
                        ::std::io::ErrorKind::InvalidData,
                        err.to_string(),
                    )))
                })
            }
        }
    };
}

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
