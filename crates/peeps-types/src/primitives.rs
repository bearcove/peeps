use compact_str::{CompactString, ToCompactString};
use facet::Facet;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub type MetaSerializeError = facet_format::SerializeError<facet_value::ToValueError>;

/// First-use monotonic anchor for process-relative timestamps.
/// "Process birth" is defined as the first call to `PTime::now()`.
fn ptime_anchor() -> &'static Instant {
    static PTIME_ANCHOR: OnceLock<Instant> = OnceLock::new();
    PTIME_ANCHOR.get_or_init(Instant::now)
}

/// process start time + N milliseconds
#[derive(Facet)]
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

/// Opaque textual entity identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub(crate) CompactString);

/// Opaque textual scope identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(pub(crate) CompactString);

/// Opaque textual event identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(pub(crate) CompactString);

impl EntityId {
    pub fn new(id: impl Into<CompactString>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl ScopeId {
    pub fn new(id: impl Into<CompactString>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl EventId {
    pub fn new(id: impl Into<CompactString>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
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

fn next_opaque_id() -> CompactString {
    static PROCESS_PREFIX: OnceLock<u16> = OnceLock::new();
    static COUNTER: AtomicU64 = AtomicU64::new(1);

    let prefix = *PROCESS_PREFIX.get_or_init(|| {
        let pid = std::process::id() as u64;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        ((seed ^ pid) & 0xFFFF) as u16
    });

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed) & 0x0000_FFFF_FFFF_FFFF;
    let raw = ((prefix as u64) << 48) | counter;
    PeepsHex2(raw).to_compact_string()
}

#[track_caller]
pub(crate) fn caller_source() -> CompactString {
    let location = std::panic::Location::caller();
    CompactString::from(format!("{}:{}", location.file(), location.line()))
}

/// `peeps-hex-2` formatter:
/// lowercase hex with `a..f` remapped to `p,e,s,P,E,S`.
struct PeepsHex2(u64);

impl fmt::Display for PeepsHex2 {
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
