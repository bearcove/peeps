use facet::Facet;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantError {
    ZeroId(&'static str),
    EmptyField(&'static str),
    EmptyBacktraceFrames,
}

impl fmt::Display for InvariantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroId(field) => write!(f, "{field} must be non-zero"),
            Self::EmptyField(field) => write!(f, "{field} must be non-empty"),
            Self::EmptyBacktraceFrames => write!(f, "backtrace frames must be non-empty"),
        }
    }
}

impl Error for InvariantError {}

#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(u64);

impl ModuleId {
    pub fn new(value: u64) -> Result<Self, InvariantError> {
        if value == 0 {
            return Err(InvariantError::ZeroId("module_id"));
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
// r[impl model.backtrace]
pub struct BacktraceId(u64);

impl BacktraceId {
    pub fn new(value: u64) -> Result<Self, InvariantError> {
        if value == 0 {
            return Err(InvariantError::ZeroId("backtrace_id"));
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

#[cfg(feature = "rusqlite")]
impl rusqlite::types::ToSql for BacktraceId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok((self.0 as i64).into())
    }
}

#[cfg(feature = "rusqlite")]
impl rusqlite::types::FromSql for BacktraceId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let v = i64::column_result(value)?;
        BacktraceId::new(v as u64).map_err(|e| {
            rusqlite::types::FromSqlError::Other(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )))
        })
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

#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceCapabilities {
    pub trace_v1: bool,
    pub requires_frame_pointers: bool,
    pub sampling_supported: bool,
    pub alloc_tracking_supported: bool,
}
