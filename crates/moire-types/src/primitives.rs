use facet::Facet;
#[cfg(feature = "rusqlite")]
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[facet(rename_all = "snake_case")]
pub enum ChannelDir {
    Tx,
    Rx,
}

/// Raw JSON text payload used by request/response entities.
///
/// This wrapper preserves the exact JSON source string that was captured
/// at instrumentation boundaries.
#[derive(Facet, Clone, Debug, PartialEq, Eq)]
#[facet(transparent)]
pub struct Json(pub(crate) String);

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

#[cfg(feature = "rusqlite")]
impl ToSql for Json {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for Json {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(Json::new(String::column_result(value)?))
    }
}

/// First-use monotonic anchor for process-relative timestamps.
/// "Process birth" is defined as the first call to `PTime::now()`.
fn ptime_anchor() -> &'static Instant {
    static PTIME_ANCHOR: OnceLock<Instant> = OnceLock::new();
    PTIME_ANCHOR.get_or_init(Instant::now)
}

// r[impl model.ptime]
/// process start time + N milliseconds
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct PTime(u64);

impl PTime {
    pub fn now() -> Self {
        let elapsed_ms = ptime_anchor().elapsed().as_millis().min(u64::MAX as u128) as u64;
        Self(elapsed_ms)
    }

    pub fn as_millis(&self) -> u64 {
        self.0
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for PTime {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let as_i64 = i64::try_from(self.0).map_err(|_| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("PTime out of SQLite i64 range: {}", self.0),
            )))
        })?;
        Ok(as_i64.into())
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for PTime {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw_i64 = i64::column_result(value)?;
        let raw_u64 = u64::try_from(raw_i64).map_err(|_| FromSqlError::OutOfRange(raw_i64))?;
        Ok(PTime(raw_u64))
    }
}

/// Opaque textual entity identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct EntityId(pub(crate) String);

/// Opaque textual scope identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct ScopeId(pub(crate) String);

/// Opaque textual event identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct EventId(pub(crate) String);

#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct ConnectionId(pub(crate) u64);

#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct SessionId(pub(crate) String);

impl EntityId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl ScopeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl EventId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl ConnectionId {
    pub fn new(id: u64) -> Self {
        assert!(
            id > 0 && id <= moire_trace_types::JS_SAFE_INT_MAX_U64,
            "invariant violated: connection_id must be in 1..={}, got {id}",
            moire_trace_types::JS_SAFE_INT_MAX_U64
        );
        Self(id)
    }

    pub fn next(self) -> Self {
        Self::new(self.0.saturating_add(1))
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CONNECTION#{:x}", self.0)
    }
}

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn from_ordinal(value: u64) -> Self {
        Self(format!("session:{value}"))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for ConnectionId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let value = i64::try_from(self.0).map_err(|_| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("ConnectionId out of SQLite i64 range: {}", self.0),
            )))
        })?;
        Ok(value.into())
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for ConnectionId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw_i64 = i64::column_result(value)?;
        let raw_u64 = u64::try_from(raw_i64).map_err(|_| FromSqlError::OutOfRange(raw_i64))?;
        Ok(ConnectionId::new(raw_u64))
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for EntityId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for EntityId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(EntityId::new(String::column_result(value)?))
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for ScopeId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for ScopeId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(ScopeId::new(String::column_result(value)?))
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for EventId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for EventId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(EventId::new(String::column_result(value)?))
    }
}

pub(crate) fn next_entity_id() -> EntityId {
    EntityId(next_opaque_id())
}

pub(crate) fn next_scope_id() -> ScopeId {
    ScopeId(next_opaque_id())
}

pub(crate) fn next_event_id() -> EventId {
    EventId(next_opaque_id())
}

pub fn process_prefix_u16() -> u16 {
    static PROCESS_PREFIX: OnceLock<u16> = OnceLock::new();
    *PROCESS_PREFIX.get_or_init(|| {
        let pid = std::process::id() as u64;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        ((seed ^ pid) & 0xFFFF) as u16
    })
}

// r[impl model.id.uniqueness]
fn next_opaque_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);

    let prefix = process_prefix_u16();

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed) & 0x0000_FFFF_FFFF_FFFF;
    let raw = ((prefix as u64) << 48) | counter;
    MoireHex(raw).to_string()
}

// r[impl model.id.format]
/// `moire-hex` formatter:
/// lowercase hex with `a..f` remapped to `p,e,s,P,E,S`.
struct MoireHex(u64);

impl fmt::Display for MoireHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const DIGITS: &[u8; 16] = b"0123456789pesPES";
        let mut out = [0u8; 16];
        for (idx, shift) in (0..16).zip((0..64).step_by(4).rev()) {
            let nibble = ((self.0 >> shift) & 0xF) as usize;
            out[idx] = DIGITS[nibble];
        }
        // SAFETY: DIGITS only contains ASCII bytes.
        f.write_str(unsafe { std::str::from_utf8_unchecked(&out) })
    }
}
