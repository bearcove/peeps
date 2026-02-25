use arborium::tree_sitter;
use moire_types::LineRange;

pub struct CutResult {
    /// The scope excerpt with cuts applied. Only contains lines from scope_range.
    /// Within that range, cut regions have their first line replaced with `/* ... */`
    /// and remaining lines empty. Line count = scope_range.end - scope_range.start + 1.
    pub cut_source: String,
    /// 1-based inclusive line range of the containing scope in the original file.
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

/// Number of sibling statements to keep on each side of the target.
const NEIGHBOR_COUNT: usize = 1;

/// Given source content, a language name, and a target position, find the
/// containing scope, classify its children into keep/cut, and return the
/// modified source with `/* ... */` placeholders for cut regions.
///
/// The returned `cut_source` preserves the same number of lines as the
/// scope in the original file (scope_range.end - scope_range.start + 1),
/// making line numbers stable for gutter rendering.
pub fn cut_source(
    content: &str,
    lang_name: &str,
    target_line: u32,
    target_col: Option<u32>,
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
    let body_children: Vec<tree_sitter::Node> = if is_source_file {
        // For source_file, all named children are body children
        (0..scope.named_child_count())
            .filter_map(|i| scope.named_child(i))
            .collect()
    } else {
        // Find the body node (block or declaration_list)
        let body = (0..scope.child_count())
            .filter_map(|i| scope.child(i))
            .find(|c| c.kind() == body_kind)?;
        (0..body.named_child_count())
            .filter_map(|i| body.named_child(i))
            .collect()
    };

    // If few enough children, no cutting needed — just return the scope as-is
    if body_children.len() <= (NEIGHBOR_COUNT * 2 + 1) {
        let scope_start = scope.start_position().row as u32 + 1;
        let scope_end = scope.end_position().row as u32 + 1;
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
    let keep_start = target_idx.saturating_sub(NEIGHBOR_COUNT);
    let keep_end = (target_idx + NEIGHBOR_COUNT + 1).min(body_children.len());

    // Build the scope range
    let scope_start_row = scope.start_position().row; // 0-based
    let scope_end_row = scope.end_position().row; // 0-based

    // Build cut_source by processing lines
    let mut result_lines: Vec<String> =
        Vec::with_capacity((scope_end_row - scope_start_row + 1) as usize);

    // Determine which line ranges to cut (0-based rows)
    let mut cut_ranges: Vec<(usize, usize)> = Vec::new(); // inclusive start/end rows

    // Lines before first body child but after scope header: keep (scope header)
    // Lines of cut children: replace
    // Lines of kept children: keep
    // Lines after last body child but before scope end: keep (closing brace)

    if !body_children.is_empty() {
        // Before the keep range: cut children [0..keep_start)
        if keep_start > 0 {
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
        if keep_end < body_children.len() {
            let cut_start = body_children[keep_end].start_position().row;
            let cut_end = body_children[body_children.len() - 1].end_position().row;
            if cut_end >= cut_start {
                cut_ranges.push((cut_start, cut_end));
            }
        }
    }

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

/// Extract the target statement and collapse it to a single line.
///
/// Finds the statement containing the target position, extracts its full text,
/// and collapses whitespace so a multi-line `let x = foo\n    .await;` becomes
/// `let x = foo .await;`.
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

    // Skip leading attribute children so compact view doesn't show
    // `#[moire::instrument] pub async fn foo()` — just `pub async fn foo()`.
    let text_start = {
        let mut start = statement.start_byte();
        for i in 0..statement.child_count() {
            if let Some(child) = statement.child(i) {
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
        start
    };

    let text = &content[text_start..statement.end_byte()];

    let collapsed = collapse_whitespace(text);
    if collapsed.is_empty() {
        return None;
    }

    Some(collapsed)
}

/// Collapse whitespace between tokens, suppressing the space where it would
/// look wrong in a single-line rendering:
///   - before `.` `;` `?` `)` `]` `>` `,` `:`
///   - after  `(` `[` `<` `.`
fn collapse_whitespace(text: &str) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(text.len());
    out.push_str(tokens[0]);

    for window in tokens.windows(2) {
        let prev = window[0];
        let next = window[1];

        let suppress = next.starts_with('.')
            || next.starts_with(';')
            || next.starts_with('?')
            || next.starts_with(')')
            || next.starts_with(']')
            || next.starts_with(',')
            || prev.ends_with('(')
            || prev.ends_with('[')
            || prev.ends_with('.');

        if !suppress {
            out.push(' ');
        }
        out.push_str(next);
    }

    out
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
        assert_eq!(result.scope_range.start, 1);
        assert_eq!(result.scope_range.end, 12);
        assert_line_count_invariant(src, &result);

        // The cut_source should contain `/* ... */` for cut regions
        let lines: Vec<&str> = result.cut_source.lines().collect();
        assert!(
            lines.iter().any(|l| l.trim() == "/* ... */"),
            "expected cut marker in output"
        );
        // Target line should be preserved
        assert!(
            lines.iter().any(|l| l.contains("let target = 5")),
            "target line should be preserved"
        );
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
        // Method chains should not have spaces before `.`
        assert_eq!(
            result,
            "let handle = session.establish_as_acceptor(self.root_settings, self.metadata).await?;"
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
        // pointing at `async fn recv(&mut self)`), we should get the full
        // signature collapsed, NOT just `(&mut self)`.
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
            !result.contains('\n'),
            "should be collapsed to one line: {result}"
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
        assert_eq!(result.scope_range.start, 1); // outer function
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
        assert!(
            result.scope_range.start <= 7 && result.scope_range.end >= 9,
            "scope should cover short_fn (lines 7-9), got {:?}",
            result.scope_range
        );
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
        assert_eq!(result.scope_range.start, 1);
        assert_eq!(result.scope_range.end, total_lines);
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
        assert_eq!(result.scope_range.start, 1, "should walk up to outer fn");
        assert_line_count_invariant(src, &result);
    }
}
