# Frontend Investigate MVP Spec

Status: todo
Owner: wg-frontend
Scope: `crates/peeps-web/frontend`

## Goal

Build a Requests-first interface for local debugging without kitchen-sink scope.

## Stack

- Vite
- Preact
- TypeScript

## Core UX

- one top-level tab only: `Requests`
- one primary action: `Jump to now`
- one default view: stuck requests table (`elapsed >= 5s`)
- ELK graph prototype allowed (mock data first)
- requests table must be powered by canonical query from `003-todo-api-contract.md`

## Visual direction

- dense technical UI
- no noisy ornamentation
- OS theme only via CSS `light-dark()`
- no manual theme toggle

## Layout

Desktop:
- header: title + snapshot indicator + jump button
- left: stuck requests list/table
- center: ELK prototype graph
- right: side inspector

Mobile:
- requests -> graph -> inspector stacked

## Interaction contract

- clicking request focuses corresponding graph context
- hover cards on nodes/edges
- clicking node/edge opens side inspector
- keyboard support: arrows + enter + esc

## Scope boundaries

In scope:
- Requests tab only
- ELK prototype with icons/hover cards/inspector
- ELK layout execution off main thread (web worker)

Out of scope:
- additional tabs
- SQL editor UI
- auto-refresh/live stream UI

## Acceptance criteria

1. Requests tab only.
2. `Jump to now` updates snapshot.
3. stuck-request triage works from table + inspector.
4. ELK prototype interactions are usable with mock data.
