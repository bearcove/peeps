use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use facet::Facet;
use moire_trace_types::{FrameId, RelPc};
use moire_types::{SourcePreviewBatchRequest, SourcePreviewBatchResponse, SourcePreviewResponse};
use rusqlite_facet::ConnectionFacetExt;

use crate::api::source_context::{cut_source, extract_target_statement};
use crate::app::AppState;
use crate::db::Db;
use crate::snapshot::table::lookup_frame_source_by_raw;
use crate::util::http::{json_error, json_ok};
use crate::util::source_path::resolve_source_path;

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

// r[impl api.source.previews]
pub async fn api_source_previews(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let body: SourcePreviewBatchRequest = match facet_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("invalid request json: {error}"),
            );
        }
    };

    if body.frame_ids.is_empty() {
        return json_error(
            StatusCode::BAD_REQUEST,
            "frame_ids must be non-empty for source preview batch fetch",
        );
    }

    let mut lookups = Vec::with_capacity(body.frame_ids.len());
    let mut unknown_frame_ids = Vec::new();
    for frame_id in body.frame_ids {
        match lookup_frame_source_by_raw(frame_id.as_u64()) {
            Some((canonical_frame_id, module_identity, rel_pc)) => {
                lookups.push((canonical_frame_id, module_identity, rel_pc));
            }
            None => unknown_frame_ids.push(frame_id),
        }
    }
    if !unknown_frame_ids.is_empty() {
        let rendered = unknown_frame_ids
            .iter()
            .map(|id| id.as_u64().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return json_error(
            StatusCode::BAD_REQUEST,
            format!("unknown frame_id values in batch: [{rendered}]"),
        );
    }

    let db = state.db.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut previews = Vec::with_capacity(lookups.len());
        let mut unavailable_frame_ids = Vec::new();
        for (frame_id, module_identity, rel_pc) in lookups {
            match lookup_source_in_db(&db, frame_id, module_identity, rel_pc)? {
                Some(preview) => previews.push(preview),
                None => unavailable_frame_ids.push(frame_id),
            }
        }
        Ok::<SourcePreviewBatchResponse, String>(SourcePreviewBatchResponse {
            previews,
            unavailable_frame_ids,
        })
    })
    .await
    .unwrap_or_else(|error| Err(format!("join source preview batch lookup: {error}")));

    match result {
        Ok(response) => json_ok(&response),
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
        source_file: resolved_path.into_owned(),
        target_line,
        target_col,
        total_lines,
        html,
        context_html,
        context_range,
        context_line,
    }))
}
