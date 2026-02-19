#![doc = include_str!("README.md")]

use camino::{Utf8Path, Utf8PathBuf};
use facet::Facet;
#[cfg(feature = "rusqlite")]
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::collections::BTreeMap;
use std::fmt;
use std::panic::Location;
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

    pub fn into_string(self) -> String {
        format!("{}:{}", self.location.file(), self.location.line())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SourceLeft {
    manifest_dir: &'static str,
    krate: &'static str,
}

impl SourceLeft {
    pub const fn new(manifest_dir: &'static str, krate: &'static str) -> Self {
        Self {
            manifest_dir,
            krate,
        }
    }

    pub const fn manifest_dir(self) -> &'static str {
        self.manifest_dir
    }

    pub const fn krate(self) -> &'static str {
        self.krate
    }

    #[track_caller]
    pub fn resolve(self) -> Source {
        self.join(SourceRight::caller())
    }

    pub fn join(self, right: SourceRight) -> Source {
        Source::resolve(self, right)
    }
}

/// A fully resolved source code location, including crate identity.
#[derive(Clone, Debug)]
pub struct Source {
    /// Absolute UTF-8 path to the source file on disk.
    path: Utf8PathBuf,
    /// 1-based source line number.
    line: u32,
    /// Crate name that owns this source location.
    krate: String,
}

impl Source {
    pub fn resolve(left: SourceLeft, right: SourceRight) -> Self {
        Self::resolve_parts(
            left.manifest_dir(),
            left.krate(),
            right.location.file(),
            right.location.line(),
        )
    }

    fn resolve_parts(manifest_dir: &str, krate: &str, file: &str, line: u32) -> Self {
        let file = Utf8Path::new(file);
        let path = if file.is_absolute() {
            file.to_path_buf()
        } else {
            Utf8Path::new(manifest_dir).join(file)
        };
        Self {
            path,
            line,
            krate: String::from(krate),
        }
    }

    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    pub const fn line(&self) -> u32 {
        self.line
    }

    pub fn krate(&self) -> &str {
        self.krate.as_str()
    }

    pub fn into_display_string(self) -> String {
        format!("{}:{}", self.path, self.line)
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path, self.line)
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
    let lock = source_intern();
    let mut intern = lock
        .lock()
        .expect("source intern mutex poisoned; cannot continue");
    intern.intern(source)
}

pub fn source_for_id(source_id: SourceId) -> Option<Source> {
    let lock = source_intern();
    let intern = lock
        .lock()
        .expect("source intern mutex poisoned; cannot continue");
    intern.lookup(source_id)
}

fn source_intern() -> &'static StdMutex<SourceIntern> {
    static SOURCE_INTERN: OnceLock<StdMutex<SourceIntern>> = OnceLock::new();
    SOURCE_INTERN.get_or_init(|| StdMutex::new(SourceIntern::new()))
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

struct SourceIntern {
    next_id: u64,
    by_key: BTreeMap<(Utf8PathBuf, u32, String), SourceId>,
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
            source.path.clone(),
            source.line,
            String::from(source.krate.as_str()),
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

    #[test]
    fn resolves_relative_path_from_manifest_dir() {
        let source = Source::resolve_parts("/repo/crate", "my-crate", "src/lib.rs", 42);
        assert_eq!(source.path(), Utf8Path::new("/repo/crate/src/lib.rs"));
        assert_eq!(source.line(), 42);
        assert_eq!(source.krate(), "my-crate");
        assert_eq!(source.to_string(), "/repo/crate/src/lib.rs:42");
    }

    #[test]
    fn preserves_absolute_path() {
        let source = Source::resolve_parts("/repo/crate", "my-crate", "/other/place/main.rs", 7);
        assert_eq!(source.path(), Utf8Path::new("/other/place/main.rs"));
        assert_eq!(source.line(), 7);
        assert_eq!(source.krate(), "my-crate");
    }

    #[test]
    fn intern_round_trips_source() {
        let source = Source::resolve_parts("/repo", "peeps", "src/lib.rs", 12);
        let source_id = intern_source(source.clone());
        let loaded = source_for_id(source_id).expect("source should be present");
        assert_eq!(loaded.path(), source.path());
        assert_eq!(loaded.line(), source.line());
        assert_eq!(loaded.krate(), source.krate());
    }
}
