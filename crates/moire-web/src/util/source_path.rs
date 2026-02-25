static SYSROOT_BY_HASH: std::sync::OnceLock<std::collections::HashMap<String, String>> =
    std::sync::OnceLock::new();

fn sysroot_by_commit_hash() -> &'static std::collections::HashMap<String, String> {
    SYSROOT_BY_HASH.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        let home = match std::env::var("RUSTUP_HOME")
            .ok()
            .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.rustup")))
        {
            Some(h) => h,
            None => return map,
        };
        let toolchains_dir = format!("{home}/toolchains");
        let entries = match std::fs::read_dir(&toolchains_dir) {
            Ok(e) => e,
            Err(_) => return map,
        };
        for entry in entries.flatten() {
            let intro = entry.path().join("share/doc/rust/html/intro.html");
            let html = match std::fs::read_to_string(&intro) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let needle = "/rust-lang/rust/commit/";
            let commit_hash = html.find(needle).and_then(|pos| {
                let after = &html[pos + needle.len()..];
                let end = after
                    .find(|c: char| !c.is_ascii_hexdigit())
                    .unwrap_or(after.len());
                let hash = &after[..end];
                if hash.len() == 40 {
                    Some(hash.to_owned())
                } else {
                    None
                }
            });
            if let Some(hash) = commit_hash {
                let sysroot = entry.path().to_string_lossy().into_owned();
                map.insert(hash, sysroot);
            }
        }
        map
    })
}

/// Remaps `/rustc/{hash}/...` to the matching rustup toolchain's rust-src component.
pub fn resolve_source_path(path: &str) -> std::borrow::Cow<'_, str> {
    if let Some(after_rustc) = path.strip_prefix("/rustc/") {
        if let Some(slash) = after_rustc.find('/') {
            let hash = &after_rustc[..slash];
            if hash.len() == 40 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                let rest = &after_rustc[slash + 1..];
                if let Some(sysroot) = sysroot_by_commit_hash().get(hash) {
                    let remapped = format!("{sysroot}/lib/rustlib/src/rust/{rest}");
                    return std::borrow::Cow::Owned(remapped);
                }
            }
        }
    }
    std::borrow::Cow::Borrowed(path)
}
