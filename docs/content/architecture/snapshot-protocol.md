+++
title = "Snapshot Protocol"
weight = 1
+++

## Pull snapshot protocol

peeps uses a pull model for snapshots. The server asks all connected processes "give me your graph right now" and assembles one consistent point-in-time view.

### Why pull, not push

A push/streaming model would give you a firehose of events with no consistent point-in-time view. With pull, you get a snapshot that represents "the world at time T" across all connected processes simultaneously. This makes cross-process analysis meaningful.

### Client side

A background task is spawned when `PEEPS_DASHBOARD=<addr>` is set. It connects via TCP to the peeps-web server.

**Wire format:** 4-byte big-endian `u32` length prefix, then a JSON payload.

The client receives a `SnapshotRequest`:

```json
{
  "type": "snapshot_request",
  "snapshot_id": 42,
  "timeout_ms": 5000
}
```

And responds with a `GraphReply`:

```json
{
  "type": "graph_reply",
  "snapshot_id": 42,
  "process": "my-service",
  "pid": 12345,
  "graph": { ... }
}
```

The `graph` field contains all currently live nodes and edges from that process's registry. It is `null` if the process encountered an error building the snapshot.

### Server side

The TCP listener binds to `PEEPS_LISTEN` (default `127.0.0.1:9119`). It maintains a registry of connected processes.

On snapshot trigger:

1. Allocate a new `snapshot_id`
2. Broadcast `SnapshotRequest` to all connections
3. Wait for responses with a timeout of `DEFAULT_TIMEOUT_MS` (5000ms) plus a 500ms grace period
4. Processes that don't respond in time are recorded as `timeout` in `snapshot_processes`
5. Processes that disconnect are recorded as `disconnected`
6. All received nodes and edges are persisted to SQLite under the new `snapshot_id`

Each connection uses separate reader/writer tasks communicating via an `mpsc` channel (capacity 32).

### HTTP trigger

`POST /api/jump-now` on `PEEPS_HTTP` (default `127.0.0.1:9130`) triggers a new snapshot and returns the `snapshot_id` along with a summary of process responses.
