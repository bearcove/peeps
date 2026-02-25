use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use facet::Facet;
use moire_trace_types::{FrameId, RelPc};
use moire_types::SourcePreviewResponse;
use rusqlite_facet::ConnectionFacetExt;

use crate::api::source_context::{cut_source, extract_target_statement};
use crate::app::AppState;
use crate::db::Db;
use crate::snapshot::table::lookup_frame_source_by_raw;
use crate::util::http::{json_error, json_ok};

#[derive(Facet)]
struct SymbolicationCacheRow {
    source_file_path: Option<String>,
    source_line: Option<i64>,
    source_col: Option<i64>,
}

#[derive(Facet)]
struct SymbolicationCacheParams {
    module_identity: String,
    rel_pc: RelPc,
}

// r[impl api.source.preview]
pub async fn api_source_preview(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> impl IntoResponse {
    let raw_query = raw_query.unwrap_or_default();

    // r[impl api.source.preview.frame-id]
    let frame_id_raw = match parse_query_u64(&raw_query, "frame_id") {
        Some(v) => v,
        None => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "missing or invalid frame_id query parameter",
            );
        }
    };

    // r[impl api.source.preview.security]
    let (frame_id, module_identity, rel_pc) = match lookup_frame_source_by_raw(frame_id_raw) {
        Some(triple) => triple,
        None => {
            return json_error(
                StatusCode::NOT_FOUND,
                format!("unknown frame_id {frame_id_raw}"),
            );
        }
    };

    let db = state.db.clone();
    let result = tokio::task::spawn_blocking(move || {
        lookup_source_in_db(&db, frame_id, module_identity, rel_pc)
    })
    .await
    .unwrap_or_else(|error| Err(format!("join source lookup: {error}")));

    match result {
        Ok(Some(response)) => json_ok(&response),
        Ok(None) => json_error(StatusCode::NOT_FOUND, "source not available for frame"),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

fn parse_query_u64(query: &str, key: &str) -> Option<u64> {
    query.split('&').find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k == key {
            v.parse::<u64>().ok()
        } else {
            None
        }
    })
}

fn arborium_language(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "rs" => Some("rust"),
        "go" => Some("go"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("jsx"),
        "py" => Some("python"),
        "rb" => Some("ruby"),
        "java" => Some("java"),
        "kt" | "kts" => Some("kotlin"),
        "scala" => Some("scala"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("cpp"),
        "zig" => Some("zig"),
        "sh" | "bash" => Some("bash"),
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "toml" => Some("toml"),
        "xml" => Some("xml"),
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "scss" => Some("scss"),
        "md" | "mdx" => Some("markdown"),
        "sql" => Some("sql"),
        "swift" => Some("swift"),
        "ex" | "exs" => Some("elixir"),
        "hs" => Some("haskell"),
        "ml" | "mli" => Some("ocaml"),
        "lua" => Some("lua"),
        "php" => Some("php"),
        "r" => Some("r"),
        _ => None,
    }
}

static SYSROOT_BY_HASH: std::sync::OnceLock<std::collections::HashMap<String, String>> =
    std::sync::OnceLock::new();

/// Scans `~/.rustup/toolchains/`, runs `rustc -vV` for each, and builds a map
/// from commit-hash → sysroot path. Cached after first call.
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
            // The commit hash appears in share/doc/rust/html/intro.html as a
            // GitHub commit URL: /rust-lang/rust/commit/<hash>
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
fn resolve_source_path(path: &str) -> std::borrow::Cow<'_, str> {
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

fn html_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '&' => result.push_str("&amp;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(ch),
        }
    }
    result
}

fn lookup_source_in_db(
    db: &Db,
    frame_id: FrameId,
    module_identity: String,
    rel_pc: RelPc,
) -> Result<Option<SourcePreviewResponse>, String> {
    let conn = db.open()?;

    let rows = conn
        .facet_query_ref::<SymbolicationCacheRow, _>(
            "SELECT source_file_path, source_line, source_col
             FROM symbolication_cache
             WHERE module_identity = :module_identity AND rel_pc = :rel_pc",
            &SymbolicationCacheParams {
                module_identity,
                rel_pc,
            },
        )
        .map_err(|error| format!("query symbolication_cache: {error}"))?;

    let row = match rows.into_iter().next() {
        Some(row) => row,
        None => return Ok(None),
    };

    let source_file_path = match row.source_file_path {
        Some(path) if !path.is_empty() => path,
        _ => return Ok(None),
    };

    let target_line = match row.source_line {
        Some(line) if line > 0 => {
            u32::try_from(line).map_err(|_| format!("source_line {line} out of u32 range"))?
        }
        _ => return Ok(None),
    };

    let target_col = row.source_col.and_then(|col| u32::try_from(col).ok());

    let resolved_path = resolve_source_path(&source_file_path);

    let content = std::fs::read_to_string(resolved_path.as_ref())
        .map_err(|error| format!("read source file {resolved_path}: {error}"))?;

    let total_lines = u32::try_from(content.lines().count()).unwrap_or(u32::MAX);

    let lang = arborium_language(&source_file_path);

    // Full file highlight
    let html = match lang {
        Some(lang_name) => {
            let mut hl = arborium::Highlighter::new();
            hl.highlight(lang_name, &content)
                .unwrap_or_else(|_| html_escape(&content))
        }
        None => html_escape(&content),
    };

    // Language-aware context: cut the scope and highlight the excerpt
    let (context_html, context_range) = match lang {
        Some(lang_name) => match cut_source(&content, lang_name, target_line, target_col) {
            Some(cut_result) => {
                let highlighted = {
                    let mut hl = arborium::Highlighter::new();
                    hl.highlight(lang_name, &cut_result.cut_source)
                        .unwrap_or_else(|_| html_escape(&cut_result.cut_source))
                };
                (Some(highlighted), Some(cut_result.scope_range))
            }
            None => (None, None),
        },
        None => (None, None),
    };

    // Single-line collapsed statement for compact display
    let context_line = lang.and_then(|lang_name| {
        let stmt = extract_target_statement(&content, lang_name, target_line, target_col)?;
        let mut hl = arborium::Highlighter::new();
        Some(
            hl.highlight(lang_name, &stmt)
                .unwrap_or_else(|_| html_escape(&stmt)),
        )
    });

    Ok(Some(SourcePreviewResponse {
        frame_id,
        source_file: source_file_path,
        target_line,
        target_col,
        total_lines,
        html,
        context_html,
        context_range,
        context_line,
    }))
}
