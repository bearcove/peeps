# 001 - Unified Wait Graph Model

## Goal

Define one canonical graph model for all blocking relationships.

## Scope

- Node kinds:
  - `task`
  - `future`
  - `lock`
  - `channel`
  - `rpc_request`
  - `process`
- Edge kinds:
  - `task_waits_on_resource`
  - `resource_owned_by_task`
  - `task_wakes_future`
  - `future_resumes_task`
  - `rpc_client_to_request`
  - `rpc_request_to_server_task`

## Deliverables

- Rust types for normalized graph nodes/edges in peeps data path.
- Stable IDs for every node kind.
- Edge metadata contract (`first_seen`, `last_seen`, `count`, `severity_hint`, `source_snapshot`).
- Doc section with blocking vs non-blocking semantics.

## Acceptance

- Every existing instrumentation source can map to this model without lossy ad-hoc transforms.
- No dashboard-only schema drift.

## Notes

- Keep this model backend-first and frontend-agnostic.

