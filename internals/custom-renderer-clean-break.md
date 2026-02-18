# Custom Graph Renderer: Clean Break Plan

This is the blunt version.

We tried to force a complex, ELK-routed, compound graph model through React Flow.
It sucked.

We are not doing gradual rollout, compatibility layers, or dual-stack rendering.
We are replacing the graph renderer in one shot.

## What Failed

The core failure was not drawing rectangles or paths. The core failure was model mismatch.

- ELK gives us compound graph layout plus routed edge geometry.
- React Flow has its own parent/child coordinate semantics and interaction model.
- We repeatedly ended up translating coordinates between systems, then debugging translation bugs instead of debugging the app graph.
- We added temporary instrumentation and fallback logic to diagnose routing; it added noise and churn.

In practice, we kept paying complexity tax for a framework that is not our graph model.

## Decision

Build a custom graph canvas for peeps.

Hard requirements:

- ELK remains the layout and edge-routing engine.
- One canonical coordinate system for rendered geometry (`world` coordinates).
- No framework-level hidden graph semantics (no `parentId` behavior owned by third-party runtime).
- First-class support for compound/subgraph containers, routed edges, recording scrubbing, and future node dragging.

## Target Architecture

### 1. Canonical Graph Geometry

Create a single typed geometry contract produced by layout code and consumed by renderer:

- `GraphGeometry`
- `nodes: { id, kind, worldRect, data }[]`
- `groups: { id, scopeKind, label, worldRect, members }[]`
- `edges: { id, sourceId, targetId, polyline: Point[], kind, data }[]`

Everything is absolute world coordinates. No parent-local render coordinates.

### 2. ELK Adapter

`layout.tsx` (or successor) becomes a pure adapter:

- input: entity defs, edge defs, measured node sizes, grouping mode
- output: `GraphGeometry`
- rules:
- preserve ELK node/group rectangles in world space
- preserve ELK section/bendpoint geometry in world space
- keep section ordering deterministic (`incomingSections`/`outgoingSections` when present)
- no heuristic coordinate guessing

### 3. Renderer (SVG-first)

Use a custom SVG scene graph with explicit layers:

- Layer 1: group containers
- Layer 2: edges (polyline or smoothed bezier from polyline)
- Layer 3: nodes/cards
- Layer 4: overlays (selection glow, hover, labels)

Why SVG first:

- Direct path rendering and interaction on edges.
- DOM-level event handling is simpler for current graph sizes.
- Easy to inspect generated geometry in DevTools.

### 4. Camera + Viewport

Implement our own camera transform:

- `camera = { x, y, zoom }`
- world -> screen transform for rendering
- screen -> world inverse transform for interactions
- pan: pointer drag on empty canvas
- zoom: wheel centered on cursor
- fit-view: compute bounds from geometry and animate camera

### 5. Interaction Model

Implement explicit interaction state machine:

- selection: node, edge, or none
- hover: node/edge id
- drag mode:
- `pan`
- `node-drag(nodeId)` (future-compatible now, enabled once constraints are in)

Node dragging behavior:

- drag updates node `worldRect` position in local draft state
- connected edges re-render immediately from node anchors
- optional command: “re-run ELK from current positions as hints” (later)

### 6. Inspector + App Integration

Graph canvas is a dumb renderer with callbacks:

- `onNodeClick(id)`
- `onEdgeClick(id)`
- `onBackgroundClick()`
- `onNodeDragStart/Move/End`

App state (filters, focus, inspector, recording frame) stays in `App.tsx` domain logic.

### 7. Recording/Scrubbing

No React Flow-specific layout cache.

- union layout still built once from ELK geometry
- per-frame rendering toggles visibility/ghosting over stable world coordinates
- camera state persists across scrub changes

## Build It In One Shot

No partial migration. No compatibility path.

We do this in a branch and switch the app to the new renderer when done.

## Parallel Task Split (for Agent Team)

### Task A: Geometry Contract + ELK Adapter

Owner output:

- new `GraphGeometry` types
- ELK adapter returning absolute world-space nodes/groups/edges
- deterministic section ordering
- remove translation heuristics and debug scaffolding

Expected files:

- `frontend/src/graph/geometry.ts`
- `frontend/src/graph/elkAdapter.ts`
- `frontend/src/layout.tsx` (removed or slim wrapper)

### Task B: Camera + Canvas Core

Owner output:

- `GraphCanvas` with world/screen transform
- wheel zoom to cursor
- pan drag on background
- fit-view utility

Expected files:

- `frontend/src/graph/canvas/GraphCanvas.tsx`
- `frontend/src/graph/canvas/camera.ts`
- `frontend/src/graph/canvas/useCameraController.ts`

### Task C: Edge Renderer

Owner output:

- SVG edge rendering from ELK polyline points
- optional smoothing pass preserving endpoints
- edge hit area and selection/hover visuals

Expected files:

- `frontend/src/graph/render/EdgeLayer.tsx`
- `frontend/src/graph/render/edgePath.ts`

### Task D: Node + Group Renderer

Owner output:

- group container layer
- node card rendering (mock, channel pair, rpc pair)
- node hit testing and selection styling

Expected files:

- `frontend/src/graph/render/NodeLayer.tsx`
- `frontend/src/graph/render/GroupLayer.tsx`
- reuse existing node card UI pieces as needed

### Task E: Interaction Controller

Owner output:

- selection/hover wiring
- pointer state machine
- node drag behavior (enabled behind internal flag if needed, but implemented)

Expected files:

- `frontend/src/graph/interaction/useGraphInteraction.ts`
- `frontend/src/graph/interaction/hitTest.ts`

### Task F: App Integration + React Flow Removal

Owner output:

- replace `ReactFlow` graph mount with custom `GraphCanvas`
- keep existing toolbar/filter/inspector behavior
- remove React Flow node/edge type plumbing

Expected files:

- `frontend/src/App.tsx`
- `frontend/src/components/graph/*` (as needed)

### Task G: Validation + Cleanup

Owner output:

- deterministic layout snapshots for key examples
- remove dead React Flow-only code paths, CSS, and warnings
- verify record/scrub/focus/filter interactions

Expected files:

- `frontend/src/graph/__tests__/*`
- cleanup across `frontend/src/*`

## Definition of Done

We are done when all of these are true:

- no `ReactFlow` usage in runtime graph rendering path
- ELK-driven nodes/groups/edges render correctly with subgraphs enabled
- no coordinate translation heuristics
- pan/zoom/fit/select/hover are stable
- inspector selection works for nodes and edges
- recording scrub keeps stable geometry and correct visibility
- node dragging exists in code path (can be enabled by default if solid)

## Non-Goals For This Cut

- fancy physics animation
- minimap
- edge rerouting independent from ELK
- collaborative editing

Ship the clean renderer first. Add extras later.

