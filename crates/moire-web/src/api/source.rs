use arborium::tree_sitter;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use facet::Facet;
use moire_trace_types::{FrameId, RelPc};
use moire_types::{LineRange, SourcePreviewResponse};
use rusqlite_facet::ConnectionFacetExt;

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

/// Node kinds that represent "interesting" statements in Rust's tree-sitter grammar.
const INTERESTING_RUST_KINDS: &[&str] = &[
    "let_declaration",
    "const_item",
    "static_item",
    "expression_statement",
    "macro_invocation",
    "function_item",
    "closure_expression",
    "if_expression",
    "match_expression",
    "for_expression",
    "while_expression",
    "loop_expression",
    "return_expression",
    "assignment_expression",
    "compound_assignment_expr",
];

/// Given source content and a target position, use tree-sitter to find the
/// containing statement when the target line is uninteresting (`.await`, `}`, etc.).
///
/// Returns `Some(LineRange)` when the containing statement starts on a different
/// line than `target_line`, `None` otherwise.
fn find_display_range(
    lang_name: &str,
    content: &str,
    target_line: u32,
    target_col: Option<u32>,
) -> Option<LineRange> {
    let ts_lang = arborium::get_language(lang_name)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang).ok()?;
    let tree = parser.parse(content.as_bytes(), None)?;

    let row = target_line - 1; // tree-sitter is 0-based
    let col = target_col.unwrap_or(0);
    let point = tree_sitter::Point::new(row as usize, col as usize);

    let node = tree
        .root_node()
        .named_descendant_for_point_range(point, point)?;

    // Walk up until we find an interesting node
    let mut current = node;
    let interesting = loop {
        if INTERESTING_RUST_KINDS.contains(&current.kind()) {
            break current;
        }
        current = current.parent()?;
    };

    let start = interesting.start_position().row as u32 + 1;
    let end = interesting.end_position().row as u32 + 1;

    // Only return a range if it differs from just the target line
    if start == target_line && end == target_line {
        return None;
    }

    Some(LineRange { start, end })
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

    let content = std::fs::read_to_string(&source_file_path)
        .map_err(|error| format!("read source file {source_file_path}: {error}"))?;

    let total_lines = u32::try_from(content.lines().count()).unwrap_or(u32::MAX);

    let lang = arborium_language(&source_file_path);

    let display_range = lang.and_then(|l| find_display_range(l, &content, target_line, target_col));

    let html = match lang {
        Some(lang) => {
            let mut hl = arborium::Highlighter::new();
            hl.highlight(lang, &content)
                .unwrap_or_else(|_| html_escape(&content))
        }
        None => html_escape(&content),
    };

    Ok(Some(SourcePreviewResponse {
        frame_id,
        source_file: source_file_path,
        target_line,
        target_col,
        display_range,
        total_lines,
        html,
    }))
}
