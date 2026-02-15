# 003 - Cycle Detection + Ranking

## Goal

Detect deadlock candidates from the normalized wait graph and rank by impact.

## Scope

- SCC detection over blocking edges (Tarjan or Kosaraju).
- Candidate classes:
  - multi-node cycle
  - self-loop with sustained wait
- Ranking signals:
  - worst wait age
  - blocked task count
  - cross-process involvement
  - repeated appearance across snapshots

## Deliverables

- `find_deadlock_candidates(graph)` implementation.
- Candidate payload with:
  - node/edge set
  - representative cycle path
  - severity score
  - rationale strings
- Threshold config for warn/danger.

## Acceptance

- Detector ignores clearly non-blocking/idle edges.
- Candidate output is stable across equivalent graph orderings.

## Notes

- Keep detector pure and unit-testable (no UI or transport assumptions).

