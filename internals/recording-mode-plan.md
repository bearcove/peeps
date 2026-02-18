# Recording Mode Plan

This file is the single source of truth for recording-mode work tracking.

Scope:
- Turn snapshotting into a record flow (`start`, periodic capture, `stop`)
- Support timeline scrubbing in UI
- Reach a final data model that supports stable layout over time

## Outcome We Want

When debugging transient stalls and recoveries, we can hit `Record`, let the app run, hit `Stop`, and scrub through time without losing graph readability.

## Milestones

### M0 - Groundwork

- [x] Confirm product semantics for record sessions (`start`/`stop`, single active session, interval defaults)
- [x] Confirm retention policy (frame cap and eviction behavior) — max_frames=1000 default, drop oldest on overflow
- [x] Confirm whether recording is global (all connected processes) or filterable (future) — global for now
- [x] Confirm minimum UI behavior while recording (live latest frame + elapsed + frame count)

### M1 - Record V1 (Thin Slice) ✓

- [x] Backend: add recording session state (`session_id`, `interval_ms`, `started_at`, `stopped_at`, `status`)
- [x] Backend: periodic snapshot loop every `interval_ms` (default `500ms`)
- [x] Backend: store frames in order with capture timestamp (pre-serialized JSON)
- [x] Backend: add APIs:
- [x] `POST /api/record/start`
- [x] `POST /api/record/stop`
- [x] `GET /api/record/current`
- [x] `GET /api/record/current/frame/{frame_index}`
- [x] Frontend: add `Record`/`Stop` button
- [x] Frontend: add basic timeline scrubber over captured frames
- [x] Frontend: render selected frame as normal graph (no layout stabilization yet)
- [x] Frontend: add `Live` toggle to follow newest frame while recording

### M2 - Stable Layout via Union Graph ✓

Frontend-only approach: union graph is built client-side from fetched frames, no backend union API needed for now.

- [x] Frontend: build union graph from all recording frames (`unionGraph.ts:buildUnionLayout()`)
- [x] Frontend: track per-node/per-edge frame presence (`nodePresence`, `edgePresence` maps)
- [x] Frontend: run ELK on union graph once, cache positions
- [x] Frontend: apply visibility masks when scrubbing (filter to active nodes/edges at frame `t`)
- [x] Frontend: keep stable positions across frames (union positions locked, auto-fit suppressed)
- [ ] Backend: build session-level union graph — deferred, not needed while frontend approach works
- [ ] Backend: compute activity intervals per node/edge — deferred
- [ ] Backend: expose union graph + frame-indexed visibility metadata — deferred

### M3 - Temporal Diagnostics UX

- [ ] Add change summary per frame (`nodes +/-, edges +/-`)
- [ ] Add inspector diffs against previous frame
- [ ] Add optional ghost mode (dim non-active nodes)
- [ ] Add jump controls (`next change`, `prev change`)

### M4 - Scale + Export

- [ ] Add frame downsampling options for long sessions
- [ ] Add max memory guardrails + overflow behavior
- [ ] Add export/import for recording sessions
- [ ] Add perf telemetry for recording overhead

## Final Data Model (Target)

This is the target model we should design toward, even if V1 only implements a subset.

### RecordingSession

- `session_id: string`
- `status: "recording" | "stopped"`
- `interval_ms: u32`
- `started_at_unix_ms: i64`
- `stopped_at_unix_ms: i64 | null`
- `frame_count: u32`
- `max_frames: u32`
- `overflowed: bool`

### Frame

- `frame_index: u32` (monotonic, 0-based)
- `captured_at_unix_ms: i64`
- `snapshot_id: i64` (if backed by existing snapshot API)
- `processes: ProcessFrame[]`

### ProcessFrame

- `proc_key: string`
- `proc_time_ms: u64 | null`
- `snapshot: Snapshot` (existing peeps snapshot payload for that process)

### UnionGraph

- `nodes: NodeHistory[]`
- `edges: EdgeHistory[]`

### NodeHistory

- `node_id: string` (stable entity id)
- `kind: string`
- `first_seen_frame: u32`
- `last_seen_frame: u32`
- `active_intervals: Interval[]` (`[start, end]`, inclusive, can have gaps)
- `attrs_latest: object`
- `attrs_by_frame?: map<u32, object>` (optional; enable only if needed for rich temporal attr diffs)
- `layout?: { x: f32, y: f32, w: f32, h: f32 }`

### EdgeHistory

- `edge_id: string` (stable, deterministic from `from+to+kind+label` or explicit id)
- `from_node_id: string`
- `to_node_id: string`
- `kind: string` (for example `needs`, `touches`)
- `first_seen_frame: u32`
- `last_seen_frame: u32`
- `active_intervals: Interval[]`
- `attrs_latest: object`
- `layout?: { points: [f32, f32][] }`

### Interval

- `start_frame: u32`
- `end_frame: u32`

## API Shape

- [x] `POST /api/record/start`
  - request: `{ interval_ms?: number, max_frames?: number }`
  - response: `RecordCurrentResponse { session: RecordingSessionInfo }`
- [x] `POST /api/record/stop`
  - request: `{}` (implicit current session)
  - response: `RecordCurrentResponse { session: RecordingSessionInfo }`
- [x] `GET /api/record/current`
  - response: `RecordCurrentResponse { session: RecordingSessionInfo | null }`
- [x] `GET /api/record/current/frame/{frame_index}`
  - response: `SnapshotCutResponse` (pre-serialized, same shape as `/api/snapshot`)
- [ ] `GET /api/record/:session_id/union`
  - response: union graph + intervals (+ layout if precomputed) — M2

## UI Interaction Model

- [x] `Record` button starts session and switches UI to recording mode
- [x] `Stop` ends session and keeps timeline available
- [x] Scrubber selects frame index (range slider in RecordingTimeline component)
- [x] `Live` mode auto-follows newest frame while recording
- [x] Frame label shows frame N/total + relative elapsed
- [ ] Optional ghost toggle for non-active nodes — M3

## Risks

- [x] Layout jitter if we re-run ELK per frame instead of union graph — solved by union graph approach
- [ ] Memory growth for long sessions with full attr history
- [ ] Snapshot latency drift at small intervals
- [ ] UX overload if we show too much temporal detail by default

## Decisions Log

- [x] Recording default interval target is `500ms`
- [x] Only one active session at a time (409 Conflict if already recording)
- [x] Frames stored as pre-serialized JSON to avoid Clone on Snapshot types
- [x] Decide whether to compute layout in backend, frontend, or both — frontend-only for now
- [x] Decide when union graph is built (on stop vs incremental during recording) — built on transition to scrubbing, after all frames fetched
