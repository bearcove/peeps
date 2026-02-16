+++
title = "Data Model"
weight = 1
+++

peeps represents your program as a directed graph. The two building blocks are **nodes** and **edges**.

## Nodes

A node represents a runtime entity — a future being polled, a lock being held, a channel endpoint, an RPC request in flight.

Each node has four fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Globally unique. Format: `{kind}:{ulid}` for most nodes. |
| `kind` | `NodeKind` | One of 21 variants (see below). |
| `label` | `Option<String>` | Human-readable label, when available. |
| `attrs_json` | `String` | JSON with type-specific attributes and a `meta` sub-object for shared metadata. |

### ID formats

Most nodes use `{kind}:{ulid}` — e.g., `future:01ARZ3NDEKTSV4RRFFQ69G5FAV`. Some node types use different schemes:

- **Request**: `request:{span_id}` (span ID from caller metadata)
- **Response**: `response:{ulid}`
- **RemoteTx**: `remote_tx:{mother_request_ulid}:{channel_idx}:{dir}`
- **RemoteRx**: `remote_rx:{mother_request_ulid}:{channel_idx}:{dir}`

### 21 node kinds

Organized by category:

| Category | Kinds |
|----------|-------|
| **Async** | `Future`, `JoinSet` |
| **Sync** | `Semaphore`, `OnceCell`, `Notify` |
| **Timers** | `Sleep`, `Interval`, `Timeout` |
| **Channels** | `Tx`, `Rx`, `RemoteTx`, `RemoteRx` |
| **Locks** | `Lock` |
| **RPC** | `Request`, `Response` |
| **System** | `Command`, `FileOp`, `NetConnect`, `NetAccept`, `NetReadable`, `NetWritable` |

## Edges

An edge connects two nodes with a direction and a kind.

| Field | Type | Description |
|-------|------|-------------|
| `src` | `String` | Source node ID. |
| `dst` | `String` | Destination node ID. |
| `kind` | `EdgeKind` | One of `Needs`, `Touches`, `Spawned`, `ClosedBy`. |
| `attrs_json` | `String` | JSON-encoded edge attributes. |

Edge semantics are covered in detail in the [Edges](@/concepts/edges.md) page.
