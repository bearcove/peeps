# moire-web modularization plan

`crates/moire-web/src/main.rs` is currently a 4k+ line mixed-concern file.
This makes normal changes risky: every edit drags in unrelated state, and
database/path/proxy/symbolication concerns leak across handler boundaries.

This plan is the source of truth for splitting it safely, one module at a time,
without changing behavior.

## Ground rules

- No behavior changes during structural moves.
- Keep each step small enough to review.
- Keep compiling and run `cargo test -p moire-web` (or narrower checks) after each step.
- `main.rs` becomes bootstrap-only; all real logic moves into modules.
- Stop passing raw `PathBuf` database paths across layers; introduce a DB boundary type.

## Target shape

`crates/moire-web/src/`

- `main.rs` (bootstrap only)
- `lib.rs` (module wiring and minimal exports)
- `app/`
  - `mod.rs` (state and app wiring)
  - `cli.rs`
  - `startup.rs`
- `api/`
  - `mod.rs`
  - `connections.rs`
  - `snapshot.rs`
  - `recording.rs`
  - `sql.rs`
- `db/`
  - `mod.rs` (`Db` facade)
  - `schema.rs`
  - `query.rs`
  - `persist.rs`
- `snapshot/`
  - `mod.rs`
  - `table.rs`
- `symbolication/`
  - `mod.rs`
  - `cache.rs`
  - `resolve.rs`
- `recording/`
  - `mod.rs`
  - `session.rs`
- `tcp/`
  - `mod.rs`
  - `handshake.rs`
  - `ingest.rs`
- `proxy/`
  - `mod.rs`
  - `vite.rs`
- `util/`
  - `mod.rs`
  - `http.rs`
  - `time.rs`

Exact filenames can shift, but concerns should not.

## Migration order (module-by-module)

### 1) Establish skeleton + move pure utilities

- Add `lib.rs` and module skeleton with stubs.
- Move helpers with no side effects first:
  - JSON response helpers
  - header copy/skip helpers
  - time/id conversion helpers
- Keep all call sites behavior-identical.

### 2) Extract DB boundary

- Add `db::Db` facade that owns the database location/open policy.
- Move schema init/reset and query/persist helpers into `db/`.
- Replace function signatures taking `&PathBuf` db paths with `&Db` / `Arc<Db>`.
- Progress:
  - Schema init/versioning, query packs, and persistence helpers (connections, backtraces, cuts, delta batches) now live in `db/*`.

### 3) Extract API handlers by concern

- Move HTTP handlers from `main.rs` into `api/*`.
- Handlers call into `db/`, `snapshot/`, `recording/`, `symbolication/`; no inline SQL.
- Progress:
  - SQL/query request handling moved to `api/sql.rs` with thin wrappers in `main.rs`.

### 4) Extract snapshot and symbolication

- Move snapshot table loading/merge logic into `snapshot/`.
- Move pending frame jobs, cache operations, and resolver logic into `symbolication/`.
- Progress:
  - Snapshot backtrace/frame catalog loading and frame-id tests moved to `snapshot/table.rs`.

### 5) Extract recording flow

- Move record start/stop/frame/export/import logic into `recording/`.
- Keep API layer as thin request/response orchestration.

### 6) Extract TCP ingest and proxy/dev-server logic

- Move TCP accept/handshake/message ingestion into `tcp/`.
- Move Vite/proxy/reaper into `proxy/`.

### 7) Shrink `main.rs` to bootstrap

- `main.rs` should only parse CLI, call startup, and run the app.

## Acceptance checks

- `main.rs` is small and readable (bootstrap only).
- No functions outside `db/` take a db path.
- API handlers are thin orchestration, not implementation.
- `cargo test -p moire-web` passes.
- Manual smoke checks still work:
  - health endpoint
  - connection ingest
  - snapshot endpoint
  - record start/stop

## Tracking checklist

- [x] Step 1 complete
- [x] Step 2 complete
- [ ] Step 3 complete
- [ ] Step 4 complete
- [ ] Step 5 complete
- [ ] Step 6 complete
- [ ] Step 7 complete
