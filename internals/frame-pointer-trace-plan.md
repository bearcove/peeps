# Frame Pointer Trace Plan

This file is the execution plan for replacing callsite/facade source capture with frame capture + deferred symbolication.

Scope in one sentence: stop threading `SourceId` from macro-expanded callsites, capture stack frames in-process with frame pointers, and symbolize later in a separate pipeline.

## Why Change This

Right now, the facade machinery exists mostly to inject source context at call sites. It works, but it is invasive and expensive to maintain: wrappers, extension traits, `_with_source` variants, and macro expansion in every consumer crate.

The PoC proved we can capture raw frames and recover crate/module/function/file/line later, including absolute paths when debug info preserves them. That lets us move source attribution out of hot-path API design and into a dedicated trace pipeline.

## Hard Decisions (Final)

- No backward compatibility layer.
- Require frame pointers for instrumented builds.
- Do not use `backtrace` crate for capture in production path.
- Reuse and extend existing wire handshake (`peeps-wire::Handshake`) instead of creating a second protocol.
- Fail fast on missing module identity or missing symbolication prerequisites.

## Target Architecture

### Crates

- `peeps-trace-types`
  - Shared wire/storage types for module identity, frame keys, and interned backtraces.
- `peeps-trace-capture`
  - In-process capture and interning.
  - Frame-pointer unwinder (`x86_64` + `aarch64` first).
- `peeps-trace-symbolicate`
  - Out-of-process/offline symbolication and symbol cache.
- `peeps-trace-alloc` (optional feature crate)
  - Allocation/deallocation capture + pointer attribution table.
- `peeps-trace-sampler` (optional feature crate)
  - Thread sampling (every `Tms`) feeding the same backtrace interner.

### Core Types

- `ModuleId` (interned)
- `BacktraceId` (interned)
- `FrameKey { module_id, rel_pc }`
- `BacktraceRecord { id, frames: Vec<FrameKey> }`
- `ModuleRecord { id, path, runtime_base, build_id/debug_id, arch }`

### Runtime/Event Shape

- Keep a canonical single-location field for fast UI linking (`source`), derived from top resolved frame.
- Add `backtrace_id` on events/edges/entities where we need deep attribution.

## Protocol Plan (Existing Handshake)

Extend current `ClientMessage::Handshake` payload with trace capability + module manifest:

- capture capabilities:
  - `trace_v1: true`
  - `requires_frame_pointers: true`
  - `sampling_supported`, `alloc_tracking_supported`
- module manifest:
  - module path
  - runtime base
  - build-id/debug-id
  - arch

Server behavior:

- Validate all required module/debug identities at connect.
- Reject connection if any required module cannot be mapped to debug info.
- Do not accept trace-bearing messages until handshake validation succeeds.

## Milestones

### M0 - Schema + Handshake Contract

- [ ] Add `peeps-trace-types` crate with v1 schema.
- [ ] Extend `peeps-wire::Handshake` to carry trace capabilities + module manifest.
- [ ] Add strict validation path in `peeps-web` connection setup.
- [ ] Add explicit connection rejection errors for trace precondition failures.

### M1 - Frame Pointer Capture Foundation

- [ ] Add `peeps-trace-capture` with arch-specific FP unwinder.
- [ ] Add module registry (`ip -> module_id, rel_pc`) with strict null/overflow checks.
- [ ] Add `BacktraceId` interner keyed by canonical frame sequence.
- [ ] Add startup invariant checks for frame-pointer unwind sanity.
- [ ] Enforce build flags for instrumented targets:
- [ ] Rust: `-C force-frame-pointers=yes`
- [ ] C/C++ deps: `-fno-omit-frame-pointer`

### M2 - Symbolication Pipeline

- [ ] Add `peeps-trace-symbolicate` crate.
- [ ] Implement `FrameKey -> symbol` resolver with module/debug cache.
- [ ] Resolve crate/module path + file/line/col, intern resolved frames.
- [ ] Fail hard on missing module/debug artifacts for declared modules.
- [ ] Add deterministic report tooling for unresolved frames.

### M3 - Runtime Integration (No Facade Dependency)

- [ ] Introduce capture API callable from normal functions (no macro required).
- [ ] Replace source injection at key runtime event/edge/entity write points with `backtrace_id`.
- [ ] Derive canonical `source` from top resolved frame for existing views.
- [ ] Remove facade-only source plumbing where no longer needed.
- [ ] Delete obsolete `_with_source` propagation layers once call paths are migrated.

### M4 - Allocation Tracking

- [ ] Add `peeps-trace-alloc` with opt-in modes: `off`, `sampled`, `full`.
- [ ] Track pointer attribution map (`ptr -> alloc backtrace + size/class`).
- [ ] Capture dealloc events and link back to allocation attribution.
- [ ] Add strict memory guardrails for attribution map growth.

### M5 - Sampling Profiler Mode

- [ ] Add `peeps-trace-sampler` for periodic thread stack capture (`Tms`).
- [ ] Feed sampled stacks through same `BacktraceId` interner.
- [ ] Add per-thread/process rate controls and overload backpressure.
- [ ] Add explicit drop/overflow accounting in emitted telemetry.

## Cutover Rules

- No dual-path compatibility bridge.
- Ship behind a feature gate only until end-to-end trace pipeline is green.
- Once M3 is complete, remove facade path and old source propagation code in one delete pass.

## Risks and Mitigations

- Runtime overhead from mandatory frame pointers.
  - Mitigation: benchmark before/after on representative workloads.
- Unwind correctness across toolchains and mixed-language binaries.
  - Mitigation: startup validation + CI matrix for supported targets.
- Alloc tracking memory pressure.
  - Mitigation: strict caps, drop policy, and explicit overflow counters.
- Sampling perturbation at low intervals.
  - Mitigation: hard minimum interval + adaptive throttling.

## Out of Scope (This Plan)

- Preserving wire compatibility with older agents.
- Supporting binaries that omit frame pointers.
- Best-effort symbolication fallbacks for missing build IDs.

