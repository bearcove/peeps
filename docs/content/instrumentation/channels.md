+++
title = "Channels"
weight = 3
+++

peeps wraps all three Tokio channel types.

## mpsc

Both bounded (`peeps::channel(capacity)`) and unbounded (`peeps::unbounded_channel()`). Creates a `Tx` node and an `Rx` node.

## oneshot

`peeps::oneshot_channel()`. Creates a `Tx` and `Rx` node pair.

## watch

Wraps `tokio::sync::watch`.

## What they track

**mpsc attributes (on the Tx node):**

| Attribute | Description |
|-----------|-------------|
| `sent_total` | Total messages sent |
| `queue_len` | Current buffer occupation (`sent - received`) |
| `high_watermark` | Peak `queue_len` ever observed |
| `capacity` | Buffer capacity (bounded channels only) |
| `utilization` | `queue_len / capacity` as a ratio (bounded channels only) |
| `bounded` | Whether this is a bounded channel |
| `sender_count` | Number of live sender handles |
| `send_waiters` | Senders blocked waiting for capacity |
| `closed` | Whether the sender side is closed |
| `close_cause` | Why it closed (e.g., `all_senders_dropped`) |

**mpsc attributes (on the Rx node):**

| Attribute | Description |
|-----------|-------------|
| `received_total` | Total messages received |
| `recv_waiters` | Receivers blocked waiting for a message |
| `closed` | Whether the receiver side is closed |

**oneshot** tracks state as one of: `pending`, `sent`, `received`, `sender_dropped`.

**watch** tracks current value state and waiter counts.

## Edge behavior

- Receivers emit `needs` edges while waiting for messages.
- When a channel is closed (e.g., all senders dropped), `closed_by` edges record the cause.
