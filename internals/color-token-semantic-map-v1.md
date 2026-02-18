# Color Token Semantic Map v1

Scope: first-pass mapping for `status`, `interactive`, and `border` buckets only.

Inputs:

- `/Users/amos/bearcove/peeps/internals/color-token-usage.tsv`
- `/Users/amos/bearcove/peeps/internals/color-token-usage.json`

Goal of this pass:

- collapse high-use `--color-lit-*` tokens into a smaller semantic layer
- prefer existing core tokens where intent matches
- introduce new semantic tokens only when intent is clearly distinct

## Proposed Semantic Tokens (this pass)

Existing core tokens reused:

- `--border`
- `--border-subtle`
- `--focus-ring`
- `--accent`

New semantic tokens to add:

- `--status-ok-fg`
- `--status-ok-bg`
- `--status-warn-fg`
- `--status-warn-bg`
- `--status-crit-fg`
- `--status-crit-bg`
- `--status-crit-strong`
- `--status-warn-strong`
- `--interactive-selected-fg`
- `--interactive-selected-bg`
- `--interactive-selected-border`
- `--interactive-selected-halo`
- `--interactive-selected-fill`
- `--interactive-info-fg`
- `--interactive-info-bg`
- `--interactive-info-border`
- `--control-border`
- `--focus-border`
- `--focus-halo-strong`

## Mapping Table

### Status

| Current token | Value | Proposed semantic token | Notes |
|---|---|---|---|
| `--color-lit-ld-d59cb2c4` | `light-dark(#155724, #75b798)` | `--status-ok-fg` | Used in badge/duration/timestamp OK text |
| `--color-lit-ld-c64378ce` | `light-dark(#d4edda, #1f3d2b)` | `--status-ok-bg` | OK badge background |
| `--color-lit-ld-d791833b` | `light-dark(#856404, #ffc107)` | `--status-warn-fg` | Used in badge/duration/timestamp/app header warn text |
| `--color-lit-ld-849f65eb` | `light-dark(#fff3cd, #3a2f1a)` | `--status-warn-bg` | Warn badge/header background |
| `--color-lit-ld-6cbed306` | `light-dark(#721c24, #ff6b6b)` | `--status-crit-fg` | Used in badge/duration/timestamp/menu danger |
| `--color-lit-ld-7633e6e6` | `light-dark(#f8d7da, #3a1a1a)` | `--status-crit-bg` | Crit badge background |
| `--color-lit-ld-015826c9` | `light-dark(#d30000, #ff6b6b)` | `--status-crit-strong` | Strong crit border/emphasis in graph/inspector |
| `--color-lit-ld-3349dec3` | `light-dark(#bf5600, #ffa94d)` | `--status-warn-strong` | Strong warn border/emphasis in graph/inspector |

### Interactive

| Current token | Value | Proposed semantic token | Notes |
|---|---|---|---|
| `--color-lit-ld-51fce81c` | `light-dark(#3b82f6, #60a5fa)` | `--accent` | Already mapped as app accent |
| `--color-lit-ld-4ed2e9cc` | `light-dark(#4a7dff, #6b9aff)` | `--interactive-selected-border` | Filter menu active border |
| `--color-lit-ld-7e87d777` | `light-dark(#eef3ff, #1a2340)` | `--interactive-selected-bg` | Filter menu active background |
| `--color-lit-ld-d1be9e2f` | `light-dark(#2956b2, #8bb4ff)` | `--interactive-selected-fg` | Filter menu active text |
| `--color-lit-ld-e3eb2eb1` | `light-dark(rgba(74, 125, 255, 0.25), rgba(107, 154, 255, 0.25))` | `--interactive-selected-halo` | Active control ring/halo |
| `--color-lit-ld-e2fd553d` | `light-dark(rgba(74, 125, 255, 0.15), rgba(107, 154, 255, 0.15))` | `--interactive-selected-fill` | Active fill tint |
| `--color-lit-ld-6d678c8a` | `light-dark(#1a56db, #7aadff)` | `--interactive-info-fg` | Timeline/inspector info text |
| `--color-lit-ld-ffa8dbee` | `light-dark(#e8f0fe, #1a2544)` | `--interactive-info-bg` | Timeline info background |
| `--color-lit-ld-f726ec88` | `light-dark(#b4cef9, #2f4a8a)` | `--interactive-info-border` | Timeline info border |
| `--color-lit-ld-98c02837` | `light-dark(#0071e3, #0a84ff)` | `--interactive-selected-border` | Keep same semantic as selected border |
| `--color-lit-ld-7af67c48` | `light-dark(rgba(0, 113, 227, 0.3), rgba(10, 132, 255, 0.4))` | `--interactive-selected-halo` | Blue highlight glow |
| `--color-lit-ld-a9342461` | `light-dark(rgba(0, 113, 227, 0.18), rgba(10, 132, 255, 0.22))` | `--interactive-selected-fill` | Soft blue glow/fill |
| `--color-lit-rgb-05954929` | `rgba(59, 130, 246, 0.2)` | `--interactive-selected-halo` | Convert to `light-dark(...)` semantic halo |
| `--color-lit-ld-84038782` | `light-dark(rgba(0, 0, 0, 0.15), rgba(255, 255, 255, 0.2))` | `--focus-ring` | Generic focus halo in controls |
| `--color-lit-ld-43da4eab` | `light-dark(#999, #666)` | `--focus-border` | Border color on focus for text/action controls |

### Border

| Current token | Value | Proposed semantic token | Notes |
|---|---|---|---|
| `--color-lit-ld-15d73b5f` | `light-dark(#d2d2d7, #2a2a2e)` | `--border-subtle` | Main panel/table separators |
| `--color-lit-ld-b19fa9e1` | `light-dark(#d2d2d7, #3a3a3e)` | `--border` | Primary control borders |
| `--color-lit-ld-cd269152` | `light-dark(#a09888, #665e52)` | `--control-border` | Warm control/menu/select/checkbox border |
| `--color-lit-ld-374928ee` | `light-dark(#d2d7e0, #3a3a3e)` | `--border` | Panel utility border; near-equivalent to `--border` |

## Notes

- `StorybookPage.css` currently contributes several local border shades (`--color-lit-ld-d376042d`, `--color-lit-ld-0383b9fd`, `--color-lit-ld-edf3e31a`). Treat those as lab-only and map them after app UI buckets are consolidated.
- `--color-lit-rgb-05954929` should be replaced by a semantic `light-dark(...)` token to avoid single-mode RGBA drift.
- After remapping, run the usage report again and delete zero-reference `--color-lit-*` aliases.
