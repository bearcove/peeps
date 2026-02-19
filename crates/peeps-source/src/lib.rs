#![doc = include_str!("README.md")]

use compact_str::CompactString;
use facet::Facet;
#[cfg(feature = "rusqlite")]
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::collections::BTreeMap;
use std::panic::Location;
use std::path::Path;
use std::sync::{Mutex as StdMutex, OnceLock};

#[derive(Clone, Copy, Debug)]
pub struct SourceRight {
    location: &'static Location<'static>,
}

impl SourceRight {
    #[track_caller]
    pub fn caller() -> Self {
        Self {
            location: Location::caller(),
        }
    }

    #[cfg(test)]
    pub const fn from_location(location: &'static Location<'static>) -> Self {
        Self { location }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SourceLeft {
    manifest_dir: &'static str,
}

impl SourceLeft {
    pub const fn new(manifest_dir: &'static str) -> Self {
        Self { manifest_dir }
    }

    pub const fn manifest_dir(self) -> &'static str {
        self.manifest_dir
    }

    #[track_caller]
    pub fn resolve(self) -> Source {
        self.join(SourceRight::caller())
    }

    pub fn join(self, right: SourceRight) -> Source {
        Source::resolve(self, right)
    }
}

/// A fully resolved source code location, including create information that identifies where things
/// are being done, where futures are being polled, where locks are being awaited on, etc.
#[derive(Clone, Debug)]
pub struct Source {
    source: CompactString,
    krate: Option<CompactString>,
}

impl Source {
    pub fn new(source: impl Into<CompactString>, krate: Option<CompactString>) -> Self {
        Self {
            source: source.into(),
            krate,
        }
    }

    pub fn resolve(left: SourceLeft, right: SourceRight) -> Self {
        Self::resolve_parts(
            left.manifest_dir(),
            right.location.file(),
            right.location.line(),
        )
    }

    fn resolve_parts(manifest_dir: &str, file: &str, line: u32) -> Self {
        let file = Path::new(file);
        let resolved = if file.is_absolute() {
            file.to_path_buf()
        } else {
            Path::new(manifest_dir).join(file)
        };
        let source = CompactString::from(format!("{}:{}", resolved.display(), line));
        let krate = infer_crate_name_from_manifest_dir(manifest_dir);
        Self { source, krate }
    }

    pub fn as_str(&self) -> &str {
        self.source.as_str()
    }

    pub fn krate(&self) -> Option<&str> {
        self.krate.as_ref().map(|k| k.as_str())
    }

    pub fn into_compact_string(self) -> CompactString {
        self.source
    }
}

/// A JSON-safe (U53) interned source identifier.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[facet(transparent)]
pub struct SourceId(u64);

impl SourceId {
    pub const MAX_U53: u64 = (1u64 << 53) - 1;

    pub fn new(raw: u64) -> Self {
        assert!(
            raw <= Self::MAX_U53,
            "SourceId out of JSON-safe U53 range: {raw}"
        );
        Self(raw)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

#[cfg(feature = "rusqlite")]
impl ToSql for SourceId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok((self.0 as i64).into())
    }
}

#[cfg(feature = "rusqlite")]
impl FromSql for SourceId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let raw_i64 = i64::column_result(value)?;
        let raw_u64 = u64::try_from(raw_i64).map_err(|_| FromSqlError::OutOfRange(raw_i64))?;
        if raw_u64 > SourceId::MAX_U53 {
            return Err(FromSqlError::OutOfRange(raw_i64));
        }
        Ok(SourceId(raw_u64))
    }
}

pub fn intern_source(source: Source) -> SourceId {
    static SOURCE_INTERN: OnceLock<StdMutex<SourceIntern>> = OnceLock::new();
    let lock = SOURCE_INTERN.get_or_init(|| StdMutex::new(SourceIntern::new()));
    let mut intern = lock
        .lock()
        .expect("source intern mutex poisoned; cannot continue");
    intern.intern(source)
}

pub fn source_for_id(source_id: SourceId) -> Option<Source> {
    static SOURCE_INTERN: OnceLock<StdMutex<SourceIntern>> = OnceLock::new();
    let lock = SOURCE_INTERN.get_or_init(|| StdMutex::new(SourceIntern::new()));
    let intern = lock
        .lock()
        .expect("source intern mutex poisoned; cannot continue");
    intern.lookup(source_id)
}

impl From<Source> for SourceId {
    fn from(source: Source) -> Self {
        intern_source(source)
    }
}

impl From<SourceRight> for Source {
    fn from(right: SourceRight) -> Self {
        panic!(
            "invalid Source conversion: SourceRight ({}:{}) cannot be used without SourceLeft; join explicitly via SourceLeft::join(SourceRight)",
            right.location.file(),
            right.location.line()
        );
    }
}

fn infer_crate_name_from_manifest_dir(manifest_dir: &str) -> Option<CompactString> {
    let manifest_path = Path::new(manifest_dir).join("Cargo.toml");
    let content = std::fs::read_to_string(manifest_path).ok()?;
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package = trimmed == "[package]";
            continue;
        }
        if !in_package || !trimmed.starts_with("name") {
            continue;
        }
        let (_, raw_value) = trimmed.split_once('=')?;
        let value = raw_value.trim();
        if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
            return None;
        }
        return Some(CompactString::from(&value[1..value.len() - 1]));
    }
    None
}

struct SourceIntern {
    next_id: u64,
    by_key: BTreeMap<(CompactString, Option<CompactString>), SourceId>,
    by_id: BTreeMap<SourceId, Source>,
}

impl SourceIntern {
    fn new() -> Self {
        Self {
            next_id: 1,
            by_key: BTreeMap::new(),
            by_id: BTreeMap::new(),
        }
    }

    fn intern(&mut self, source: Source) -> SourceId {
        let key = (
            CompactString::from(source.source.as_str()),
            source
                .krate
                .as_ref()
                .map(|k| CompactString::from(k.as_str())),
        );
        if let Some(existing) = self.by_key.get(&key).copied() {
            return existing;
        }

        let id = SourceId::new(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("source id counter overflow");
        assert!(
            self.next_id <= SourceId::MAX_U53 + 1,
            "source id counter exceeded JSON-safe U53 range"
        );

        self.by_key.insert(key, id);
        self.by_id.insert(id, source);
        id
    }

    fn lookup(&self, id: SourceId) -> Option<Source> {
        self.by_id.get(&id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolves_relative_path_from_manifest_dir() {
        let source = Source::resolve_parts("/repo/crate", "src/lib.rs", 42);
        let expected = PathBuf::from("/repo/crate").join("src/lib.rs");
        assert_eq!(source.as_str(), format!("{}:42", expected.display()));
    }

    #[test]
    fn preserves_absolute_path() {
        let source = Source::resolve_parts("/repo/crate", "/other/place/main.rs", 7);
        assert_eq!(source.as_str(), "/other/place/main.rs:7");
    }

    #[test]
    fn infers_crate_name_from_manifest() {
        let base = std::env::temp_dir().join(format!(
            "peeps-source-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).expect("failed to create temp manifest dir");
        std::fs::write(
            base.join("Cargo.toml"),
            "[package]\nname = \"source-left-right-test\"\nversion = \"0.1.0\"\n",
        )
        .expect("failed to write Cargo.toml");

        let source = Source::resolve_parts(
            base.to_str().expect("temp path must be valid utf-8"),
            "src/lib.rs",
            1,
        );

        assert_eq!(source.krate(), Some("source-left-right-test"));
        std::fs::remove_dir_all(base).expect("failed to cleanup temp manifest dir");
    }

    #[test]
    fn crate_name_is_none_when_manifest_missing() {
        let base = std::env::temp_dir().join(format!(
            "peeps-source-test-missing-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).expect("failed to create temp dir");

        let source = Source::resolve_parts(
            base.to_str().expect("temp path must be valid utf-8"),
            "src/lib.rs",
            1,
        );

        assert_eq!(source.krate(), None);
        std::fs::remove_dir_all(base).expect("failed to cleanup temp dir");
    }

    #[test]
    fn intern_round_trips_source() {
        let source = Source::new("/repo/src/lib.rs:12", Some(CompactString::from("peeps")));
        let source_id = intern_source(source.clone());
        assert_eq!(
            source_for_id(source_id)
                .expect("source should be present")
                .as_str(),
            source.as_str()
        );
        assert_eq!(
            source_for_id(source_id)
                .expect("source should be present")
                .krate(),
            source.krate()
        );
    }
}
