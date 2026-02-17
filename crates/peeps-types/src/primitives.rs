use compact_str::{CompactString, ToCompactString};
use facet::Facet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub type MetaSerializeError = facet_format::SerializeError<facet_value::ToValueError>;

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

/// First-use monotonic anchor for process-relative timestamps.
/// "Process birth" is defined as the first call to `PTime::now()`.
fn ptime_anchor() -> &'static Instant {
    static PTIME_ANCHOR: OnceLock<Instant> = OnceLock::new();
    PTIME_ANCHOR.get_or_init(Instant::now)
}

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

/// Opaque textual entity identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct EntityId(pub(crate) CompactString);

/// Opaque textual scope identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct ScopeId(pub(crate) CompactString);

/// Opaque textual event identifier suitable for wire formats and JS runtimes.
#[derive(Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
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
    PeepsHex(raw).to_compact_string()
}

#[track_caller]
pub(crate) fn caller_source() -> CompactString {
    let location = std::panic::Location::caller();
    let file = absolute_source_path(location.file());
    CompactString::from(format!("{}:{}", file.display(), location.line()))
}

fn absolute_source_path(file: &str) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::path::absolute(path).unwrap_or_else(|_| {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    })
}

/// `peeps-hex-2` formatter:
/// lowercase hex with `a..f` remapped to `p,e,s,P,E,S`.
struct PeepsHex(u64);

impl fmt::Display for PeepsHex {
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

#[cfg(test)]
mod tests {
    use super::caller_source;
    use std::path::Path;

    #[test]
    fn caller_source_uses_absolute_path() {
        #[track_caller]
        fn capture() -> compact_str::CompactString {
            caller_source()
        }
        let source = capture();
        let (path, _line) = source
            .rsplit_once(':')
            .expect("caller source should include :line");
        assert!(
            Path::new(path).is_absolute(),
            "source path should be absolute: {source}"
        );
    }
}
