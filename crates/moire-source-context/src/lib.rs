use arborium::HtmlFormat;
use arborium::advanced::{Span, spans_to_html};
use arborium::tree_sitter;
use moire_types::LineRange;
use moire_types::{ContextCodeLine, ContextSeparator, SourceContextLine};

pub struct CutResult {
    /// The scope excerpt with cuts applied. Only contains lines from scope_range.
    /// Within that range, cut regions have their first line replaced with `/* ... */`
    /// and remaining lines empty. Line count = scope_range.end - scope_range.start + 1.
    pub cut_source: String,
    /// 1-based inclusive line range of the displayed scope excerpt in the original file.
    /// For function scopes, this starts at the function body (signature omitted).
    /// Line 1 of cut_source = line scope_range.start in the original.
    pub scope_range: LineRange,
}

/// Scope-level node kinds we recognize in Rust's tree-sitter grammar.
const SCOPE_KINDS: &[&str] = &[
    "function_item",
    "closure_expression",
    "impl_item",
    "source_file",
];

/// The body child name for each scope kind.
fn body_kind_for_scope(scope_kind: &str) -> &'static str {
    match scope_kind {
        "function_item" | "closure_expression" => "block",
        "impl_item" => "declaration_list",
        _ => "source_file", // sentinel — we use all children
    }
}

fn display_scope_rows(
    scope: tree_sitter::Node<'_>,
    body: Option<tree_sitter::Node<'_>>,
    body_children: &[tree_sitter::Node<'_>],
) -> (usize, usize) {
    if scope.kind() != "function_item" {
        return (scope.start_position().row, scope.end_position().row);
    }

    // For function items, avoid duplicating the signature by using body rows.
    if let (Some(first), Some(last)) = (body_children.first(), body_children.last()) {
        return (first.start_position().row, last.end_position().row);
    }

    // Empty-body fallback: keep a stable single line.
    let mut row = body
        .map(|body_node| body_node.start_position().row.saturating_add(1))
        .unwrap_or_else(|| scope.start_position().row);
    let scope_end_row = scope.end_position().row;
    if row > scope_end_row {
        row = scope_end_row;
    }
    (row, row)
}

/// Number of sibling statements to keep on each side of the target for
/// regular (expanded) context rendering.
const NEIGHBOR_COUNT: usize = 1;

/// Number of sibling statements to keep on each side of the target for
/// compact/collapsed rendering.
const COMPACT_NEIGHBOR_COUNT: usize = 0;

/// Given source content, a language name, and a target position, find the
/// containing scope, classify its children into keep/cut, and return the
/// modified source with `/* ... */` placeholders for cut regions.
///
/// The returned `cut_source` preserves the same number of lines as the
/// displayed scope excerpt in the original file
/// (scope_range.end - scope_range.start + 1), making line numbers stable
/// for gutter rendering.
pub fn cut_source(
    content: &str,
    lang_name: &str,
    target_line: u32,
    target_col: Option<u32>,
) -> Option<CutResult> {
    cut_source_with_neighbor_count(content, lang_name, target_line, target_col, NEIGHBOR_COUNT)
}

/// Aggressive context cutter for compact/collapsed displays.
pub fn cut_source_compact(
    content: &str,
    lang_name: &str,
    target_line: u32,
    target_col: Option<u32>,
) -> Option<CutResult> {
    cut_source_with_neighbor_count(
        content,
        lang_name,
        target_line,
        target_col,
        COMPACT_NEIGHBOR_COUNT,
    )
}

fn cut_source_with_neighbor_count(
    content: &str,
    lang_name: &str,
    target_line: u32,
    target_col: Option<u32>,
    neighbor_count: usize,
) -> Option<CutResult> {
    let ts_lang = arborium::get_language(lang_name)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang).ok()?;
    let tree = parser.parse(content.as_bytes(), None)?;

    let row = (target_line - 1) as usize;
    let col = target_col.unwrap_or(0) as usize;
    let point = tree_sitter::Point::new(row, col);

    // Find deepest named node at target
    let node = tree
        .root_node()
        .named_descendant_for_point_range(point, point)?;

    // Walk up to nearest scope node
    let scope = find_scope(node, point)?;

    let source_lines: Vec<&str> = content.lines().collect();

    // Find the body child
    let body_kind = body_kind_for_scope(scope.kind());
    let is_source_file = scope.kind() == "source_file";

    // Collect body children (the statements/items inside the scope)
    let (body_node, body_children): (Option<tree_sitter::Node>, Vec<tree_sitter::Node>) =
        if is_source_file {
            // For source_file, all named children are body children
            (
                None,
                (0..scope.named_child_count())
                    .filter_map(|i| scope.named_child(i))
                    .collect(),
            )
        } else {
            // Find the body node (block or declaration_list)
            let body = (0..scope.child_count())
                .filter_map(|i| scope.child(i))
                .find(|c| c.kind() == body_kind)?;
            let children = (0..body.named_child_count())
                .filter_map(|i| body.named_child(i))
                .collect();
            (Some(body), children)
        };
    let is_function_scope = scope.kind() == "function_item";
    let (mut scope_start_row, mut scope_end_row) =
        display_scope_rows(scope, body_node, &body_children);

    // If few enough children, no cutting needed — just return the scope as-is.
    // Skip in compact mode: collect_compact_block_elision_ranges may still
    // need to elide block interiors within the single kept child.
    if body_children.len() <= (neighbor_count * 2 + 1) && neighbor_count != COMPACT_NEIGHBOR_COUNT {
        let scope_start = scope_start_row as u32 + 1;
        let scope_end = scope_end_row as u32 + 1;
        let cut = source_lines[(scope_start - 1) as usize..scope_end as usize].join("\n");
        return Some(CutResult {
            cut_source: cut,
            scope_range: LineRange {
                start: scope_start,
                end: scope_end,
            },
        });
    }

    // Find the target child index
    let target_idx = body_children
        .iter()
        .position(|child| contains_point(child, point))
        .unwrap_or_else(|| {
            // Fallback: find closest child by line
            body_children
                .iter()
                .enumerate()
                .min_by_key(|(_, c)| {
                    let c_start = c.start_position().row;
                    let c_end = c.end_position().row;
                    if row >= c_start && row <= c_end {
                        0usize
                    } else if row < c_start {
                        c_start - row
                    } else {
                        row - c_end
                    }
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        });

    // Classify children: keep neighbors around target, cut the rest
    let keep_start = target_idx.saturating_sub(neighbor_count);
    let keep_end = (target_idx + neighbor_count + 1).min(body_children.len());

    if is_function_scope && !body_children.is_empty() {
        scope_start_row = body_children[keep_start].start_position().row;
        scope_end_row = body_children[keep_end - 1].end_position().row;
    }

    // Build cut_source by processing lines
    let mut result_lines: Vec<String> = Vec::with_capacity(scope_end_row - scope_start_row + 1);

    // Determine which line ranges to cut (0-based rows)
    let mut cut_ranges: Vec<(usize, usize)> = Vec::new(); // inclusive start/end rows

    // Lines before first body child but after scope header: keep (scope header)
    // Lines of cut children: replace
    // Lines of kept children: keep
    // Lines after last body child but before scope end: keep (closing brace)

    if !body_children.is_empty() {
        // Before the keep range: cut children [0..keep_start)
        if keep_start > 0 && !is_function_scope {
            let cut_start = body_children[0].start_position().row;
            let cut_end = if keep_start < body_children.len() {
                body_children[keep_start]
                    .start_position()
                    .row
                    .saturating_sub(1)
            } else {
                body_children[body_children.len() - 1].end_position().row
            };
            if cut_end >= cut_start {
                cut_ranges.push((cut_start, cut_end));
            }
        }

        // After the keep range: cut children [keep_end..len)
        if keep_end < body_children.len() && !is_function_scope {
            let cut_start = body_children[keep_end].start_position().row;
            let cut_end = body_children[body_children.len() - 1].end_position().row;
            if cut_end >= cut_start {
                cut_ranges.push((cut_start, cut_end));
            }
        }

        // Compact mode: also elide interiors of kept blocks (e.g. async move bodies)
        // while preserving outer lines and stable file-based line numbers.
        if neighbor_count == COMPACT_NEIGHBOR_COUNT {
            for child in &body_children[keep_start..keep_end] {
                collect_compact_block_elision_ranges(*child, &mut cut_ranges);
            }
        }
    }

    cut_ranges = merge_line_ranges(cut_ranges);

    for row_idx in scope_start_row..=scope_end_row {
        let in_cut = cut_ranges
            .iter()
            .find(|(s, e)| row_idx >= *s && row_idx <= *e);
        if let Some(&(cut_start, _cut_end)) = in_cut {
            if row_idx == cut_start {
                // First line of cut region: insert /* ... */ with indentation
                let indent = if row_idx < source_lines.len() {
                    let line = source_lines[row_idx];
                    let trimmed = line.trim_start();
                    &line[..line.len() - trimmed.len()]
                } else {
                    ""
                };
                result_lines.push(format!("{indent}/* ... */"));
            } else {
                // Remaining lines in cut region: empty
                result_lines.push(String::new());
            }
        } else if row_idx < source_lines.len() {
            result_lines.push(source_lines[row_idx].to_string());
        } else {
            result_lines.push(String::new());
        }
    }

    Some(CutResult {
        cut_source: result_lines.join("\n"),
        scope_range: LineRange {
            start: scope_start_row as u32 + 1,
            end: scope_end_row as u32 + 1,
        },
    })
}

fn collect_compact_block_elision_ranges(
    node: tree_sitter::Node<'_>,
    ranges: &mut Vec<(usize, usize)>,
) {
    if is_block_like_statement_node(node.kind()) {
        let start_row = node.start_position().row;
        let end_row = node.end_position().row;
        if end_row > start_row + 1 {
            ranges.push((start_row + 1, end_row - 1));
        }
    }

    // Elide long parameter lists: keep only the first parameter.
    if node.kind() == "parameters" && node.parent().is_some_and(|p| p.kind() == "function_item") {
        let start_row = node.start_position().row;
        let end_row = node.end_position().row;
        if end_row > start_row + MAX_PARAM_LIST_ROWS {
            let first_param_end_row = (0..node.named_child_count())
                .filter_map(|i| node.named_child(i))
                .next()
                .map(|p| p.end_position().row)
                .unwrap_or(start_row);
            let elide_start = first_param_end_row + 1;
            let elide_end = end_row.saturating_sub(1);
            if elide_end >= elide_start {
                ranges.push((elide_start, elide_end));
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_compact_block_elision_ranges(child, ranges);
        }
    }
}

/// Walk up the tree to find the nearest scope node.
///
/// Closures with ≤1 statement walk up to find richer outer context.
/// `function_item` and `impl_item` are always terminal — their body is the
/// context; the signature is shown separately as the frame header.
/// `source_file` is also always terminal.
fn find_scope<'a>(
    node: tree_sitter::Node<'a>,
    target: tree_sitter::Point,
) -> Option<tree_sitter::Node<'a>> {
    let mut current = node;
    let mut found_scope: Option<tree_sitter::Node<'a>> = None;

    loop {
        if SCOPE_KINDS.contains(&current.kind()) {
            let child_count = count_body_children(&current);
            // If the target is on the closing brace row, don't walk up —
            // this scope is the right one regardless of child count.
            let on_closing_brace = target_is_on_closing_brace(&current, target);
            let is_terminal = matches!(
                current.kind(),
                "source_file" | "function_item" | "impl_item"
            );
            if child_count <= 1 && !is_terminal && !on_closing_brace {
                // Too few statements in a closure — keep looking for outer scope
                found_scope = Some(current);
            } else {
                return Some(current);
            }
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                // Reached root — check if root is source_file
                if current.kind() == "source_file" && contains_point(&current, target) {
                    return Some(current);
                }
                return found_scope;
            }
        }
    }
}

/// Returns true if `target` is on the closing brace row of `scope`'s body
/// (i.e. target.row >= body.end_position().row).  This means the target is
/// past all statements and we should NOT walk up to a richer outer scope.
fn target_is_on_closing_brace(scope: &tree_sitter::Node, target: tree_sitter::Point) -> bool {
    if scope.kind() == "source_file" {
        return false;
    }
    let body_kind = body_kind_for_scope(scope.kind());
    let body = (0..scope.child_count())
        .filter_map(|i| scope.child(i))
        .find(|c| c.kind() == body_kind);
    match body {
        Some(b) => target.row >= b.end_position().row,
        None => false,
    }
}

fn count_body_children(scope: &tree_sitter::Node) -> usize {
    let body_kind = body_kind_for_scope(scope.kind());
    if scope.kind() == "source_file" {
        return scope.named_child_count();
    }
    let body = (0..scope.child_count())
        .filter_map(|i| scope.child(i))
        .find(|c| c.kind() == body_kind);
    match body {
        Some(b) => b.named_child_count(),
        None => 0,
    }
}

fn contains_point(node: &tree_sitter::Node, point: tree_sitter::Point) -> bool {
    let start = node.start_position();
    let end = node.end_position();
    (point.row > start.row || (point.row == start.row && point.column >= start.column))
        && (point.row < end.row || (point.row == end.row && point.column <= end.column))
}

/// Statement-level node kinds we walk up to when extracting the target statement.
const STATEMENT_KINDS: &[&str] = &[
    "let_declaration",
    "const_item",
    "static_item",
    "expression_statement",
    "macro_invocation",
    "function_item",
    "if_expression",
    "match_expression",
    "for_expression",
    "while_expression",
    "loop_expression",
    "return_expression",
];

/// Maximum number of interior lines (`{ ... }` body only) before we elide a block.
const STATEMENT_BLOCK_INTERIOR_MAX_LINES: usize = 4;

/// If a function's parameter list spans more than this many rows, elide all
/// parameters after the first one, leaving a single `/* ... */` placeholder.
const MAX_PARAM_LIST_ROWS: usize = 3;

/// Extract a compact, context-aware statement snippet around the target position.
///
/// - Preserves formatting/newlines (not whitespace-collapsed).
/// - Strips leading Rust attributes on the statement.
/// - Aggressively elides long block interiors with a placeholder to keep cards compact.
pub fn extract_target_statement(
    content: &str,
    lang_name: &str,
    target_line: u32,
    target_col: Option<u32>,
) -> Option<String> {
    let ts_lang = arborium::get_language(lang_name)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang).ok()?;
    let tree = parser.parse(content.as_bytes(), None)?;

    let row = (target_line - 1) as usize;
    let col = target_col.unwrap_or(0) as usize;
    let point = tree_sitter::Point::new(row, col);

    let node = tree
        .root_node()
        .named_descendant_for_point_range(point, point)?;

    // Walk up to nearest statement node, but stop before scope nodes
    let mut current = node;
    loop {
        if STATEMENT_KINDS.contains(&current.kind()) {
            break;
        }
        if SCOPE_KINDS.contains(&current.kind()) {
            break;
        }
        match current.parent() {
            Some(parent) => {
                if SCOPE_KINDS.contains(&parent.kind()) {
                    // If we're on an intermediate node (not a block/decl_list
                    // that we can search children of) and the parent is also a
                    // statement kind (e.g. function_item), step up to it — we
                    // want the full signature, not just a sub-node like
                    // `parameters` or `async`.
                    let is_body_container =
                        current.kind() == "block" || current.kind() == "declaration_list";
                    if !is_body_container && STATEMENT_KINDS.contains(&parent.kind()) {
                        current = parent;
                    }
                    break;
                }
                current = parent;
            }
            None => break,
        }
    }

    // If we stopped at a block/declaration_list, find the child whose row range covers target
    let statement = if current.kind() == "block" || current.kind() == "declaration_list" {
        (0..current.named_child_count())
            .filter_map(|i| current.named_child(i))
            .find(|c| {
                let s = c.start_position().row;
                let e = c.end_position().row;
                point.row >= s && point.row <= e
            })
            .unwrap_or(current)
    } else {
        current
    };

    // Prefer the outermost statement before we hit a scope boundary.
    // Example: if target is inside `match` in `let x = match ...`, show the whole `let`.
    let statement = {
        let mut outer = statement;
        while let Some(parent) = outer.parent() {
            if SCOPE_KINDS.contains(&parent.kind()) {
                break;
            }
            if STATEMENT_KINDS.contains(&parent.kind()) {
                outer = parent;
                continue;
            }
            break;
        }
        outer
    };

    // Skip leading attribute children so compact view doesn't show
    // `#[moire::instrument] pub async fn foo()` — just `pub async fn foo()`.
    let (text_start, text_start_row) = {
        let mut start = statement.start_byte();
        let mut row = statement.start_position().row;
        for i in 0..statement.child_count() {
            if let Some(child) = statement.child(i) {
                if child.kind() == "attribute_item"
                    || child.kind() == "attribute"
                    || child.kind() == "attributes"
                {
                    continue;
                }
                start = child.start_byte();
                row = child.start_position().row;
                break;
            }
        }
        (start, row)
    };

    let text = &content[text_start..statement.end_byte()];
    let snippet = compact_statement_text(statement, text, text_start_row);
    if snippet.is_empty() {
        return None;
    }

    Some(snippet)
}

/// Extract the collapsed enclosing-function context for compact display.
///
/// Walks up the syntax tree from the target location to find the nearest
/// enclosing `function_item` (named function or method, not closure).
///
/// Returns a compact single-line signature context that includes:
/// - enclosing module path (if any)
/// - enclosing impl type (if any)
/// - full function signature (with parameter + return types)
///
/// Currently only implemented for Rust; returns `None` for other languages.
pub fn extract_enclosing_fn(
    content: &str,
    lang_name: &str,
    target_line: u32,
    target_col: Option<u32>,
) -> Option<String> {
    if lang_name != "rust" {
        return None;
    }
    let ts_lang = arborium::get_language(lang_name)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang).ok()?;
    let tree = parser.parse(content.as_bytes(), None)?;

    let row = (target_line - 1) as usize;
    let col = target_col.unwrap_or(0) as usize;
    let point = tree_sitter::Point::new(row, col);

    let node = tree
        .root_node()
        .named_descendant_for_point_range(point, point)?;

    let bytes = content.as_bytes();

    // Walk up to find the nearest enclosing function_item.
    // Closures (closure_expression) are skipped naturally since they
    // are not "function_item" nodes.
    let fn_node = {
        let mut current = node;
        loop {
            if current.kind() == "function_item" {
                break Some(current);
            }
            match current.parent() {
                Some(parent) => current = parent,
                None => break None,
            }
        }
    }?;

    let signature = extract_function_signature_text(content, &fn_node)?;
    let mut qualifiers = collect_module_qualifiers(&fn_node, bytes);
    if let Some(impl_type) = find_enclosing_impl_type_name(&fn_node, bytes) {
        qualifiers.push(impl_type);
    }

    if qualifiers.is_empty() {
        Some(signature)
    } else {
        Some(format!("{}::{}", qualifiers.join("::"), signature))
    }
}

fn collapse_ws_inline(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_function_signature_text(
    content: &str,
    fn_node: &tree_sitter::Node<'_>,
) -> Option<String> {
    let bytes = content.as_bytes();

    // Qualifiers before the fn keyword (async, const, unsafe, extern).
    // Tree-sitter groups them under a "function_modifiers" node.
    let mut qualifiers: Vec<&str> = Vec::new();
    for i in 0..fn_node.child_count() {
        let child = fn_node.child(i)?;
        match child.kind() {
            "attribute_item" | "visibility_modifier" => continue,
            "function_modifiers" => {
                qualifiers.push(child.utf8_text(bytes).ok()?);
            }
            "fn" => break,
            _ => {}
        }
    }

    let name = fn_node.child_by_field_name("name")?.utf8_text(bytes).ok()?;

    // Parameters: show self as-is, for others show only the pattern (name), not the type.
    let params_node = fn_node.child_by_field_name("parameters")?;
    let mut params: Vec<String> = Vec::new();
    for i in 0..params_node.child_count() {
        let child = params_node.child(i)?;
        match child.kind() {
            "parameter" => {
                // pattern field is the name; omit the type
                if let Some(pat) = child.child_by_field_name("pattern") {
                    params.push(pat.utf8_text(bytes).ok()?.to_string());
                }
            }
            "self_parameter" | "shorthand_self" => {
                // &self, &mut self, self, mut self
                params.push(collapse_ws_inline(child.utf8_text(bytes).ok()?));
            }
            _ => {}
        }
    }

    // Return type (optional)
    let return_type = fn_node.child_by_field_name("return_type").and_then(|rt| {
        let text = collapse_ws_inline(rt.utf8_text(bytes).ok()?);
        Some(format!(" -> {text}"))
    });

    let qualifiers_prefix = if qualifiers.is_empty() {
        String::new()
    } else {
        format!("{} ", qualifiers.join(" "))
    };

    let ret = return_type.as_deref().unwrap_or("");
    Some(format!(
        "{qualifiers_prefix}fn {name}({}){ret}",
        params.join(", "),
    ))
}

fn find_enclosing_impl_type_name(fn_node: &tree_sitter::Node<'_>, bytes: &[u8]) -> Option<String> {
    let mut current = *fn_node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "impl_item" {
            let type_node = parent.child_by_field_name("type")?;
            let raw = type_node.utf8_text(bytes).ok()?;
            let collapsed = collapse_ws_inline(raw);
            if collapsed.is_empty() {
                return None;
            }
            return Some(collapsed);
        }
        current = parent;
    }
    None
}

fn collect_module_qualifiers(fn_node: &tree_sitter::Node<'_>, bytes: &[u8]) -> Vec<String> {
    let mut rev_modules: Vec<String> = Vec::new();
    let mut current = *fn_node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "mod_item"
            && let Some(name_node) = parent.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(bytes)
        {
            let collapsed = collapse_ws_inline(name);
            if !collapsed.is_empty() {
                rev_modules.push(collapsed);
            }
        }
        current = parent;
    }
    rev_modules.reverse();
    rev_modules
}

fn is_block_like_statement_node(kind: &str) -> bool {
    kind == "block" || kind == "declaration_list" || kind == "match_block"
}

fn collect_statement_elision_ranges(node: tree_sitter::Node<'_>, ranges: &mut Vec<(usize, usize)>) {
    if is_block_like_statement_node(node.kind()) {
        let start_row = node.start_position().row;
        let end_row = node.end_position().row;
        if end_row > start_row + 1 {
            let interior_start = start_row + 1;
            let interior_end = end_row - 1;
            let interior_len = interior_end - interior_start + 1;
            let should_elide = node
                .parent()
                .is_some_and(|parent| parent.kind() == "function_item")
                || interior_len > STATEMENT_BLOCK_INTERIOR_MAX_LINES;
            if should_elide {
                ranges.push((interior_start, interior_end));
            }
        }
    }

    // Elide long parameter lists: keep only the first parameter.
    if node.kind() == "parameters" && node.parent().is_some_and(|p| p.kind() == "function_item") {
        let start_row = node.start_position().row;
        let end_row = node.end_position().row;
        if end_row > start_row + MAX_PARAM_LIST_ROWS {
            let first_param_end_row = (0..node.named_child_count())
                .filter_map(|i| node.named_child(i))
                .next()
                .map(|p| p.end_position().row)
                .unwrap_or(start_row);
            let elide_start = first_param_end_row + 1;
            let elide_end = end_row.saturating_sub(1);
            if elide_end >= elide_start {
                ranges.push((elide_start, elide_end));
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_statement_elision_ranges(child, ranges);
        }
    }
}

fn merge_line_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    if ranges.len() <= 1 {
        return ranges;
    }
    ranges.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some((_, last_end)) = merged.last_mut()
            && start <= *last_end + 1
        {
            if end > *last_end {
                *last_end = end;
            }
            continue;
        }
        merged.push((start, end));
    }
    merged
}

fn leading_ws_byte_len(line: &str) -> usize {
    line.char_indices()
        .find_map(|(idx, ch)| if ch.is_whitespace() { None } else { Some(idx) })
        .unwrap_or(line.len())
}

fn normalize_statement_lines(lines: Vec<String>) -> String {
    let Some(first_non_empty) = lines.iter().position(|line| !line.trim().is_empty()) else {
        return String::new();
    };
    let Some(last_non_empty) = lines.iter().rposition(|line| !line.trim().is_empty()) else {
        return String::new();
    };

    let slice = &lines[first_non_empty..=last_non_empty];
    let continuation_indent = slice
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            if idx == 0 || line.trim().is_empty() {
                return None;
            }
            Some(leading_ws_byte_len(line))
        })
        .min()
        .unwrap_or(0);

    let mut out = Vec::with_capacity(slice.len());
    for (idx, line) in slice.iter().enumerate() {
        let trimmed_end = line.trim_end_matches([' ', '\t']);
        if trimmed_end.trim().is_empty() {
            out.push(String::new());
            continue;
        }
        let drop = if idx == 0 {
            0
        } else {
            continuation_indent.min(leading_ws_byte_len(trimmed_end))
        };
        out.push(trimmed_end[drop..].to_string());
    }
    out.join("\n")
}

fn compact_statement_text(
    statement: tree_sitter::Node<'_>,
    text: &str,
    text_start_row: usize,
) -> String {
    let mut lines: Vec<String> = text.lines().map(|line| line.to_string()).collect();
    if lines.is_empty() {
        return String::new();
    }

    let mut ranges = Vec::new();
    collect_statement_elision_ranges(statement, &mut ranges);
    let merged_ranges = merge_line_ranges(ranges);

    for (start_row, end_row) in merged_ranges.into_iter().rev() {
        if end_row < text_start_row {
            continue;
        }
        let local_start = start_row.saturating_sub(text_start_row);
        if local_start >= lines.len() {
            continue;
        }
        let mut local_end = end_row.saturating_sub(text_start_row);
        if local_end >= lines.len() {
            local_end = lines.len() - 1;
        }
        if local_end < local_start {
            continue;
        }

        let indent_len = leading_ws_byte_len(&lines[local_start]);
        let indent = &lines[local_start][..indent_len];
        lines.splice(local_start..=local_end, [format!("{indent}/* ... */")]);
    }

    normalize_statement_lines(lines)
}

/// Split a `CutResult` into context lines, using `render` to produce the content
/// string for each real line. Cut markers and trailing empty lines become separators.
fn collect_context_lines(
    cut_result: &CutResult,
    render: impl Fn(&str, usize, usize) -> String,
) -> Vec<SourceContextLine> {
    let source = &cut_result.cut_source;

    let mut line_starts: Vec<usize> = vec![0];
    for (i, &b) in source.as_bytes().iter().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }

    let mut result: Vec<SourceContextLine> = Vec::with_capacity(line_starts.len());
    let mut skip_empty = false;

    for (line_idx, &line_start) in line_starts.iter().enumerate() {
        let line_end = line_starts
            .get(line_idx + 1)
            .map(|&s| s - 1)
            .unwrap_or(source.len());
        let line_text = &source[line_start..line_end];
        let line_num = cut_result.scope_range.start + line_idx as u32;

        if line_text.trim() == "/* ... */" {
            result.push(SourceContextLine::Separator(ContextSeparator {
                indent_cols: leading_indent_cols(line_text),
            }));
            skip_empty = true;
            continue;
        }

        if skip_empty && line_text.trim().is_empty() {
            continue;
        }
        skip_empty = false;

        let content = render(source, line_start, line_end);
        result.push(SourceContextLine::Line(ContextCodeLine {
            line_num,
            html: content,
        }));
    }

    result
}

/// Convert a `CutResult` into context lines with arborium syntax-highlighted HTML content.
pub fn highlighted_context_lines(
    cut_result: &CutResult,
    lang_name: &str,
) -> Vec<SourceContextLine> {
    let source = &cut_result.cut_source;
    let spans: Vec<Span> = arborium::Highlighter::new()
        .highlight_spans(lang_name, source)
        .unwrap_or_default();
    let format = HtmlFormat::CustomElements;

    collect_context_lines(cut_result, |src, line_start, line_end| {
        let line_text = &src[line_start..line_end];
        let line_spans: Vec<Span> = spans
            .iter()
            .filter(|s| (s.start as usize) < line_end && (s.end as usize) > line_start)
            .map(|s| Span {
                start: (s.start as usize).saturating_sub(line_start) as u32,
                end: ((s.end as usize).min(line_end) - line_start) as u32,
                capture: s.capture.clone(),
                pattern_index: s.pattern_index,
            })
            .collect();
        spans_to_html(line_text, line_spans, &format)
    })
}

/// Convert a `CutResult` into context lines with plain text content (no highlighting).
pub fn text_context_lines(cut_result: &CutResult) -> Vec<SourceContextLine> {
    collect_context_lines(cut_result, |src, line_start, line_end| {
        src[line_start..line_end].to_string()
    })
}

fn leading_indent_cols(text: &str) -> u32 {
    let mut cols = 0u32;
    for ch in text.chars() {
        match ch {
            ' ' => cols += 1,
            '\t' => cols += 4,
            _ => break,
        }
    }
    cols
}

#[cfg(test)]
mod tests;
