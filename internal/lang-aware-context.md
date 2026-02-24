# Language-aware source context

## Problem

In moire's graph view, backtrace frames show a source line. Often the async
state machine's instruction pointer lands on `.await`, `.await?;`, `}`, or `};`
— positions that provide no useful context. The expanded backtrace panel
(SourcePreview) shows surrounding lines which helps, but the graph card view
needs something smarter.

Showing a fixed line range around the target doesn't work either — a function
body might be 200 lines, and the interesting context is the containing statement
plus its immediate neighbors, not an arbitrary window.

## Approach: cut before highlight

The key insight: modify the source text _before_ syntax highlighting. This
avoids the painful problem of adjusting byte ranges within highlighted HTML.

Pipeline:

```
source text + tree-sitter AST + target position
  → identify containing scope (function/closure/impl block)
  → identify which child nodes to keep vs cut
  → replace cut regions with empty lines (preserving line numbering)
  → place a `/* ... */` marker on the first line of each cut region
  → syntax-highlight the modified source with arborium
  → send to frontend
```

### Preserving line numbers

When a region spanning lines 15–40 is cut, those 26 lines are replaced with:
- Line 15: `/* ... */`
- Lines 16–40: empty

This means line 47 in the output is still line 47 in the original file. The
frontend can display correct line numbers in the gutter without a separate
mapping table.

### Frontend rendering

The frontend iterates over the highlighted lines. When it encounters a run of
consecutive empty lines (preceded by a `/* ... */` marker), it collapses them
into a single visual separator showing the cut range. Non-empty lines render
normally with their original line numbers.

## Tree surgery details

### Step 1: find the containing scope

Walk up from the target node to find the nearest:
- `function_item`
- `closure_expression`
- `impl_item` (if the target is at module level)
- `source_file` (last resort)

This gives us the outer boundary of the context.

### Step 2: identify children to keep

Within the scope's body block, iterate the direct child statements/items. Each
child is a tree-sitter node with a byte range. Classify each child as:

- **target**: contains the target position → always keep
- **neighbor**: within N statements of the target → keep
- **scope header**: the function signature, `impl` header → always keep
- **cut**: everything else → replace with empty lines + marker

N = 1 or 2 seems right (one or two sibling statements on each side).

### Step 3: build the cut source

Operate on the original source bytes:

1. Sort the "cut" regions by byte offset
2. For each cut region, replace all bytes with spaces/newlines that preserve the
   original line count, placing `/* ... */` on the first line
3. Regions that are single-line can be cut at the node level (not the whole
   line), e.g. in `let x = match y { 1 => a, 2 => b, 3 => c }` we could cut
   individual match arms

For the initial implementation, cutting at statement granularity (whole child
nodes) is sufficient. Sub-statement cutting (match arms, etc.) is a refinement.

### Step 4: highlight and return

Run `arborium::Highlighter::highlight(lang, &cut_source)` on the modified
source. The tree-sitter parse inside arborium will see the cut markers as
regular comments — they'll get comment highlighting, which is visually
appropriate for a "this was redacted" indicator.

## API shape

The `SourcePreviewResponse` currently returns:

```rust
pub struct SourcePreviewResponse {
    pub frame_id: FrameId,
    pub source_file: String,
    pub target_line: u32,
    pub target_col: Option<u32>,
    pub display_range: Option<LineRange>,
    pub total_lines: u32,
    pub html: String,
}
```

With this feature, the response gains a second HTML field (or replaces the
semantics of `html`). Options:

**Option A: separate field**

```rust
pub struct SourcePreviewResponse {
    // ... existing fields ...
    pub html: String,                   // full file, highlighted
    pub context_html: Option<String>,   // cut source, highlighted
    pub context_range: Option<LineRange>, // line range of the containing scope
}
```

The frontend uses `context_html` for the graph card (compact + expanded) and
`html` for the full SourcePreview panel. `context_range` tells the frontend
which lines the context covers (so it knows the gutter start number).

**Option B: only return context**

Stop returning the full file HTML. The graph card uses context, and the
expanded backtrace panel fetches a separate endpoint or gets the full file
on demand. This avoids sending the entire highlighted file for every frame
when only a small window is needed.

Option B is better long-term (less data over the wire, faster response) but
Option A is simpler to ship incrementally.

## What this replaces

The current `display_range: Option<LineRange>` field and the corresponding
`find_display_range()` function become unnecessary once this ships. The
context HTML subsumes their purpose — the graph card renders the cut source
directly instead of joining a range of raw lines.

## Open questions

- **Scope nesting**: if the target is inside a closure inside a function, do we
  show the closure body or the outer function? Probably the innermost scope that
  has more than one statement.
- **Non-Rust languages**: the initial implementation targets Rust node kinds.
  Other languages need their own scope/statement kind lists. The tree-sitter
  grammar structure varies significantly.
- **Cache invalidation**: the context depends on the source file content at
  read time. If the file changes between requests, the cache is stale. This is
  already true for the current full-file approach — no new problem.
- **Large functions**: a function with 500 statements, where we keep 3–5,
  produces a context with many cut markers. This should be fine visually (one
  separator per cut region) but worth verifying.
- **Performance**: tree-sitter parse + cut + re-highlight per frame. The parse
  is fast (~1ms for typical files). The highlight is already happening. The cut
  operation is trivial string manipulation. Should be fine, but if it's not,
  we cache per (file_path, target_line).

## Implementation order

1. Write `cut_source(content: &str, lang: Language, target_line: u32, target_col: Option<u32>) -> CutResult`
   as a pure function, testable in isolation. `CutResult` contains the modified
   source string and the scope's line range.
2. Wire into `lookup_source_in_db` — highlight the cut source, return as
   `context_html`.
3. Frontend: render `context_html` in graph cards (collapsed: first meaningful
   line; expanded: multi-line with cut markers collapsed).
4. Remove `display_range` / `find_display_range()`.
