# Source Capture + PeepsContext Implementation Spec

This is a build-oriented spec. It defines exact API shape, attribution rules, tests, and rollout criteria.

## Goal

Attribution must use two signals:

1. callsite location (`Location::caller()` or explicit `Source`)
2. callsite crate root (`PeepsContext.manifest_dir`)

This must work for cross-crate value usage via a crate-local facade generated at item scope.

## End-state API

### Core types

`peeps` MUST expose:

```rust
pub struct PeepsContext { manifest_dir: &'static str }
impl PeepsContext {
    pub const fn new(manifest_dir: &'static str) -> Self;
    pub const fn manifest_dir(self) -> &'static str;
}

pub struct Source { /* location */ }
impl Source {
    #[track_caller]
    pub fn caller() -> Self;
}
```

### Method shape

For instrumented operations, peeps MUST expose:

- context-bearing entrypoint: `*_with_cx(..., cx: PeepsContext, ...)`
- explicit-source + context entrypoint for every operation surfaced through `facade!()` across the peeps wrapper API:
  - either `*_with_source(..., source: Source, cx: PeepsContext, ...)`
  - or equivalent naming/signature where explicit source can override `Location::caller()`
- any `*_with_cx` entrypoint that may capture implicit caller location MUST be `#[track_caller]`

For every wrapper type surfaced through `facade!()`, peeps MUST NOT expose inherent methods that clash with facade ergonomic method names.
Those names are reserved for facade-generated extension traits so crate-local context injection can win method resolution.
If temporary compatibility shims are needed, they MUST use non-conflicting names and be marked deprecated.

For ergonomic methods on facade-covered wrappers, peeps SHOULD keep wrappers with `#[track_caller]` that call explicit-source low-level APIs.

### Facade-covered wrapper inventory (normative)

In this plan, "facade-covered wrapper API surface" means the full public wrapper set exported by peeps wrappers, not only `Mutex`.
The required coverage set is:

- synchronization: `Mutex<T>`, `RwLock<T>`, `Notify`, `OnceCell<T>`, `Semaphore`
- channels: `Sender<T>`, `Receiver<T>`, `UnboundedSender<T>`, `UnboundedReceiver<T>`, `OneshotSender<T>`, `OneshotReceiver<T>`, `BroadcastSender<T>`, `BroadcastReceiver<T>`, `WatchSender<T>`, `WatchReceiver<T>`
- process/task wrappers: `Command`, `Child`, `JoinSet<T>`
- rpc wrappers: `RpcRequestHandle`, `RpcResponseHandle`

If wrapper types are added or removed in `/Users/amos/bearcove/peeps/crates/peeps/src/enabled.rs` and `/Users/amos/bearcove/peeps/crates/peeps/src/disabled.rs`, this inventory MUST be updated in the same change.

Example requirement:

```rust
impl<T> Mutex<T> {
    pub fn lock_with_source(
        &self,
        source: Source,
        cx: PeepsContext,
    ) -> MutexGuard<'_, T>;
    pub fn try_lock_with_source(
        &self,
        source: Source,
        cx: PeepsContext,
    ) -> Option<MutexGuard<'_, T>>;
    pub fn lock_with_cx(&self, cx: PeepsContext) -> MutexGuard<'_, T>;
    pub fn try_lock_with_cx(&self, cx: PeepsContext) -> Option<MutexGuard<'_, T>>;
}
```

### Macro split (normative)

To avoid ambiguity with current usage, macros are split:

1. `init!()` remains runtime-only bootstrap.
- Allowed in function bodies.
- Expands to runtime setup (for example `__init_from_macro(...)`), not trait/module generation.

2. `facade!()` (new) generates crate-local facade bindings.
- Must be invoked at item/module scope.
- Generates `PEEPS_CX`, extension traits/wrappers, and a `prelude` re-export module.
- Is the normative path for crate-local method ergonomics.

## Facade contract (`facade!()`)

`facade!()` MUST generate crate-local bindings that inject:

- `PeepsContext::new(env!("CARGO_MANIFEST_DIR"))`
- `#[track_caller]` at facade method boundary

Type identity MUST remain global (`::peeps::Mutex<T>`), while call context is crate-local.
Facade methods own ergonomic names like `.lock()` because inherent name clashes are disallowed above.
Trait method resolution is normative:

- `facade!()` MUST generate `pub mod prelude` that re-exports facade extension traits.
- callsites using method syntax MUST import `use crate::peeps::prelude::*;`
- callsites that do not import the prelude MUST use UFCS to call facade traits explicitly.

Representative generated pattern:

```rust
pub mod peeps {
    pub const PEEPS_CX: ::peeps::PeepsContext =
        ::peeps::PeepsContext::new(env!("CARGO_MANIFEST_DIR"));

    pub trait MutexExt<T> {
        fn lock(&self) -> ::peeps::MutexGuard<'_, T>;
    }

    impl<T> MutexExt<T> for ::peeps::Mutex<T> {
        #[track_caller]
        fn lock(&self) -> ::peeps::MutexGuard<'_, T> {
            self.lock_with_source(::peeps::Source::caller(), PEEPS_CX)
        }
    }

    pub mod prelude {
        pub use super::MutexExt;
    }
}
```

## Normative attribution algorithm

Given operation input `(cx, maybe_source_override)`:

1. Determine raw location:
   - if explicit source was supplied, use it
   - else capture `Location::caller()` at the current `#[track_caller]` boundary
2. Capture boundary requirements:
   - `*_with_cx` methods that do implicit capture MUST be `#[track_caller]`
   - facade wrapper methods MUST be `#[track_caller]`
   - facade wrappers MUST pass explicit `Source::caller()` to explicit-source low-level APIs
   - direct low-level callers that skip facade MUST either:
     - pass explicit `Source`, or
     - call from a `#[track_caller]` boundary and accept wrapper-quality location
3. Build source string as `{file}:{line}`.
4. Resolve file path for inference:
   - if source path is absolute, use it
   - if source path is relative, join with `cx.manifest_dir`
5. Infer crate with manifest-aware cache:
   - cache key MUST include both `cx.manifest_dir` and source string
   - key shape is normative: `(manifest_dir, source)`
6. Infer crate:
   - walk upward from resolved file directory to nearest `Cargo.toml`
   - read `[package].name`
7. Failure behavior:
   - if no file or no package found, keep source as-is and set `krate = None`
   - MUST NOT panic
8. Persist resulting `(source, krate)` on emitted entity/event/edge-meta payloads.

When helper chains are too deep, callers MAY capture `Source` upstream and forward it explicitly.

## File-by-file changes

### `/Users/amos/bearcove/peeps/crates/peeps/src/enabled.rs`

- enforce context-bearing methods for instrumented public APIs
- for all facade-covered wrappers, rename/remove inherent ergonomic names that clash with facade traits
- ensure location capture uses explicit `Source` override first, `Location::caller()` otherwise
- ensure every emission path carries `cx.manifest_dir` into attribution

### `/Users/amos/bearcove/peeps/crates/peeps/src/disabled.rs`

- mirror exact signatures (`*_with_cx` and required explicit-source + context forms for facade-surfaced operations)
- mirror the no-clashing-method rule so enabled/disabled APIs stay isomorphic
- keep zero-cost behavior

### `/Users/amos/bearcove/peeps/crates/peeps-types/src/primitives.rs`

- add contextual inference API that accepts `manifest_dir`
- stop relying on process-global root for new attribution path
- keep old global-root helpers only as temporary migration shims
- require cache keys to include `manifest_dir` (not source-only cache keys)

### `/Users/amos/bearcove/peeps/crates/peeps-types/src/entities.rs`
### `/Users/amos/bearcove/peeps/crates/peeps-types/src/scopes.rs`
### `/Users/amos/bearcove/peeps/crates/peeps-types/src/edges.rs`

- route builder-time source/krate fill through contextual inference APIs
- preserve explicit `krate` overrides when provided

### callsite adopters (workspace + dependent crates)

- update direct peeps function usage to pass context/source as required
- prefer generated facade paths where available
- preserve existing runtime `init!()` callsites during migration

## Acceptance tests

The implementation is complete only when all scenarios below pass.

1. Direct call, single crate:
   - `Mutex::lock_with_cx(PeepsContext::new(env!("CARGO_MANIFEST_DIR")))`
   - expected: with no explicit source override, `source` points to the direct callsite via `#[track_caller]`; `krate` is that crate

2. Wrapper without good caller threading:
   - wrapper calls lower helper without `#[track_caller]`
   - expected: explicit upstream `Source` forwarding restores user callsite

3. Cross-crate value usage via facade:
   - crate A creates `::peeps::Mutex<T>`
   - crate B imports `use crate::peeps::prelude::*;` then calls `.lock()` through B facade trait
   - expected: attribution uses B manifest root for crate inference
   - this scenario is a representative case; implementation coverage MUST include the full facade-covered wrapper API surface, not only `Mutex`

4. Non-`Mutex` facade interception proof:
   - use `RwLock<T>` via facade in crate B (import `use crate::peeps::prelude::*;`, call `.read()` or `.write()`)
   - expected: method resolves through facade trait path (no inherent-name clash)
   - expected: attribution uses B manifest root and B callsite

5. Two crates, same relative source path (cache isolation regression test):
   - crate A and crate B both emit relative source `src/lib.rs:<line>`
   - both run in one process
   - expected: cache keys isolate by `manifest_dir`; inferred crate names are correct per crate

6. Relative internal source path:
   - source looks like `crates/.../file.rs:line`
   - expected: resolved through `cx.manifest_dir`, crate inference succeeds if file exists

7. Missing/unresolvable source file:
   - expected: no panic, `krate = None`, source preserved

8. Direct low-level API call without explicit source:
   - call `*_with_cx` outside facade from a known wrapper
   - expected: attribution uses the nearest `#[track_caller]` boundary location (often wrapper location when caller threading is weak)
   - expected: docs mark this as lower-fidelity than facade + explicit source forwarding

## Rollout plan

1. API stabilization pass:
   - finalize `PeepsContext` + `Source` signatures in enabled/disabled
2. Attribution-core pass:
   - implement contextual inference in peeps-types
3. Callsite pass:
   - migrate peeps + known dependent crates
4. Facade pass:
   - add `facade!()` generated extension traits
5. Cleanup pass:
   - remove temporary global-root dependence from primary path

## Compatibility + versioning policy

1. Clashing inherent ergonomic method names have zero compatibility window.
   - when a wrapper is moved under facade interception, clashing inherent names are removed in that same release.
2. Compatibility shims are allowed only for non-clashing names.
   - they MUST be deprecated immediately
   - in this pre-1.0 repo (`0.x`), they MUST be removed by the next minor release
3. Release notes MUST include:
   - mapping from removed inherent names to facade/prelude usage
   - any temporary non-clashing shim names and their planned removal release

## Done criteria

Work is done when all are true:

1. Acceptance tests above are implemented and passing.
2. New attribution code path does not depend on process-global inference root.
3. Cross-crate facade path proves caller-crate context in test.
4. Cache key includes `manifest_dir` and passes same-relative-path multi-crate test.
5. Snapshot validation shows no regression in source quality and reduced wrapper leakage.
6. Docs and API signatures match shipped behavior.
