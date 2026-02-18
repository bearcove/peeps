use compact_str::{CompactString, ToCompactString};
use facet::Facet;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{OnceLock, RwLock};
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
    CompactString::from(format!("{}:{}", location.file(), location.line()))
}

pub(crate) fn infer_krate_from_source(source: &str) -> Option<CompactString> {
    if let Ok(cache) = source_krate_cache().read() {
        if let Some(cached) = cache.get(source) {
            return cached.clone();
        }
    }

    let inferred = infer_krate_from_source_uncached(source);

    if let Ok(mut cache) = source_krate_cache().write() {
        cache.insert(CompactString::from(source), inferred.clone());
    }

    inferred
}

fn infer_krate_from_source_uncached(source: &str) -> Option<CompactString> {
    let file = source_file_path(source)?;
    let mut dir = file.parent()?.to_path_buf();

    loop {
        if let Some(cached) = cached_dir_krate(&dir) {
            return cached;
        }

        let inferred = infer_krate_from_dir(&dir);
        if let Ok(mut cache) = dir_krate_cache().write() {
            cache.insert(dir.clone(), inferred.clone());
        }
        if inferred.is_some() {
            return inferred;
        }

        if !dir.pop() {
            return None;
        }
    }
}

fn source_file_path(source: &str) -> Option<PathBuf> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }

    let path = match source.rsplit_once(':') {
        Some((path, line)) if line.parse::<u32>().is_ok() => Path::new(path),
        _ => Path::new(source),
    };

    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        std::env::current_dir().ok().map(|cwd| cwd.join(path))
    }
}

fn infer_krate_from_dir(dir: &Path) -> Option<CompactString> {
    let cargo_toml = dir.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return None;
    }
    cargo_package_name(&cargo_toml)
}

fn cargo_package_name(cargo_toml: &Path) -> Option<CompactString> {
    let contents = std::fs::read_to_string(cargo_toml).ok()?;
    let mut in_package = false;

    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "name" {
            continue;
        }

        let value = value.split('#').next().map(str::trim)?;
        let value = value.strip_prefix('"')?.strip_suffix('"')?;
        return Some(CompactString::from(value));
    }

    None
}

fn source_krate_cache() -> &'static RwLock<HashMap<CompactString, Option<CompactString>>> {
    static SOURCE_KRATE_CACHE: OnceLock<RwLock<HashMap<CompactString, Option<CompactString>>>> =
        OnceLock::new();
    SOURCE_KRATE_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn dir_krate_cache() -> &'static RwLock<HashMap<PathBuf, Option<CompactString>>> {
    static DIR_KRATE_CACHE: OnceLock<RwLock<HashMap<PathBuf, Option<CompactString>>>> =
        OnceLock::new();
    DIR_KRATE_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn cached_dir_krate(dir: &Path) -> Option<Option<CompactString>> {
    let cache = dir_krate_cache().read().ok()?;
    cache.get(dir).cloned()
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
    use super::{caller_source, infer_krate_from_source};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn caller_source_tracks_line() {
        #[track_caller]
        fn capture() -> compact_str::CompactString {
            caller_source()
        }
        let source = capture();
        let (_path, line) = source
            .rsplit_once(':')
            .expect("caller source should include :line");
        let line = line.parse::<u32>().expect("line segment should parse");
        assert!(line > 0, "line should be non-zero in source: {source}");
    }

    #[test]
    fn infer_krate_from_source_reads_package_name() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "peeps_types_krate_lookup_{}_{}",
            std::process::id(),
            nonce
        ));
        let src = root.join("src");
        std::fs::create_dir_all(&src).expect("should create temp source tree");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"peeps_types_krate_lookup\"\nversion = \"0.0.0\"\n",
        )
        .expect("should write Cargo.toml");
        let file = src.join("lib.rs");
        std::fs::write(&file, "pub fn hello() {}\n").expect("should write source file");

        let source = format!("{}:7", file.display());
        let inferred = infer_krate_from_source(&source);
        assert_eq!(inferred.as_deref(), Some("peeps_types_krate_lookup"));

        let _ = std::fs::remove_dir_all(root);
    }
}
