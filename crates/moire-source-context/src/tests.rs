use super::*;

fn format_context_lines(lines: &[moire_types::SourceContextLine]) -> String {
    let mut out = String::new();
    for line in lines {
        match line {
            moire_types::SourceContextLine::Line(l) => {
                out.push_str(&format!("{:>4} | {}\n", l.line_num, l.html));
            }
            moire_types::SourceContextLine::Separator(_) => {
                out.push_str("     | ...\n");
            }
        }
    }
    out
}

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
    let result = extract_target_statement(src, "rust", 2, Some(0)).expect("should find statement");
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
    let result = extract_target_statement(src, "rust", 2, Some(12)).expect("should find statement");
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
    let result = extract_target_statement(src, "rust", 2, Some(4)).expect("should find statement");
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
    let result = extract_target_statement(src, "rust", 3, Some(4)).expect("should find statement");
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
    let result = extract_target_statement(src, "rust", 3, Some(0)).expect("should find statement");
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
    let result = extract_target_statement(src, "rust", 1, Some(0)).expect("should find statement");
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
    let result = extract_target_statement(src, "rust", 1, Some(0)).expect("should find statement");
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
    let result = extract_target_statement(src, "rust", 3, Some(4)).expect("should find statement");
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
    let result = extract_target_statement(src, "rust", 2, Some(4)).expect("should find statement");
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
fn long_param_list_is_truncated_in_compact_cut_source1() {
    // When target is the `fn` declaration line, find_scope stops at the
    // function_item (terminal scope), so the context is the function body —
    // the signature is the frame_header, not shown in the body.
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
    let header =
        extract_enclosing_fn(src, "rust", 3, None).unwrap_or_default();
    let compact = cut_source_compact(src, "rust", 3, None).expect("compact");
    assert_line_count_invariant(src, &compact);
    insta::assert_snapshot!(
        "fn_decl_target_compact",
        format!("# {header}\n\n{}", format_context_lines(&text_context_lines(&compact)))
    );
    let normal = cut_source(src, "rust", 3, None).expect("normal");
    assert_line_count_invariant(src, &normal);
    insta::assert_snapshot!(
        "fn_decl_target_normal",
        format!("# {header}\n\n{}", format_context_lines(&text_context_lines(&normal)))
    );
}

#[test]
fn long_param_list_is_truncated_in_compact_cut_source2() {
    let src = r#"fn helper() {}

fn spawn_lock_order_worker(
    task_name: &'static str,
    first_name: &'static str,
    first: Arc<SyncMutex<()>>,
    second_name: &'static str,
    second: Arc<SyncMutex<()>>,
    ready_barrier: Arc<Barrier>,
    completed_tx: oneshot::Sender<()>,
) {
    moire::task::spawn(async move {
        let _first_guard = first.lock();
        println!("{task_name} locked {first_name}; waiting for peer");

        ready_barrier.wait();

        println!(
            "{task_name} attempting {second_name}; this should deadlock due to lock-order inversion"
        );
        let _second_guard = second.lock();

        println!("{task_name} unexpectedly acquired {second_name}; deadlock did not occur");
        let _ = completed_tx.send(());
    }.named(task_name));
}

fn other() {}"#;

    // target: `.named(task_name)` line
    let target_line = 25;

    let header = extract_enclosing_fn(src, "rust", target_line, None).unwrap_or_default();

    let compact = cut_source_compact(src, "rust", target_line, None).expect("compact");
    insta::assert_snapshot!(
        "compact",
        format!("# {header}\n\n{}", format_context_lines(&text_context_lines(&compact)))
    );

    let normal = cut_source(src, "rust", target_line, None).expect("normal");
    insta::assert_snapshot!(
        "normal",
        format!("# {header}\n\n{}", format_context_lines(&text_context_lines(&normal)))
    );
}
