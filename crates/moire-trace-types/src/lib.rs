use facet::Facet;
use std::error::Error;
use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantError {
    ZeroId(&'static str),
    IdOutOfRange {
        field: &'static str,
        max: u64,
        got: u64,
    },
    EmptyField(&'static str),
    EmptyBacktraceFrames,
}

impl fmt::Display for InvariantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroId(field) => write!(f, "{field} must be non-zero"),
            Self::IdOutOfRange { field, max, got } => {
                write!(f, "{field} must be <= {max}, got {got}")
            }
            Self::EmptyField(field) => write!(f, "{field} must be non-empty"),
            Self::EmptyBacktraceFrames => write!(f, "backtrace frames must be non-empty"),
        }
    }
}

impl Error for InvariantError {}

pub const ID_PREFIX_BITS: u32 = 16;
pub const ID_COUNTER_BITS: u32 = 37;
pub const ID_COUNTER_MAX_U64: u64 = (1u64 << ID_COUNTER_BITS) - 1;
pub const JS_SAFE_INT_MAX_U64: u64 = (1u64 << 53) - 1;

fn process_prefix_u16() -> u16 {
    static PROCESS_PREFIX: OnceLock<u16> = OnceLock::new();
    *PROCESS_PREFIX.get_or_init(|| {
        let pid = std::process::id() as u64;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0);
        ((seed ^ pid) & 0xFFFF) as u16
    })
}

macro_rules! define_u64_id {
    (
        $(#[$meta:meta])*
        $name:ident,
        field = $field:literal
        , max = $max:expr
    ) => {
        #[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[facet(transparent)]
        $(#[$meta])*
        pub struct $name(u64);

        impl $name {
            fn from_raw(value: u64) -> Result<Self, InvariantError> {
                if value == 0 {
                    return Err(InvariantError::ZeroId($field));
                }
                if value > $max {
                    return Err(InvariantError::IdOutOfRange {
                        field: $field,
                        max: $max,
                        got: value,
                    });
                }
                Ok(Self(value))
            }

            #[cfg(test)]
            #[allow(dead_code)]
            fn from_prefixed_counter(prefix: u16, counter: u64) -> Result<Self, InvariantError> {
                if counter > ID_COUNTER_MAX_U64 {
                    return Err(InvariantError::IdOutOfRange {
                        field: $field,
                        max: ID_COUNTER_MAX_U64,
                        got: counter,
                    });
                }
                let raw = ((u64::from(prefix)) << ID_COUNTER_BITS) | counter;
                Self::from_raw(raw)
            }

            pub fn next() -> Result<Self, InvariantError> {
                static NEXT_COUNTER: AtomicU64 = AtomicU64::new(1);
                let counter = NEXT_COUNTER.fetch_add(1, Ordering::Relaxed);
                if counter > ID_COUNTER_MAX_U64 {
                    return Err(InvariantError::IdOutOfRange {
                        field: $field,
                        max: ID_COUNTER_MAX_U64,
                        got: counter,
                    });
                }
                let prefix = process_prefix_u16();
                let raw = ((u64::from(prefix)) << ID_COUNTER_BITS) | counter;
                Self::from_raw(raw)
            }

            pub fn get(self) -> u64 {
                self.0
            }

            pub fn process_prefix(self) -> u16 {
                (self.0 >> ID_COUNTER_BITS) as u16
            }

            pub fn counter(self) -> u64 {
                self.0 & ID_COUNTER_MAX_U64
            }
        }

        #[cfg(feature = "rusqlite")]
        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                let value = i64::try_from(self.0).map_err(|error| {
                    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{} does not fit i64: {error}", $field),
                    )))
                })?;
                Ok(value.into())
            }
        }

        #[cfg(feature = "rusqlite")]
        impl rusqlite::types::FromSql for $name {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                let value = i64::column_result(value)?;
                let value = u64::try_from(value).map_err(|error| {
                    rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("{field} must be non-negative i64: {error}", field = $field),
                    )))
                })?;
                $name::from_raw(value).map_err(|error| {
                    rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        error.to_string(),
                    )))
                })
            }
        }
    };
}

define_u64_id!(ModuleId, field = "module_id", max = JS_SAFE_INT_MAX_U64);
define_u64_id!(
    // r[impl model.backtrace]
    BacktraceId,
    field = "backtrace_id",
    max = JS_SAFE_INT_MAX_U64
);
define_u64_id!(FrameId, field = "frame_id", max = JS_SAFE_INT_MAX_U64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backtrace_id_next_is_prefixed_and_js_safe() {
        let first = BacktraceId::next().expect("first id must be valid");
        let second = BacktraceId::next().expect("second id must be valid");

        assert!(first.get() > 0, "id must be non-zero");
        assert!(
            second.get() > first.get(),
            "ids must be monotonic within a process"
        );
        assert!(
            first.get() <= JS_SAFE_INT_MAX_U64 && second.get() <= JS_SAFE_INT_MAX_U64,
            "ids must be JS-safe"
        );
        assert_eq!(
            first.process_prefix(),
            second.process_prefix(),
            "ids from same process must share prefix"
        );
    }
}

#[derive(Facet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModulePath(String);

impl ModulePath {
    pub fn new(value: impl Into<String>) -> Result<Self, InvariantError> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvariantError::EmptyField("module_path"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Facet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildId(String);

impl BuildId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvariantError> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvariantError::EmptyField("build_id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Facet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DebugId(String);

impl DebugId {
    pub fn new(value: impl Into<String>) -> Result<Self, InvariantError> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvariantError::EmptyField("debug_id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Facet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleArch(String);

impl ModuleArch {
    pub fn new(value: impl Into<String>) -> Result<Self, InvariantError> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvariantError::EmptyField("arch"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Facet, Debug, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ModuleIdentity {
    BuildId(BuildId),
    DebugId(DebugId),
}

#[derive(Facet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FrameKey {
    pub module_id: ModuleId,
    pub rel_pc: u64,
}

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct BacktraceRecord {
    pub id: BacktraceId,
    pub frames: Vec<FrameKey>,
}

impl BacktraceRecord {
    pub fn new(id: BacktraceId, frames: Vec<FrameKey>) -> Result<Self, InvariantError> {
        if frames.is_empty() {
            return Err(InvariantError::EmptyBacktraceFrames);
        }
        Ok(Self { id, frames })
    }
}

#[derive(Facet, Debug, Clone, PartialEq, Eq)]
pub struct ModuleRecord {
    pub id: ModuleId,
    pub path: ModulePath,
    pub runtime_base: u64,
    pub identity: ModuleIdentity,
    pub arch: ModuleArch,
}
