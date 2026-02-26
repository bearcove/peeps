use super::*;
use std::path::Path;

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

fn run_fixture(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    let lang = path.extension().and_then(|e| e.to_str()).unwrap_or("rust");

    // Find the 👉 marker to determine target line; strip it from source.
    let mut target_line: Option<u32> = None;
    let mut clean_lines: Vec<&str> = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        if let Some(rest) = line.strip_prefix("👉") {
            target_line = Some((i + 1) as u32);
            clean_lines.push(rest.strip_prefix(' ').unwrap_or(rest));
        } else {
            clean_lines.push(line);
        }
    }
    let source = clean_lines.join("\n");
    let target_line = target_line.ok_or("fixture missing '👉' target marker")?;

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("fixture");

    let header = extract_enclosing_fn(&source, lang, target_line, None).unwrap_or_default();

    let compact_body = match cut_source_compact(&source, lang, target_line, None) {
        Some(compact) => format!(
            "# scope {}..{}\n\n{}",
            compact.scope_range.start,
            compact.scope_range.end,
            format_context_lines(&text_context_lines(&compact))
        ),
        None => "(no result)".to_string(),
    };
    insta::assert_snapshot!(
        format!("{stem}.compact"),
        format!("# {header}\n{compact_body}")
    );

    let normal_body = match cut_source(&source, lang, target_line, None) {
        Some(normal) => format!(
            "# scope {}..{}\n\n{}",
            normal.scope_range.start,
            normal.scope_range.end,
            format_context_lines(&text_context_lines(&normal))
        ),
        None => "(no result)".to_string(),
    };
    insta::assert_snapshot!(
        format!("{stem}.normal"),
        format!("# {header}\n{normal_body}")
    );

    Ok(())
}

datatest_mini::harness! {
    { test = run_fixture, root = "tests/fixtures", pattern = r"^[^/]+\.(rs|js)$" },
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
