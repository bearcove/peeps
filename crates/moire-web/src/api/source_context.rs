use arborium::tree_sitter;
use moire_types::LineRange;

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

    // If few enough children, no cutting needed — just return the scope as-is
    if body_children.len() <= (neighbor_count * 2 + 1) {
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

    if is_function_scope {
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
/// If the innermost scope has only 1 statement, walk up further —
/// UNLESS the target is not inside any body child (e.g. it's on the
/// opening/closing brace), in which case this scope is the right one.
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
            if child_count <= 1 && current.kind() != "source_file" && !on_closing_brace {
                // Too few statements, keep looking for outer scope
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
/// - Aggressively elides long block interiors as `/* ... */` to keep cards compact.
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

fn strip_leading_visibility(signature: &str) -> &str {
    let trimmed = signature.trim_start();
    let Some(rest) = trimmed.strip_prefix("pub") else {
        return trimmed;
    };

    if rest.starts_with(char::is_whitespace) {
        return rest.trim_start();
    }

    if !rest.starts_with('(') {
        return trimmed;
    }

    let mut depth = 0usize;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return trimmed;
                }
                depth -= 1;
                if depth == 0 {
                    return rest[idx + 1..].trim_start();
                }
            }
            _ => {}
        }
    }

    trimmed
}

fn extract_function_signature_text(
    content: &str,
    fn_node: &tree_sitter::Node<'_>,
) -> Option<String> {
    let (start, end) = {
        let mut start = fn_node.start_byte();
        for i in 0..fn_node.child_count() {
            if let Some(child) = fn_node.child(i) {
                if child.kind() == "attribute_item"
                    || child.kind() == "attribute"
                    || child.kind() == "attributes"
                {
                    continue;
                }
                start = child.start_byte();
                break;
            }
        }
        let end = fn_node
            .child_by_field_name("body")
            .map(|body| body.start_byte())
            .unwrap_or_else(|| fn_node.end_byte());
        (start, end)
    };

    if end <= start {
        return None;
    }
    let raw = &content[start..end];
    let collapsed = collapse_ws_inline(raw);
    let signature = strip_leading_visibility(&collapsed);
    if signature.is_empty() {
        return None;
    }
    Some(signature.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Dump tree-sitter node structure for a parsed statement. Call from any
    /// test when debugging — e.g. `dump_node_tree(src, "rust", 3, Some(0));`
    #[allow(dead_code)]
    fn dump_node_tree(content: &str, lang_name: &str, target_line: u32, target_col: Option<u32>) {
        let ts_lang = arborium::get_language(lang_name).unwrap();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse(content.as_bytes(), None).unwrap();
        let row = (target_line - 1) as usize;
        let col = target_col.unwrap_or(0) as usize;
        let point = tree_sitter::Point::new(row, col);
        let node = tree
            .root_node()
            .named_descendant_for_point_range(point, point)
            .unwrap();

        fn print_node(node: tree_sitter::Node, content: &[u8], depth: usize) {
            let indent = "  ".repeat(depth);
            let text = node.utf8_text(content).unwrap_or("<err>");
            let short = if text.len() > 50 {
                format!("{}...", &text[..50])
            } else {
                text.to_string()
            };
            eprintln!(
                "{indent}{:?} named={} rows={}..{} text={short:?}",
                node.kind(),
                node.is_named(),
                node.start_position().row,
                node.end_position().row,
            );
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    print_node(child, content, depth + 1);
                }
            }
        }

        // Walk up to root, then print from there
        let mut current = node;
        while let Some(parent) = current.parent() {
            current = parent;
        }
        print_node(current, content.as_bytes(), 0);
    }

    fn assert_line_count_invariant(_content: &str, result: &CutResult) {
        let expected = (result.scope_range.end - result.scope_range.start + 1) as usize;
        let actual = result.cut_source.lines().count();
        // lines() doesn't count a trailing empty line, so check with +1 tolerance
        // when the source ends with \n
        assert!(
            actual == expected || (result.cut_source.ends_with('\n') && actual + 1 == expected),
            "line count mismatch: expected {expected}, got {actual}\ncut_source:\n{}",
            result.cut_source
        );
    }

    #[test]
    fn function_with_many_statements() {
        let src = r#"fn example() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let target = 5;
    let f = 6;
    let g = 7;
    let h = 8;
    let i = 9;
    let j = 10;
}"#;
        // target_line = 6 (1-based), the `let target = 5;` line
        let result = cut_source(src, "rust", 6, None).expect("should find scope");
        assert_eq!(result.scope_range.start, 5);
        assert_eq!(result.scope_range.end, 7);
        assert_line_count_invariant(src, &result);

        // Function context should be trimmed to the kept statement neighborhood.
        let lines: Vec<&str> = result.cut_source.lines().collect();
        assert!(
            !result.cut_source.contains("fn example"),
            "function signature should be omitted from function context"
        );
        assert!(
            lines.iter().any(|l| l.contains("let target = 5")),
            "target line should be preserved"
        );
        assert!(
            !lines.iter().any(|l| l.trim() == "/* ... */"),
            "function-window slicing should not emit outer cut markers"
        );
    }

    #[test]
    fn compact_cut_source_keeps_only_target_statement_neighbors() {
        let src = r#"fn example() {
    let a = 1;
    let b = 2;
    let target = 3;
    let d = 4;
    let e = 5;
}"#;
        let result = cut_source_compact(src, "rust", 4, None).expect("should find scope");
        let lines: Vec<&str> = result.cut_source.lines().collect();
        assert_eq!(result.scope_range.start, 4);
        assert_eq!(result.scope_range.end, 4);
        assert!(
            lines.iter().any(|l| l.contains("let target = 3")),
            "target line should be preserved"
        );
        assert!(
            !lines.iter().any(|l| l.trim() == "/* ... */"),
            "function-window compact mode should not emit outer cut markers"
        );
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn compact_cut_source_redacts_async_move_body_in_kept_statement() {
        let src = r#"pub async fn run() -> Result<(), String> {
    let setup = 1;
    spawn(async move {
        println!("line 1");
        println!("line 2");
        println!("line 3");
        println!("line 4");
        println!("line 5");
    })
    .named("bounded_sender");
    let teardown = 2;
}"#;

        let result = cut_source_compact(src, "rust", 3, None).expect("should find scope");
        assert!(
            result.cut_source.contains("spawn(async move {"),
            "kept statement header should remain"
        );
        assert!(
            result.cut_source.contains("/* ... */"),
            "async move body interior should be redacted in compact mode"
        );
        assert!(
            !result.cut_source.contains("println!(\"line 5\")"),
            "deep closure interior should not be present"
        );
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn cut_source_omits_function_signature_from_context() {
        let src = r#"pub async fn run(
    session: &mut Session,
    permit: Permit,
) -> Result<(), String> {
    let setup = 1;
    let before = 2;
    moire::task::spawn(
        async move {
            work().await;
        },
    )
    .named("permit_waiter");
    let after = 3;
    Ok(())
}"#;
        let result = cut_source(src, "rust", 12, None).expect("should find scope");
        assert_eq!(
            result.scope_range.start, 6,
            "context should start at body, not fn signature"
        );
        assert_eq!(
            result.scope_range.end, 13,
            "context should end before function brace"
        );
        assert!(
            !result.cut_source.contains("pub async fn run"),
            "function signature must be omitted from cut context"
        );
        assert!(
            !result.cut_source.contains("\n}"),
            "function closing brace must be omitted from cut context"
        );
        assert!(
            result.cut_source.contains(".named(\"permit_waiter\")"),
            "target statement should still be present"
        );
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn cut_source_compact_omits_function_signature_from_context() {
        let src = r#"pub async fn run(
    session: &mut Session,
    permit: Permit,
) -> Result<(), String> {
    let setup = 1;
    let before = 2;
    moire::task::spawn(
        async move {
            work().await;
        },
    )
    .named("permit_waiter");
    let after = 3;
    Ok(())
}"#;
        let result = cut_source_compact(src, "rust", 12, None).expect("should find scope");
        assert_eq!(
            result.scope_range.start, 7,
            "compact context should start at body"
        );
        assert_eq!(
            result.scope_range.end, 12,
            "compact context should end at kept statement"
        );
        assert!(
            !result.cut_source.contains("pub async fn run"),
            "function signature must be omitted from compact cut context"
        );
        assert!(
            !result.cut_source.contains("\n}"),
            "function closing brace must be omitted from compact cut context"
        );
        assert!(
            result.cut_source.contains(".named(\"permit_waiter\")"),
            "target statement should still be present in compact context"
        );
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn function_with_few_statements_no_cuts() {
        let src = r#"fn small() {
    let a = 1;
    let b = 2;
}"#;
        let result = cut_source(src, "rust", 2, None).expect("should find scope");
        // With only 2 statements and NEIGHBOR_COUNT=2, no cuts needed
        assert!(!result.cut_source.contains("/* ... */"));
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn closure_inside_function() {
        let src = r#"fn outer() {
    let x = 1;
    let closure = || {
        let a = 10;
        let b = 20;
        let c = 30;
        let target = 40;
        let e = 50;
        let f = 60;
        let g = 70;
    };
    let y = 2;
}"#;
        // target_line = 7 (1-based) — inside closure with >1 statement
        let result = cut_source(src, "rust", 7, None).expect("should find scope");
        // The scope should be the closure, which has enough children
        assert!(result.cut_source.contains("let target = 40"));
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn impl_block_with_methods() {
        let src = r#"struct Foo;

impl Foo {
    fn method_a(&self) {}
    fn method_b(&self) {}
    fn method_c(&self) {}
    fn target_method(&self) {
        println!("target");
    }
    fn method_d(&self) {}
    fn method_e(&self) {}
    fn method_f(&self) {}
    fn method_g(&self) {}
}"#;
        // target_line = 8 (the println line inside target_method)
        let result = cut_source(src, "rust", 8, None).expect("should find scope");
        assert!(result.cut_source.contains("target"));
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn target_on_await_line() {
        let src = r#"async fn do_work() {
    let a = setup().await;
    let b = step_one().await;
    let c = step_two().await;
    let result = main_work()
        .await;
    let d = cleanup().await;
    let e = finalize().await;
}"#;
        // target_line = 6 (the `.await` continuation)
        let result = cut_source(src, "rust", 6, None).expect("should find scope");
        assert!(result.cut_source.contains(".await"));
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn extract_multi_line_await_statement() {
        let src = r#"async fn do_work() {
    let handle = session
        .establish_as_acceptor(self.root_settings, self.metadata)
        .await?;
}"#;
        // target on the .await line (line 3, 0-indexed row 2)
        let result = extract_target_statement(src, "rust", 3, None).expect("should find statement");
        assert!(
            result.contains('\n'),
            "should preserve statement structure, got: {result}"
        );
        assert!(
            result.contains("let handle = session"),
            "should include let binding head, got: {result}"
        );
        assert!(
            result.contains(".await?;"),
            "should include await tail, got: {result}"
        );
    }

    #[test]
    fn extract_simple_statement() {
        let src = r#"fn foo() {
    let x = 42;
}"#;
        let result = extract_target_statement(src, "rust", 2, None).expect("should find statement");
        assert_eq!(result, "let x = 42;");
    }

    #[test]
    fn extract_target_on_fn_signature_line() {
        // When the target is the fn signature line itself (e.g. from a backtrace
        // pointing at `async fn recv(&mut self)`), we should still get the
        // signature (not just `(&mut self)`), with an aggressively collapsed body.
        let src = r#"impl Session {
    async fn recv(&mut self) -> Result<Option<SelfRef<Msg<'static>>>, Self::Error> {
        let backing = match self
            .link_rx
            .recv()
            .await;
    }
}"#;
        // target_line = 2 (the `async fn recv` line), col 0
        let result =
            extract_target_statement(src, "rust", 2, Some(0)).expect("should find statement");
        assert!(
            result.contains("async fn recv"),
            "should contain fn signature, got: {result}"
        );
        assert!(
            result.contains("/* ... */"),
            "function body should be elided aggressively, got: {result}"
        );
    }

    #[test]
    fn extract_long_match_statement_elides_body() {
        let src = r#"fn process(y: Option<u32>) {
    let x = match y {
        Some(1) => 1,
        Some(2) => 2,
        Some(3) => 3,
        Some(4) => 4,
        Some(5) => 5,
        Some(6) => 6,
        Some(7) => 7,
        Some(8) => 8,
        _ => 0,
    };
    println!("{x}");
}"#;
        let result =
            extract_target_statement(src, "rust", 2, Some(12)).expect("should find statement");
        assert!(
            result.contains("let x = match y {"),
            "should include statement head, got: {result}"
        );
        assert!(
            result.contains("/* ... */"),
            "should elide long block body, got: {result}"
        );
        assert!(
            result.contains("};"),
            "should include statement tail, got: {result}"
        );
    }

    #[test]
    fn extract_statement_dedents_continuation_lines() {
        let src = r#"fn run() {
    spawn(async move {
        println!("hello");
        let _ = idle_rx.recv().await;
    })
    .named("blocked_receiver");
}"#;
        let result =
            extract_target_statement(src, "rust", 2, Some(4)).expect("should find statement");
        assert!(
            result.starts_with("spawn(async move {"),
            "head should be left-anchored, got: {result}"
        );
        assert!(
            result.contains("\n    println!(\"hello\");"),
            "body lines should be dedented to one continuation level, got: {result}"
        );
        assert!(
            result.contains("\n.named(\"blocked_receiver\");"),
            "trailing chain should be dedented, got: {result}"
        );
    }

    #[test]
    fn extract_strips_attributes() {
        let src = r#"impl Session {
    #[moire::instrument]
    pub async fn establish(self) -> Result<(Session<C>, ConnectionHandle), SessionError> {
        let (mut server_session, server_handle) = acceptor(server_conduit)
            .establish()
            .await
            .expect("server");
    }
}"#;
        // target_line = 3 (the fn line)
        let result =
            extract_target_statement(src, "rust", 3, Some(4)).expect("should find statement");
        assert!(
            !result.contains("moire::instrument"),
            "should strip attributes, got: {result}"
        );
        assert!(
            result.contains("pub async fn establish"),
            "should keep fn signature, got: {result}"
        );
    }

    #[test]
    fn extract_strips_multiple_attributes() {
        let src = r#"#[allow(dead_code)]
#[moire::instrument]
async fn recv(&mut self) -> Result<Option<Msg>> {
    self.rx.recv().await
}"#;
        let result =
            extract_target_statement(src, "rust", 3, Some(0)).expect("should find statement");
        assert!(
            !result.contains("moire::instrument"),
            "should strip moire attr, got: {result}"
        );
        assert!(
            !result.contains("allow"),
            "should strip all attrs, got: {result}"
        );
        assert!(
            result.contains("async fn recv"),
            "should keep fn signature, got: {result}"
        );
    }

    #[test]
    fn extract_target_on_fn_keyword() {
        // Target pointing at the `fn` keyword specifically
        let src = r#"fn do_stuff(x: u32, y: u32) -> bool {
    let z = x + y;
    z > 10
}"#;
        // target_line = 1, col at `fn` keyword
        let result =
            extract_target_statement(src, "rust", 1, Some(0)).expect("should find statement");
        assert!(
            result.contains("fn do_stuff"),
            "should contain fn signature, got: {result}"
        );
    }

    #[test]
    fn extract_target_on_async_fn_with_multiline_params() {
        let src = r#"async fn establish_as_acceptor(
    &mut self,
    settings: ConnectionSettings,
    metadata: Metadata<'_>,
) -> Result<Handle> {
    let handle = session
        .establish(self.root_settings, self.metadata)
        .await?;
    Ok(handle)
}"#;
        // target_line = 1 (the `async fn` line)
        let result =
            extract_target_statement(src, "rust", 1, Some(0)).expect("should find statement");
        assert!(
            result.contains("async fn establish_as_acceptor"),
            "should contain fn name, got: {result}"
        );
    }

    #[test]
    fn extract_match_expression() {
        let src = r#"fn process() {
    let x = 1;
    match self.rx.recv().await {
        Ok(Some(msg)) => {
            let payload = msg.map(|m| m.payload);
            handle(payload);
        }
        Ok(None) => {}
        Err(e) => return Err(e),
    }
    let y = 2;
}"#;
        // target on the match line
        let result =
            extract_target_statement(src, "rust", 3, Some(4)).expect("should find statement");
        assert!(
            result.contains("match"),
            "should contain match keyword, got: {result}"
        );
    }

    // ── extract_enclosing_fn tests ────────────────────────────────────────────

    #[test]
    fn enclosing_fn_free_function_no_params() {
        let src = r#"fn run() {
    let a = 1;
    spawn(async move { a });
}"#;
        let result = extract_enclosing_fn(src, "rust", 3, None).expect("should find fn");
        assert_eq!(result, "fn run()");
    }

    #[test]
    fn enclosing_fn_impl_method_with_self() {
        let src = r#"struct Foo;
impl Foo {
    pub async fn run(&self) {
        let a = 1;
        spawn(async move { a });
    }
}"#;
        let result = extract_enclosing_fn(src, "rust", 5, None).expect("should find fn");
        assert_eq!(result, "Foo::async fn run(&self)");
    }

    #[test]
    fn enclosing_fn_params_truncated_at_3() {
        let src = r#"fn process(a: u32, b: u32, c: u32, d: u32) {
    spawn(async move { a });
}"#;
        let result = extract_enclosing_fn(src, "rust", 2, None).expect("should find fn");
        assert_eq!(result, "fn process(a: u32, b: u32, c: u32, d: u32)");
    }

    #[test]
    fn enclosing_fn_exactly_3_params_no_truncation() {
        let src = r#"fn process(a: u32, b: u32, c: u32) {
    spawn(async move { a });
}"#;
        let result = extract_enclosing_fn(src, "rust", 2, None).expect("should find fn");
        assert_eq!(result, "fn process(a: u32, b: u32, c: u32)");
    }

    #[test]
    fn enclosing_fn_inside_closure_returns_outer() {
        let src = r#"fn run() {
    let v: Vec<u32> = vec![1, 2, 3];
    v.iter().for_each(|x| {
        spawn(async move { *x });
    });
}"#;
        let result = extract_enclosing_fn(src, "rust", 4, None).expect("should find fn");
        assert_eq!(result, "fn run()");
    }

    #[test]
    fn enclosing_fn_generic_impl() {
        let src = r#"struct Queue<T>;
impl<T: Send> Queue<T> {
    fn push(&mut self, value: T) {
        spawn(async move { value });
    }
}"#;
        let result = extract_enclosing_fn(src, "rust", 4, None).expect("should find fn");
        assert_eq!(result, "Queue<T>::fn push(&mut self, value: T)");
    }

    #[test]
    fn enclosing_fn_non_rust_returns_none() {
        let src = "function run() { spawn(async () => {}); }";
        let result = extract_enclosing_fn(src, "javascript", 1, None);
        assert!(result.is_none(), "non-rust should return None");
    }

    #[test]
    fn enclosing_fn_self_mut_ref() {
        let src = r#"impl Handler {
    async fn handle(&mut self, req: Request, ctx: Context) {
        spawn(async move { req });
    }
}"#;
        let result = extract_enclosing_fn(src, "rust", 3, None).expect("should find fn");
        assert_eq!(
            result,
            "Handler::async fn handle(&mut self, req: Request, ctx: Context)"
        );
    }

    #[test]
    fn enclosing_fn_includes_module_and_return_type() {
        let src = r#"mod demo {
    impl Worker {
        pub fn run(&self) -> Result<(), String> {
            do_stuff();
        }
    }
}"#;
        let result = extract_enclosing_fn(src, "rust", 4, None).expect("should find fn");
        assert_eq!(result, "demo::Worker::fn run(&self) -> Result<(), String>");
    }

    #[test]
    fn enclosing_fn_strips_pub_crate_visibility() {
        let src = r#"impl Worker {
    pub(crate) async fn run(&self) -> Result<(), String> {
        do_stuff();
    }
}"#;
        let result = extract_enclosing_fn(src, "rust", 3, None).expect("should find fn");
        assert_eq!(result, "Worker::async fn run(&self) -> Result<(), String>");
    }

    #[test]
    fn extract_channel_creation() {
        // Simulating the `let (tx_a, rx_b) = mpsc::channel(...)` case from the screenshot
        let src = r#"fn setup() {
    let (tx_a, rx_b) = mpsc::channel("memory_link.a→b", buffer);
    let (a, b) = memory_link_pair(64);
}"#;
        let result =
            extract_target_statement(src, "rust", 2, Some(4)).expect("should find statement");
        assert!(
            result.contains("mpsc::channel"),
            "should contain channel call, got: {result}"
        );
        assert!(
            result.contains("let (tx_a, rx_b)"),
            "should contain destructuring, got: {result}"
        );
    }

    #[test]
    fn single_statement_closure_walks_up() {
        let src = r#"fn outer() {
    let a = 1;
    let b = 2;
    let c = vec.iter().map(|x| {
        x + 1
    });
    let d = 3;
    let e = 4;
}"#;
        // target_line = 5 (inside closure with 1 statement)
        // Should walk up to outer function
        let result = cut_source(src, "rust", 5, None).expect("should find scope");
        assert_eq!(result.scope_range.start, 3); // outer function window around target
        assert_line_count_invariant(src, &result);
    }

    // --- Closing brace edge cases ---
    // When a backtrace points at the closing `}` of a function, we should
    // scope to that function — not walk up to source_file and capture
    // the entire file.

    #[test]
    fn closing_brace_of_empty_function_not_whole_file() {
        // `fn foo() {}` collapsed to two lines: header + brace
        let src = r#"fn unrelated_a() {
    do_stuff();
    do_more_stuff();
    even_more();
}

fn foo() {
}

fn unrelated_b() {
    something();
    something_else();
    and_more();
}"#;
        let total_lines = src.lines().count() as u32;
        // target = closing `}` of `foo`, line 8
        let result = cut_source(src, "rust", 8, None).expect("should find scope");
        assert_ne!(
            result.scope_range.end, total_lines,
            "must not capture entire file; got scope_range {:?}",
            result.scope_range
        );
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn closing_brace_of_single_stmt_function_not_whole_file() {
        let src = r#"fn unrelated_a() {
    do_stuff();
    do_more_stuff();
    even_more();
}

fn short_fn() {
    single_call()
}

fn unrelated_b() {
    something();
    something_else();
    and_more();
}"#;
        let total_lines = src.lines().count() as u32;
        // target = closing `}` of `short_fn`, line 9
        let result = cut_source(src, "rust", 9, None).expect("should find scope");
        assert_ne!(
            result.scope_range.end, total_lines,
            "must not capture entire file; got scope_range {:?}",
            result.scope_range
        );
        // Scope should contain the short_fn body
        assert_eq!(result.scope_range.start, 8);
        assert_eq!(result.scope_range.end, 8);
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn closing_brace_of_multi_stmt_function_stays_in_function() {
        let src = r#"fn many() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
}"#;
        let total_lines = src.lines().count() as u32;
        // target = closing `}` on last line
        let result = cut_source(src, "rust", total_lines, None).expect("should find scope");
        assert_eq!(result.scope_range.start, 5);
        assert_eq!(result.scope_range.end, 6);
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn closing_brace_of_method_in_impl_not_whole_file() {
        let src = r#"struct Foo;

impl Foo {
    fn method_a(&self) {
        do_a();
        do_b();
        do_c();
    }

    fn getter(&self) -> u32 {
        self.value
    }

    fn method_b(&self) {
        do_x();
        do_y();
        do_z();
    }
}"#;
        let total_lines = src.lines().count() as u32;
        // target = closing `}` of `getter`, line 12
        let result = cut_source(src, "rust", 12, None).expect("should find scope");
        assert_ne!(
            result.scope_range.end, total_lines,
            "must not capture entire file; got scope_range {:?}",
            result.scope_range
        );
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn closing_brace_of_closure_not_whole_file() {
        // Standalone closure assigned to a variable — 1 statement inside
        let src = r#"fn outer() {
    let a = 1;
    let b = 2;
    let handler = || {
        single_call()
    };
    let c = 3;
    let d = 4;
}"#;
        let total_lines = src.lines().count() as u32;
        // target = closing `}` of the closure, line 6
        let result = cut_source(src, "rust", 6, None).expect("should find scope");
        assert_ne!(
            result.scope_range.end, total_lines,
            "must not capture entire file; got scope_range {:?}",
            result.scope_range
        );
        assert_line_count_invariant(src, &result);
    }

    // Regression: walking up for a 1-stmt closure whose target is INSIDE
    // the body (not on the brace) should still walk up to outer scope.
    #[test]
    fn single_stmt_closure_target_inside_still_walks_up() {
        let src = r#"fn outer() {
    let a = 1;
    let b = 2;
    let c = vec.iter().map(|x| {
        x + 1
    });
    let d = 3;
    let e = 4;
}"#;
        // target_line = 5 — INSIDE the closure body, not on brace
        let result = cut_source(src, "rust", 5, None).expect("should find scope");
        assert_eq!(
            result.scope_range.start, 3,
            "should walk up to outer fn body"
        );
        assert_line_count_invariant(src, &result);
    }

    #[test]
    fn long_param_list_is_truncated_in_compact_cut_source() {
        // Simulate what the graph node sees: a function with many params and one
        // body statement, targeted at the fn declaration line.  find_scope walks
        // up to source_file, making the function_item a kept "body child" of the
        // file scope.  collect_compact_block_elision_ranges must elide the extra
        // parameters so the card stays compact.
        let src = r#"fn helper() {}

fn spawn_worker(
    task_name: &'static str,
    first_name: &'static str,
    first: u32,
    second_name: &'static str,
    second: u32,
    ready: bool,
    done_tx: u32,
) {
    let x = 1;
}

fn other() {}"#;
        // Target line 3: the `fn spawn_worker(` declaration.
        let result = cut_source_compact(src, "rust", 3, None).expect("should find scope");
        assert!(
            result.cut_source.contains("task_name"),
            "first param should be kept:\n{}",
            result.cut_source
        );
        assert!(
            result.cut_source.contains("/* ... */"),
            "remaining params should be elided:\n{}",
            result.cut_source
        );
        assert!(
            !result.cut_source.contains("second_name"),
            "non-first params should be gone:\n{}",
            result.cut_source
        );
        assert!(
            !result.cut_source.contains("done_tx"),
            "last param should be gone:\n{}",
            result.cut_source
        );
        assert_line_count_invariant(src, &result);
    }
}
