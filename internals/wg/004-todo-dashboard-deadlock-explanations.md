# 004 - Dashboard Deadlock Explanations

## Goal

Show explainable deadlock/wait chains in dashboard, not just raw counters.

## Scope

- Add `deadlock_candidates` to API payload.
- New panel/tab with:
  - candidate list by severity
  - cycle path rendering
  - per-edge explanation ("A waits on lock L owned by B")
- Every node in path is clickable to stable URL.

## Deliverables

- Backend serialization for candidate data.
- Frontend components for list + path details.
- Deep links for all resource kinds used in cycles.
- Copy text suitable for triage ("likely root cause", "oldest blocker", "cross-process chain").

## Acceptance

- User can open one candidate and follow links end-to-end.
- Explanations include enough detail to decide first debugging action.

## Notes

- Prioritize clarity and low visual noise over fancy graph rendering.

