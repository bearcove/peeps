# Repository Agent Notes

Read [`MANIFESTO.md`](MANIFESTO.md) first.

Summary: fail fast, loudly, and often. Validate assumptions, reject ambiguous state, and do not introduce silent fallbacks for required invariants.

## JavaScript/TypeScript Package Manager

- Use `pnpm` for all frontend dependency and script workflows in this repository.
- Do not use `npm`.
- Standard commands:
  - `pnpm install`
  - `pnpm build`
  - `pnpm test`

## Documentation Style

Read and follow `/Users/amos/bearcove/moire/TONE.md` before writing or editing documentation.

When the user provides copy, preserve cadence and phrasing. Do not smooth or rewrite by default; fix only obvious typos unless explicitly asked.

## Internals Index

- `examples/README.md` - Example authoring and runner contract (`just ex` flow, process-group lifecycle, multi-process examples).
- `docs/RESOURCES.md` - External references and technical material used while shaping instrumentation and model decisions.
- `internals/frame-pointer-trace-plan.md` - Source attribution replacement plan (still active).
