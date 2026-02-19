
## Counter-examples found in the wild

### Unnecessary `#[allow(deprecated)]`

```rust
pub fn channel<T>(name: impl Into<String>, buffer: usize) -> (Sender<T>, Receiver<T>) {
    #[allow(deprecated)]
    peeps::channel(name, buffer, peeps::Source::caller())
}
```

`peeps::channel` is not deprecated. The `#[allow(deprecated)]` was added during a refactor and is no longer needed.

### Unnecessary `#[track_caller]`

```rust
#[track_caller]
pub fn get(&self) -> Option<&T> {
    self.inner.get()
}
```

`self.inner.get()` is Tokio's `get()`, which does not use caller location. The `#[track_caller]` does nothing useful here.

### Unnecessary manual async fn

```rust
#[allow(clippy::manual_async_fn)]
pub fn get_or_init_with_source<'a, F, Fut>(
    &'a self,
    f: F,
    source: Source,
    cx: PeepsContext,
) -> impl Future<Output = &'a T> + 'a
where
    F: FnOnce() -> Fut + 'a,
    Fut: Future<Output = T> + 'a,
{
    async move { ... }
}
```

Wrong on two levels: the only good reason for a manual async fn would be to use `#[track_caller]` (which doesn't work on async fns), but this function doesn't even use `#[track_caller]` — `source` is just passed in as a parameter. So it's a manual async fn for no reason, with an `#[allow(clippy::manual_async_fn)]` to suppress the lint telling you it's unnecessary.
