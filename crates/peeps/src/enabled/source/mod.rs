#![doc = include_str!("README.md")]
use compact_str::CompactString;
use std::panic::Location;
use std::path::Path;

/// Unqualified callsite half of source identity.
///
/// Where it comes from: `Location::caller()` at wrapper callsites.
///
/// When it is captured: immediately before calling instrumented implementations.
///
/// Where it is used: joined with [`SourceLeft`] to resolve a concrete source string.
///
/// This value alone does not identify a crate. Relative paths are unresolved until joined.
#[derive(Clone, Copy, Debug)]
pub struct SourceRight {
    file: &'static str,
    line: u32,
}

impl SourceRight {
    #[track_caller]
    pub fn caller() -> Self {
        let location = Location::caller();
        Self::new(location.file(), location.line())
    }

    pub const fn new(file: &'static str, line: u32) -> Self {
        Self { file, line }
    }

    pub const fn file(self) -> &'static str {
        self.file
    }

    pub const fn line(self) -> u32 {
        self.line
    }

    pub fn join(self, left: SourceLeft) -> Source {
        Source::resolve(left, self)
    }
}

/// Ambient crate-context half of source identity.
///
/// Where it comes from: facade-expanded constants based on `env!("CARGO_MANIFEST_DIR")`.
///
/// When it is captured: compile time in the calling crate.
///
/// Where it is used: joined with [`SourceRight`] to resolve relative callsite paths and infer
/// crate name by reading `Cargo.toml`.
#[derive(Clone, Copy, Debug)]
pub struct SourceLeft {
    manifest_dir: &'static str,
}

impl SourceLeft {
    pub const fn new(manifest_dir: &'static str) -> Self {
        Self { manifest_dir }
    }

    #[track_caller]
    pub const fn caller(manifest_dir: &'static str) -> Self {
        Self::new(manifest_dir)
    }

    pub const fn manifest_dir(self) -> &'static str {
        self.manifest_dir
    }

    pub fn join(self, right: SourceRight) -> Source {
        Source::resolve(self, right)
    }
}

/// Fully resolved source identity used by instrumentation.
///
/// Where it comes from: `SourceLeft + SourceRight` via [`Source::resolve`] or `join`.
///
/// When it is produced: at the instrumentation boundary where both halves are present.
///
/// Where it is used: event/entity source fields and crate identity.
#[derive(Clone, Debug)]
pub struct Source {
    source: CompactString,
    krate: Option<CompactString>,
}

impl Source {
    pub fn resolve(left: SourceLeft, right: SourceRight) -> Self {
        let file = Path::new(right.file());
        let resolved = if file.is_absolute() {
            file.to_path_buf()
        } else {
            Path::new(left.manifest_dir()).join(file)
        };
        let source = CompactString::from(format!("{}:{}", resolved.display(), right.line()));
        let krate = infer_crate_name_from_manifest_dir(left.manifest_dir());
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

pub type UnqualSource = SourceRight;
pub type CrateContext = SourceLeft;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolves_relative_path_from_manifest_dir() {
        let left = SourceLeft::new("/repo/crate");
        let right = SourceRight::new("src/lib.rs", 42);
        let source = left.join(right);
        let expected = PathBuf::from("/repo/crate").join("src/lib.rs");
        assert_eq!(source.as_str(), format!("{}:42", expected.display()));
    }

    #[test]
    fn preserves_absolute_path() {
        let left = SourceLeft::new("/repo/crate");
        let right = SourceRight::new("/other/place/main.rs", 7);
        let source = left.join(right);
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

        let left = SourceLeft::new(base.to_str().expect("temp path must be valid utf-8"));
        let right = SourceRight::new("src/lib.rs", 1);
        let source = left.join(right);

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

        let left = SourceLeft::new(base.to_str().expect("temp path must be valid utf-8"));
        let right = SourceRight::new("src/lib.rs", 1);
        let source = left.join(right);

        assert_eq!(source.krate(), None);
        std::fs::remove_dir_all(base).expect("failed to cleanup temp dir");
    }
}
