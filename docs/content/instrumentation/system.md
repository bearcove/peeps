+++
title = "Commands, file ops, net ops"
weight = 6
+++

## Command

`peeps::Command` wraps `tokio::process::Command` and `tokio::process::Child`.

- Creates a `Command` node on spawn
- Tracks: `program`, `args` (truncated to 200 chars), `cwd`, `env_count`, `pid`, `exit_code`, `exit_signal`, `elapsed_ns`, errors
- Updated on exit, removed on `Drop`

## File operations

`peeps::fs::create_dir_all` and file read/write wrappers.

- Creates a `FileOp` node per operation
- Tracks: bytes read/written, `elapsed_ns`, result (ok/error)

## Network operations

Four wrappers:

| Function | Node kind | What it wraps |
|----------|-----------|---------------|
| `peeps::net::connect` | `NetConnect` | Connection establishment |
| `peeps::net::accept` | `NetAccept` | Socket accept |
| `peeps::net::readable` | `NetReadable` | Readability wait |
| `peeps::net::writable` | `NetWritable` | Writability wait |

All track: `endpoint`, `transport`, `elapsed_ns`.
