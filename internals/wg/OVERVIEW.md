# Wait Graph Roadmap

This workgroup tracks "wait chains" and deadlock detection for peeps + dashboard.

## Rules

- Phase files live in `internal/wg/`.
- Naming: `NNN-todo-TASKNAME.md`.
- When complete, rename `todo` to `done`.

## Phases

1. `001-todo-unified-wait-graph-model.md` - Define canonical graph schema and semantics.
2. `002-todo-edge-ingestion-normalization.md` - Build edge extraction and normalization pipeline.
3. `003-todo-cycle-detection-ranking.md` - Implement SCC/cycle detection and severity ranking.
4. `004-todo-dashboard-deadlock-explanations.md` - Expose deadlock candidates and explainable chains in UI.
5. `005-todo-validation-fixtures-and-rollout.md` - Add fixtures, tests, thresholds, and rollout safeguards.

## Current Status

- `001`: todo
- `002`: todo
- `003`: todo
- `004`: todo
- `005`: todo

## Exit Criteria

- Dashboard shows concrete blocked chains with clickable resources.
- Deadlock candidates are cycle-backed (not heuristic-only).
- Cross-process RPC links participate in the same causal graph.
- We can explain each candidate with a human-readable path.

