use arborium::theme::builtin;
use arborium_theme::highlights::HIGHLIGHTS;
use arborium_theme::theme::{Style, Theme};
use axum::http::header;
use axum::response::IntoResponse;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

/// Generates `--hl-{tag}: {hex};` variable declarations for a theme.
/// Only emits variables for tags that have a non-empty style.
fn theme_to_css_vars(theme: &Theme) -> String {
    let mut css = String::new();

    // Build tag -> style map for parent fallback
    let mut tag_to_style: HashMap<&str, &Style> = HashMap::new();
    for (i, def) in HIGHLIGHTS.iter().enumerate() {
        if !def.tag.is_empty() && !theme.styles[i].is_empty() {
            tag_to_style.insert(def.tag, &theme.styles[i]);
        }
    }

    let mut emitted: HashSet<&str> = HashSet::new();
    for (i, def) in HIGHLIGHTS.iter().enumerate() {
        if def.tag.is_empty() || emitted.contains(def.tag) {
            continue;
        }

        let style = if !theme.styles[i].is_empty() {
            &theme.styles[i]
        } else if !def.parent_tag.is_empty() {
            match tag_to_style.get(def.parent_tag) {
                Some(s) => s,
                None => continue,
            }
        } else {
            continue;
        };

        if style.is_empty() {
            continue;
        }

        emitted.insert(def.tag);

        if let Some(fg) = &style.fg {
            writeln!(css, "  --hl-{}: {};", def.tag, fg.to_hex()).unwrap();
        }
    }

    css
}

/// Generates the element rules that reference CSS variables.
/// These are theme-independent — emitted once.
fn element_rules() -> String {
    let mut css = String::new();
    let mut emitted: HashSet<&str> = HashSet::new();

    for def in HIGHLIGHTS.iter() {
        if def.tag.is_empty() || emitted.contains(def.tag) {
            continue;
        }
        emitted.insert(def.tag);
        writeln!(css, "a-{} {{ color: var(--hl-{}); }}", def.tag, def.tag).unwrap();
    }

    css
}

/// Serves arborium syntax highlighting CSS for both light and dark modes.
///
/// Emits CSS custom properties (`--hl-{tag}`) in `:root` for each theme,
/// then a single set of element rules referencing those variables.
/// This lets other UI styles reuse `--hl-keyword`, `--hl-function`, etc.
pub async fn api_arborium_theme_css() -> impl IntoResponse {
    let light = builtin::github_light();
    let dark = builtin::catppuccin_mocha();

    let mut css = String::new();

    // Light theme variables
    writeln!(css, ":root {{").unwrap();
    css.push_str(&theme_to_css_vars(&light));
    writeln!(css, "}}").unwrap();

    // Dark theme variables override
    writeln!(css, "@media (prefers-color-scheme: dark) {{").unwrap();
    writeln!(css, "  :root {{").unwrap();
    for line in theme_to_css_vars(&dark).lines() {
        writeln!(css, "  {line}").unwrap();
    }
    writeln!(css, "  }}").unwrap();
    writeln!(css, "}}").unwrap();

    // Element rules — emitted once, reference vars
    css.push('\n');
    css.push_str(&element_rules());

    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], css)
}
