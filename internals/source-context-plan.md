# Source Capture + PeepsContext Plan

This note captures where we landed after running into messy source attribution in real snapshots.

## The practical problem

You instrument an app, take a snapshot, and some nodes/events point at `core/src/ops/function.rs:250` or other wrapper internals instead of the place you care about. That means the graph is technically valid but operationally annoying: you can see the stall, but the source location is not where the bug lives.

The first instinct is "just use `#[track_caller]` everywhere". In practice, that does not hold across every wrapper, adapter, async boundary, extension trait, and helper chain. Some paths are easy to thread cleanly, some are not, and forcing perfect threading everywhere makes API ergonomics worse than the original problem.

## What we actually want

We want two properties at once:

1. Good callsite location when we can get it.
2. Reliable crate/root attribution even when callsite location degrades.

That means source attribution cannot rely on a single signal.

## Two-signal model

We keep two independent pieces of context:

1. `Location::caller()` (or explicit `peeps::Source`) for file/line.
2. `PeepsContext` for stable crate root (`manifest_dir`).

`PeepsContext` is intentionally small right now:

- `manifest_dir: &'static str`

No caller location is stored in `PeepsContext`. Caller location is separate.

## Resolution rules

When peeps needs source/crate attribution:

1. If an explicit `peeps::Source` was provided, use that location.
2. Otherwise use `Location::caller()` from the current `#[track_caller]` boundary.
3. Always use `PeepsContext.manifest_dir` to resolve relative file paths.
4. Infer crate from `manifest_dir + file` (and nearby `Cargo.toml` walk as needed).
5. If caller threading is weak on a path, move source capture upstream and pass explicit `Source` down that chain.

This gives us a stable fallback without pretending `track_caller` is perfect.

## API direction

### Internal peeps APIs

- Keep `*_with_cx(cx: PeepsContext)` style entry points.
- Keep explicit-source escape hatches where needed.
- Prefer `#[track_caller]` on public ergonomic methods.

### Macro-gated facade

The long-term ergonomic path is still to gate the "nice" API through macro expansion so each crate gets a local, pre-bound context.

`init!()` can expand to crate-local tokens that bind:

- `const PEEPS_CX: PeepsContext = PeepsContext::new(env!("CARGO_MANIFEST_DIR"));`
- facade methods/extensions that call peeps internals with `PEEPS_CX`

This makes context always available without polluting callsites.

## Why not generic type identity (`Mutex<C, T>`)?

Because cross-crate values break down quickly:

- `Mutex<C, T>` is not `Mutex<C2, T>`.
- passing instrumented values between crates becomes painful.

So the type identity should stay simple (`Mutex<T>`), and context should travel via facade/API calls, not type parameters.

## What this buys us

- Better real-world source attribution now.
- Deterministic crate/root context even when callsite propagation is imperfect.
- A clear "escape hatch" (`Source`) for stubborn chains.
- Room to add more context fields later (crate version, build metadata, etc.) without changing the core model.

## Limits (explicit)

- We still cannot magically recover the true semantic caller at every boundary without either:
  - clean `#[track_caller]` threading, or
  - explicit `Source` forwarding, or
  - heavier compile-time/runtime rewriting.
- The model is pragmatic, not magical: it improves attribution quality and keeps failure modes understandable.
