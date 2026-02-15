# Graph Model (Current Decisions)

Status: draft
Owner: wg-graph
Scope: canonical data model for `peeps-web`

## Purpose

Capture the current graph design decisions before updating the rest of the specs.

## Core principles

1. One canonical node model.
2. One canonical edge kind: `needs`.
3. No inferred/derived/heuristic edges. Explicit measurements only.
4. Keep edge semantics minimal; avoid combinatorial edge-type taxonomies.

## Nodes

Each resource/runtime entity is a node with:

- `id` (stable, globally unique within a snapshot)
- `kind`
- `process`
- shared optional fields (label/source location/etc.)
- `attrs_json` for type-specific fields

Identity convention:
- use `proc_key = {process}:{pid}` (or stable runtime instance id)
- resource IDs must include `proc_key` to avoid cross-process collisions
- connection IDs must be sanitized stable tokens (`conn_{u64}`)

Examples of node kinds:
- `task`
- `future`
- `request`
- `response`
- `lock`
- `semaphore`
- `mpsc_tx`, `mpsc_rx` (preferred over a single mpsc node)
- `oneshot_tx`, `oneshot_rx`
- `watch_tx`, `watch_rx`
- `roam_channel_tx`, `roam_channel_rx` (if direction is meaningful)
- `oncecell`

## Edges

Only one edge kind:

- `needs`

Meaning:

- source node needs destination node/resource to make forward progress.
- this includes currently-blocked dependencies and explicit structural/causal topology.
- edges encode dependency topology only.

No special structural edge kind for now.

## Edge fields

Required:

- `src_id`
- `dst_id`
- `kind = "needs"`

Optional (only if clearly needed later):

- `observed_at_ns`

Not part of base model:

- `process` on edge (already recoverable from nodes)
- `blocking` (redundant: `needs` already means dependency)
- `duration_ns` / `started_at_ns` / `ended_at_ns`
- `count`
- `why` enums
- per-edge severity

## Channel direction model

Direction should be represented by endpoint nodes, not edge labels:

- channel send endpoint node (`...:tx`)
- channel recv endpoint node (`...:rx`)

Then dependencies are naturally directional:

- task needs `...:tx` => send-side dependency
- task needs `...:rx` => recv-side dependency
- `...:tx` needs `...:rx` => sender-side progress ultimately depends on receiver draining

This avoids adding `send`/`recv` edge variants.

For Roam channels:

- explicit metadata is on requests, not on channels.
- channel provenance is established at channel provisioning time from the owning request context.
- this allows cross-process channel lineage without inventing channel metadata.

## Futures

Important constraint:

- futures can move across tasks.
- do not model futures as belonging to a single task.

Safe modeling:

- tasks need futures (`task -> future`)
- futures need resources (`future -> lock/channel/...`) when explicitly measured

## Locks: ID namespace clarification

- lock wrapper `holder_id` / waiter token IDs are local bookkeeping IDs only.
- only `peeps_task_id` is valid for cross-resource identity.
- lock edges must use canonical task IDs + canonical lock IDs.

## Snapshot model

Use synchronized server-orchestrated snapshots:

1. UI clicks `Jump to now`.
2. Server creates `snapshot_id`.
3. Server requests dumps from all currently connected processes.
4. Processes reply tagged with that `snapshot_id`.
5. Server stores replies under the same `snapshot_id`, with per-process status for missing/timeouts.

This replaces fake global sequencing from independent push timing.

Process is context, not a node:

- process identity is carried as a node attribute (and in snapshot process-status tables)
- process can be used for filtering/grouping
- no dedicated `process` node is required in the graph

## RPC request/response model

Model RPC as two nodes:

- request node (caller-side)
- response node (receiver-side lifecycle/result)

Dependency edge:

- `request -> response` (`needs`)

Emission convention:

- caller emits request node
- receiver emits response node
- receiver also emits `request -> response` edge

Pairing convention:

- both nodes must carry `attrs_json.correlation_key = "{connection}:{request_id}"`
- request/response matching is done by `correlation_key`, not by node ID prefix rewrite

This keeps request completion ownership on the side that can authoritatively report response state.

## Health/state lives on nodes

Node attributes carry triage signal, for example:

- request: elapsed/in-flight age
- channel endpoint: queue length, capacity, full/closed flags
- lock: waiter/holder counts
- semaphore: permits + waiter count

Edges do not carry severity. Edges are traversal only.

## Debugging workflow model

1. Start from a node you recognize (stuck request/task/future).
2. Traverse outgoing `needs` edges.
3. Stop at nodes whose attrs indicate unhealthy state.
4. Run SCC over filtered subgraph (only nodes in unhealthy states) for deadlock cycles.

## UI scope (for now)

- Requests-first flow.
- One top-level tab: `Requests`.
- Stuck-requests table first.
- ELK graph can be prototyped with mock data.
- No kitchen-sink multi-tab dashboard.

## What this doc intentionally leaves open

- Exact node kind names (`mpsc_tx` vs `channel_tx` naming)
- Exact required attrs per kind
- API payload shape details

Those should be aligned in `002`, `006`, and `007-*` after this baseline is accepted.
