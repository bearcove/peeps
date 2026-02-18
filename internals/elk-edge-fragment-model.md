# ELK Edge Model: Fragment-First, World-Space Normalization

This is the model we are using going forward.

The old model was: “for each app edge ID, pick one ELK edge record.”
That model is wrong for compound graphs.

The new model is: “for each app edge ID, collect all ELK routing fragments, normalize each fragment to world coordinates, then stitch deterministically.”

## Problem Statement

In compound ELK layout (`elk.hierarchyHandling = INCLUDE_CHILDREN`):

- Node coordinates are hierarchical (child-local).
- Edge sections are attached to edge records that may exist at different graph levels.
- Multiple ELK edge records can correspond to one app-level edge ID.

If we flatten nodes to world space but “pick one edge record,” we get detached or warped edges in subgraph mode.

## Non-Negotiable Rule

We do not pick one “best” edge record.

No “first match.”
No “most sections.”
No heuristic score-based winner.

## Data Model

For each ELK edge section we keep:

- `appEdgeId: string`
- `ownerGraphPath: string[]` (root -> ... -> owner graph node id)
- `ownerOffset: { x, y }` (sum of owner graph ancestry transforms)
- `sectionId?: string`
- `incomingSections?: string[]`
- `outgoingSections?: string[]`
- `localPoints: Point[]` (`startPoint + bendPoints + endPoint`)
- `worldPoints: Point[]` (computed as `local + ownerOffset`)

Conceptually:

- `edgeId -> fragments[]`

Not:

- `edgeId -> single record`

## Pipeline

### 1. Traverse ELK tree once

Walk the ELK graph hierarchy and carry cumulative transform:

- at root: `{x:0,y:0}`
- for child graph: `parentOffset + child.{x,y}`

Collect all edge sections encountered, each tagged with the current owner offset/path.

### 2. Normalize fragments to world space

For each fragment, compute `worldPoints` by adding owner offset to every section point.

After this step, all geometry is in one coordinate system.

### 3. Build chains

Within each `appEdgeId`:

- Use section linkage metadata (`incomingSections`, `outgoingSections`) when available.
- If linkage metadata is missing, use endpoint continuity in world space as tiebreaker only.
- Build one or more ordered polylines.

### 4. Select final polyline(s)

Prefer the chain that connects source-anchor vicinity to target-anchor vicinity.
If multiple valid chains exist, keep deterministic ordering by:

1. anchor proximity score
2. fragment count
3. stable lexical tie-break on section IDs

### 5. Validate invariants

Reject or loudly warn when:

- no chain reaches both endpoints
- chain start is too far from source anchor
- chain end is too far from target anchor
- unresolved section graph cycles with no deterministic break

No silent fallback during development mode.

## Invariants

For every rendered edge:

1. `polyline.length >= 2`
2. `distance(polyline.first, sourceAnchor) <= endpointTolerance`
3. `distance(polyline.last, targetAnchor) <= endpointTolerance`
4. polyline points are world-space
5. output is deterministic for same ELK input

`endpointTolerance` should be explicit and small (for example 24-48px depending on marker/handle design).

## What This Buys Us

- One coordinate system for rendering and hit-testing.
- Correct routing in subgraph mode without owner-level guessing.
- Clean support for future flattened-world operations (drag, spatial index, snapping).

## Implementation Notes (Codebase Mapping)

- ELK adapter ownership:
  - `frontend/src/graph/elkAdapter.ts`
- Geometry contract:
  - `frontend/src/graph/geometry.ts`
- Rendering:
  - `frontend/src/graph/render/EdgeLayer.tsx`
- Interaction/hit-test (world-space consumers):
  - `frontend/src/graph/interaction/hitTest.ts`

The adapter is the boundary where hierarchical ELK output becomes world-space geometry.
Renderer and interaction code should never need to know ELK hierarchy details.

## Debug Instrumentation We Keep

We keep targeted diagnostics for this model:

- per edge:
  - fragment count
  - chain count
  - chosen chain id
  - start/end anchor deltas
- aggregate:
  - edges failing invariants
  - edges with missing linkage metadata

We do not keep heuristic “choose whichever looks less wrong” logic.

## Summary

Old model: `edgeId -> pick one ELK record` (wrong for compound graphs).

New model: `edgeId -> collect fragments -> normalize to world -> chain deterministically -> validate`.

That is the model.

