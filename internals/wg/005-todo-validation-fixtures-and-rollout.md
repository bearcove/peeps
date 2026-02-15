# 005 - Validation, Fixtures, Rollout

## Goal

Ship safely with confidence in signal quality.

## Scope

- Add fixture bundles covering:
  - true deadlock cycle
  - long wait but no cycle
  - bursty transient waits
  - cross-process RPC chain cycle
- Add unit tests:
  - graph normalization
  - SCC detection
  - severity ranking
- Add integration tests for dashboard payload shape.
- Add rollout toggles/guardrails if needed.

## Deliverables

- Fixture corpus committed in repo.
- Test suite green in CI.
- Short runbook: "How to validate a deadlock candidate."
- False-positive triage checklist.

## Acceptance

- Detector catches known synthetic deadlocks.
- Detector does not flag known benign parked/idle patterns as danger.

## Notes

- Keep thresholds configurable so we can tune with real workloads.

