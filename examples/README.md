# Peeps examples

These examples live in-repo but outside the workspace crates so they are easy to run and tweak.

## Runner

Use the helper launcher to run dashboard backend + frontend + any example in one command:

```bash
scripts/run-example --list
scripts/run-example channel-full-stall
```

When you stop the runner (`Ctrl+C`), all child processes are stopped too.

## 1) Channel full stall (`tokio::sync::mpsc` behavior)

Path: `examples/channel-full-stall`

What it does:
- Creates a bounded channel with capacity `16`
- Sends `16` items to fill it
- Attempts a 17th send, which blocks forever because the receiver is intentionally stalled

### Run it

Terminal 1:

```bash
cargo run -p peeps-web
```

Terminal 2:

```bash
pnpm --dir crates/peeps-web/frontend dev
```

Terminal 3:

```bash
PEEPS_DASHBOARD=127.0.0.1:9119 \
  cargo run --manifest-path examples/channel-full-stall/Cargo.toml
```

Then open [http://127.0.0.1:9131](http://127.0.0.1:9131) and inspect the `demo.work_queue` channel nodes plus the `queue.send.blocked` task.
