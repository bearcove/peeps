# Handoff: Source Preview on Graph Entity Cards

## Completed
- Added `frame_id?: number` to `RenderTopFrame` in `frontend/src/snapshot.ts:40`
- Wired `frame_id` through `resolveBacktraceDisplay()` using `findIndex` to get the right `record.frame_ids[topFrameIndex]`
- Added `showSource?: boolean` to `GraphFilterParseResult` in `frontend/src/graphFilter.ts`
- Parsing `source:on`/`source:off` tokens + suggestions in all 4 suggestion lists
- Added `sourceLine?: string` to `GraphNodeData` in `frontend/src/components/graph/GraphNode.tsx`
- Rendering source line as `<pre className="graph-node-source-line arborium-hl" dangerouslySetInnerHTML={{ __html: data.sourceLine }} />`
- CSS for `.graph-node-source-line` in `GraphNode.css` (mono, 10px, tertiary, truncated, max-width 360px)
- Created shared cache at `frontend/src/api/sourceCache.ts` (extracted from BacktraceRenderer)
- Updated `BacktraceRenderer.tsx` to import `cachedFetchSourcePreview` from shared cache
- `GraphPanel.tsx`: fetches source previews via `cachedFetchSourcePreview`, extracts target line via `splitHighlightedHtml`, wires into node data
- Layout re-triggers when source lines arrive (`measureGraphLayout` now takes `sourceLineByFrameId` param)
- Default filter in `App.tsx:160` includes `source:on`, fallback is `false` (removing chip disables)
- StorybookPage default filter also includes `source:on`
- Split arborium theme scoping: Rust `theme.rs` now scopes to `.arborium-hl` instead of `.ui-source-preview`
- `SourcePreview.tsx` now has both classes: `ui-source-preview arborium-hl`
- All 535 tests pass

## Active Work

### Origin
User provided a detailed implementation plan for "Source Preview on Graph Entity Cards" in the moire graph view. The feature shows syntax-highlighted source code lines on entity cards so you can see "what's stuck where" during deadlock debugging.

### The Problem
The feature works but needs polish. User's feedback from the screenshot (showing working cards with syntax-highlighted source):

1. **Kill the background** — The source line `<pre>` may still be picking up some background styling. The `.graph-node-source-line` has `margin: 0; padding: 0` but no explicit `background: none` (I removed it when splitting the classes). Check if the `<pre>` element default styles or the `.arborium-hl` theme adds a background.

2. **More spacing** — The source line is tight against the main label line. Needs a bit more gap in the `.graph-node-content` flex column or margin-top on `.graph-node-source-line`.

3. **Show more context** — Currently shows exactly 1 source line (the target line). User wants either: a couple more frames shown, OR more lines per frame. Need to pick a dimension and discuss with user. The current extraction is `lines[target_line - 1]` — could easily grab ±1 line.

4. **Show `file.rs:line` somewhere** — The first important frame's source location (`file.rs:42`) should appear on the card, probably top-right. This is separate from the sublabel which shows it based on `labelBy:location`. The source location should always show when `source:on` is active regardless of labelBy mode.

5. **What should the actions be?** — User asked this question but didn't elaborate before requesting the handoff. Likely means: what interactions should the source line support? Click to open in Zed? Click to expand backtrace? Right-click context menu?

### Current State
- Branch: `main` (changes are unstaged)
- No PR yet
- Working directory: `/Users/amos/bearcove/moire`
- Also touches: `/Users/amos/bearcove/moire/crates/moire-web/src/api/theme.rs` (Rust, needs rebuild)

### Technical Context

**Files modified:**
- `frontend/src/snapshot.ts:33-40` — `RenderTopFrame` has `frame_id?: number`
- `frontend/src/snapshot.ts:218-240` — `resolveBacktraceDisplay` uses `findIndex` instead of `find` to get frame index
- `frontend/src/graphFilter.ts` — `showSource` parsing + suggestions in 4 places (root suggestions ~line 533, keySuggestions ~line 577, value completion ~line 641, fallback ~line 698)
- `frontend/src/components/graph/GraphNode.tsx:21` — `sourceLine?: string` on GraphNodeData
- `frontend/src/components/graph/GraphNode.tsx:131-133` — renders `<pre className="graph-node-source-line arborium-hl">`
- `frontend/src/components/graph/GraphNode.css` — `.graph-node-source-line` styles
- `frontend/src/api/sourceCache.ts` — NEW file, shared cache
- `frontend/src/components/inspector/BacktraceRenderer.tsx` — imports from shared cache now
- `frontend/src/components/graph/GraphPanel.tsx` — source line fetching useEffect + wiring into nodesWithScopeColor memo. Source line state is declared BEFORE layout effect (hook ordering matters).
- `frontend/src/graph/render/NodeLayer.tsx:40-44` — `measureGraphLayout` takes optional `sourceLineByFrameId` param, passes `sourceLine` into `GraphNode` during measurement
- `frontend/src/App.tsx:277` — `effectiveShowSource = graphTextFilters.showSource ?? false`
- `frontend/src/App.tsx:160` — default filter includes `source:on`
- `frontend/src/App.tsx:1410` — passes `showSource={effectiveShowSource}` to GraphPanel
- `crates/moire-web/src/api/theme.rs:8-9` — scopes to `.arborium-hl` now
- `frontend/src/ui/primitives/SourcePreview.tsx:19` — has both `ui-source-preview arborium-hl`

**Source line extraction flow:**
1. `GraphPanel` collects unique `topFrame.frame_id` values from visible entities
2. Fetches via `cachedFetchSourcePreview(frameId)` — returns `SourcePreviewResponse` with `html` (full file) and `target_line` (1-indexed)
3. Uses `splitHighlightedHtml(res.html)` to split into per-line HTML strings
4. Takes `lines[target_line - 1]` — this is the syntax-highlighted HTML for the target line
5. Stores in `Map<number, string>` keyed by frame_id
6. Passed into `GraphNodeData.sourceLine` via the `nodesWithScopeColor` memo

**Pre-existing issue:** `pnpm typecheck` has 3 errors in `utils/highlightedHtml.ts` (TS2488 iterator errors on NamedNodeMap/NodeListOf). These are NOT from our changes — they exist on main too. `pnpm test` passes fine (535 tests).

### Success Criteria
1. Source lines render with syntax highlighting on graph cards ✅
2. `source:on`/`source:off` toggle works ✅
3. Layout re-measures when source lines load ✅
4. Background removed from source line area
5. Better spacing between main label and source line
6. Show `file.rs:line` on the card (top-right?) when source is on
7. Decide on more context: more lines per frame or more frames?
8. Decide on interactions/actions for the source line
9. All tests pass ✅
10. Backtrace panel still works with shared cache ✅

### Files to Touch
- `frontend/src/components/graph/GraphNode.css` — spacing and background fixes
- `frontend/src/components/graph/GraphNode.tsx` — add file:line display, possibly more lines, interactions
- `frontend/src/components/graph/GraphPanel.tsx` — may need to extract more lines or pass file:line info

### Decisions Made
- `source:on` defaults to false (removing the chip disables it), but it's in the default filter string
- Theme scoping split: `.arborium-hl` for syntax colors, `.ui-source-preview` for layout only
- Shared cache between BacktraceRenderer and GraphPanel avoids double-fetching
- HTML kept intact (not stripped to plain text) for syntax highlighting
- Source line rendered as `<pre>` to match arborium theme selectors

### What NOT to Do
- Don't strip HTML tags from source lines — user explicitly wants syntax highlighting
- Don't scope arborium theme back to `.ui-source-preview` — that caused spacing conflicts
- Don't forget hook ordering in GraphPanel — `sourceLineByFrameId` state must be declared before the layout useEffect that references it

### Blockers/Gotchas
- The Rust file `crates/moire-web/src/api/theme.rs` was changed — needs `cargo build` for the moire-web server to serve the updated CSS scope
- `splitHighlightedHtml` uses DOMParser — works in browser and vitest (jsdom) but not in pure Node
- The pre-existing typecheck errors in `highlightedHtml.ts` are harmless but may confuse

## Bootstrap
```bash
cd /Users/amos/bearcove/moire
git diff --stat
pnpm test
# Rebuild Rust for theme.rs change:
cargo build -p moire-web
```
