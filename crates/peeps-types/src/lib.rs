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

#[macro_export]
macro_rules! impl_entity_body_slot {
    ($slot:ident :: $variant:ident ($value:ty)) => {
        impl $crate::EntityBodySlot for $slot {
            type Value = $value;
            const KIND_NAME: &'static str = stringify!($variant);

            fn project(body: &$crate::EntityBody) -> Option<&Self::Value> {
                if let $crate::EntityBody::$variant(value) = body {
                    Some(value)
                } else {
                    None
                }
            }

            fn project_mut(body: &mut $crate::EntityBody) -> Option<&mut Self::Value> {
                if let $crate::EntityBody::$variant(value) = body {
                    Some(value)
                } else {
                    None
                }
            }
        }
    };
    ($slot:ty, $value:ty, $variant:ident, $kind_name:expr) => {
        impl $crate::EntityBodySlot for $slot {
            type Value = $value;
            const KIND_NAME: &'static str = $kind_name;

            fn project(body: &$crate::EntityBody) -> Option<&Self::Value> {
                if let $crate::EntityBody::$variant(value) = body {
                    Some(value)
                } else {
                    None
                }
            }

            fn project_mut(body: &mut $crate::EntityBody) -> Option<&mut Self::Value> {
                if let $crate::EntityBody::$variant(value) = body {
                    Some(value)
                } else {
                    None
                }
            }
        }
    };
}

#[macro_export]
macro_rules! define_entity_body {
    (
        $vis:vis enum EntityBody {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident($value:ty)
            ),+ $(,)?
        }
    ) => {
        #[derive(::facet::Facet)]
        #[repr(u8)]
        #[facet(rename_all = "snake_case")]
        #[allow(dead_code)]
        $vis enum EntityBody {
            $(
                $(#[$variant_meta])*
                $variant($value),
            )+
        }

        impl EntityBody {
            pub fn kind_name(&self) -> &'static str {
                match self {
                    $(
                        Self::$variant(_) => stringify!($variant),
                    )+
                }
            }
        }

        $crate::impl_sqlite_json!(EntityBody);

        $(
            pub struct $variant;
            $crate::impl_entity_body_slot!($variant::$variant($value));
        )+
    };
}

#[macro_export]
macro_rules! declare_entity_body_slots {
    ($( $slot:ident :: $variant:ident ($value:ty) ),+ $(,)?) => {
        $(
            pub struct $slot;
            $crate::impl_entity_body_slot!($slot::$variant($value));
        )+
    };
}

#[macro_export]
macro_rules! impl_scope_body_slot {
    ($slot:ident :: $variant:ident ($value:ty)) => {
        impl $crate::ScopeBodySlot for $slot {
            type Value = $value;
            const KIND_NAME: &'static str = stringify!($variant);

            fn project(body: &$crate::ScopeBody) -> Option<&Self::Value> {
                if let $crate::ScopeBody::$variant(value) = body {
                    Some(value)
                } else {
                    None
                }
            }

            fn project_mut(body: &mut $crate::ScopeBody) -> Option<&mut Self::Value> {
                if let $crate::ScopeBody::$variant(value) = body {
                    Some(value)
                } else {
                    None
                }
            }
        }
    };
    ($slot:ty, $value:ty, $variant:ident, $kind_name:expr) => {
        impl $crate::ScopeBodySlot for $slot {
            type Value = $value;
            const KIND_NAME: &'static str = $kind_name;

            fn project(body: &$crate::ScopeBody) -> Option<&Self::Value> {
                if let $crate::ScopeBody::$variant(value) = body {
                    Some(value)
                } else {
                    None
                }
            }

            fn project_mut(body: &mut $crate::ScopeBody) -> Option<&mut Self::Value> {
                if let $crate::ScopeBody::$variant(value) = body {
                    Some(value)
                } else {
                    None
                }
            }
        }
    };
}

#[macro_export]
macro_rules! declare_scope_body_slots {
    ($( $slot:ident :: $variant:ident ($value:ty) ),+ $(,)?) => {
        $(
            pub struct $slot;
            $crate::impl_scope_body_slot!($slot::$variant($value));
        )+
    };
}

#[macro_export]
macro_rules! impl_event_target_slot {
    ($slot:ident :: $variant:ident ($value:ty)) => {
        impl $crate::EventTargetSlot for $slot {
            type Value = $value;
            const KIND_NAME: &'static str = stringify!($variant);

            fn project(target: &$crate::EventTarget) -> Option<&Self::Value> {
                if let $crate::EventTarget::$variant(value) = target {
                    Some(value)
                } else {
                    None
                }
            }

            fn project_mut(target: &mut $crate::EventTarget) -> Option<&mut Self::Value> {
                if let $crate::EventTarget::$variant(value) = target {
                    Some(value)
                } else {
                    None
                }
            }
        }
    };
    ($slot:ty, $value:ty, $variant:ident, $kind_name:expr) => {
        impl $crate::EventTargetSlot for $slot {
            type Value = $value;
            const KIND_NAME: &'static str = $kind_name;

            fn project(target: &$crate::EventTarget) -> Option<&Self::Value> {
                if let $crate::EventTarget::$variant(value) = target {
                    Some(value)
                } else {
                    None
                }
            }

            fn project_mut(target: &mut $crate::EventTarget) -> Option<&mut Self::Value> {
                if let $crate::EventTarget::$variant(value) = target {
                    Some(value)
                } else {
                    None
                }
            }
        }
    };
}

#[macro_export]
macro_rules! declare_event_target_slots {
    ($( $slot:ident :: $variant:ident ($value:ty) ),+ $(,)?) => {
        $(
            pub struct $slot;
            $crate::impl_event_target_slot!($slot::$variant($value));
        )+
    };
}

#[macro_export]
macro_rules! impl_event_kind_slot {
    ($slot:ident :: $variant:ident) => {
        impl $crate::EventKindSlot for $slot {
            const KIND: $crate::EventKind = $crate::EventKind::$variant;
            const KIND_NAME: &'static str = stringify!($variant);
        }
    };
    ($slot:ty, $variant:ident, $kind_name:expr) => {
        impl $crate::EventKindSlot for $slot {
            const KIND: $crate::EventKind = $crate::EventKind::$variant;
            const KIND_NAME: &'static str = $kind_name;
        }
    };
}

#[macro_export]
macro_rules! declare_event_kind_slots {
    ($( $slot:ident :: $variant:ident ),+ $(,)?) => {
        $(
            pub struct $slot;
            $crate::impl_event_kind_slot!($slot::$variant);
        )+
    };
}

#[macro_export]
macro_rules! impl_edge_kind_slot {
    ($slot:ident :: $variant:ident) => {
        impl $crate::EdgeKindSlot for $slot {
            const KIND: $crate::EdgeKind = $crate::EdgeKind::$variant;
            const KIND_NAME: &'static str = stringify!($variant);
        }
    };
    ($slot:ty, $variant:ident, $kind_name:expr) => {
        impl $crate::EdgeKindSlot for $slot {
            const KIND: $crate::EdgeKind = $crate::EdgeKind::$variant;
            const KIND_NAME: &'static str = $kind_name;
        }
    };
}

#[macro_export]
macro_rules! declare_edge_kind_slots {
    ($( $slot:ident :: $variant:ident ),+ $(,)?) => {
        $(
            pub struct $slot;
            $crate::impl_edge_kind_slot!($slot::$variant);
        )+
    };
}

pub(crate) mod api;
pub(crate) mod diff;
pub(crate) mod objects;
pub(crate) mod primitives;
pub(crate) mod recording;
pub(crate) mod snapshots;
pub(crate) mod sources;

pub use api::*;
pub use diff::*;
pub use objects::*;
pub use primitives::*;
pub use recording::*;
pub use snapshots::*;
pub use sources::*;
