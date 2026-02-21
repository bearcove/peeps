# Claude Notes

Read [`MANIFESTO.md`](MANIFESTO.md) first.

Summary: fail fast, loudly, and often. Validate assumptions, reject ambiguous state, and do not introduce silent fallbacks for required invariants.

## TypeScript Type Generation

Frontend types are generated from Rust via `facet-typescript`. To regenerate:

```bash
cargo run -p moire-web --bin gen_frontend_types
```

Source: `crates/moire-web/src/bin/gen_frontend_types.rs`
Output: `frontend/src/api/types.generated.ts`

The generator adds types from `moire-types` (e.g. `SnapshotCutResponse`, `ConnectionsResponse`, etc.). If Rust types change, regenerate and check for TypeScript compilation errors.

