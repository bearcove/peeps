Everything related to tracking where (in the source code) something happened.

This is hard because we cannot rely on everything being macros and Location::caller() sometimes
returns relative paths for files.

Therefore, the only way to do this is to force every crate that wants to use peeps types to call
a facade macro that expands to a bunch of extension traits that pass a crate configuration, to
everything.

Example:

```rust
peeps::facade!();

async fn blah(m: &peeps::Mutex<()>) {
    // actually MutexExt::lock(), defined above, which secretly passes
    // our CrateConfig, containing `/absolute/path/to/my/Cargo.toml`
    m.lock();
}
```

Model:

1. `SourceLeft`: ambient crate context (`$CARGO_MANIFEST_DIR`)
2. `SourceRight`: unqualified callsite (`Location::caller()`)
3. `Source`: joined value (`left + right`)

`SourceLeft`:

- Where it comes from: facade-expanded crate constant built from `env!("CARGO_MANIFEST_DIR")`
- When it is captured: compile time in the calling crate
- Where it is used: passed through wrapper APIs as ambient crate context for source resolution

`SourceRight`:

- Where it comes from: `Location::caller()` at wrapper callsite
- When it is captured: immediately before calling instrumented implementation
- Where it is used: joined with `SourceLeft` to resolve a concrete source string

`Source`:

- Where it comes from: `SourceLeft::join(SourceRight)` or `SourceRight::join(SourceLeft)`
- When it is produced: at instrumentation boundary where both halves are present
- Where it is used: event/entity source fields and crate inference

My first idea was to expand to extra parameters:

```rust
peeps::facade!();

// expands to:

const PEEPS_CRATE_CONFIG: ::peeps::CrateConfig {
    manifest_dir: env!("CARGO_MANIFEST_DIR"),
};

trait MutexExt {
    fn lock(&self) -> Blah {
        self.lock_with_cx(PEEPS_CRATE_CONFIG):
    }
}
```

That's the one to keep: explicit argument passing at the wrapper call site, then joining left and
right into a qualified `Source`.
