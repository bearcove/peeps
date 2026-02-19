Everything related to tracking where (in the source code) something happened.

This is hard because we cannot rely on everything being macros and `Location::caller()`
sometimes returns relative paths for files.

So the model is split in two at the facade boundary, then joined immediately:

1. `SourceLeft`: ambient crate context (`$CARGO_MANIFEST_DIR`)
2. `SourceRight`: unqualified callsite (`Location::caller()`)
3. `Source`: resolved source (`left.join(right)`)

`SourceLeft`:

- Where it comes from: facade-expanded crate constant built from `env!("CARGO_MANIFEST_DIR")`
- When it is captured: compile time in the calling crate
- Where it is used: only to resolve `SourceRight` into `Source` (path + crate identity)

`SourceRight`:

- Where it comes from: `Location::caller()` at wrapper callsite
- When it is captured: right before calling instrumentation
- Where it is used: joined with `SourceLeft`

`Source`:

- Where it comes from: `SourceLeft::join(SourceRight)`
- When it is produced: inside generated extension-trait method bodies
- Where it is used: passed to hidden impl methods, then written into events/entities

Example target shape:

```rust
pub mod peeps {
    pub const PEEPS_SOURCE_LEFT: ::peeps::SourceLeft =
        ::peeps::SourceLeft::new(env!("CARGO_MANIFEST_DIR"));

    impl<T> MutexExt<T> for ::peeps::Mutex<T> {
        #[track_caller]
        fn lock(&self) -> ::peeps::MutexGuard<'_, T> {
            self._lock(PEEPS_SOURCE_LEFT.resolve())
        }
    }
}
```

Important: hidden impl methods should take joined `Source`, not split left/right arguments.
