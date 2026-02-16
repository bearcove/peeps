+++
title = "Causality Stack"
weight = 4
+++

The causality stack is a task-local `Vec<String>` of node IDs, maintained per Tokio task using `tokio::task_local!`. It tracks which instrumented node is "currently executing" so that edges can be emitted automatically.

## Operations

### `push(node_id)`

Called when a `PeepableFuture` starts polling. If there's already a parent on the stack, emits a `Touches` edge from parent to this node. Pushes the node ID onto the stack.

### `pop()`

Called after poll completes. Restores the previous stack state.

### `with_top(f)`

Calls `f` with the current stack top (if any). Used internally to emit edges from the current context to a resource — for example, when a lock is acquired, `with_top` provides the "who" for the `Needs` edge.

### `scope(node_id, future)`

Wraps a future so that `node_id` is pushed before each poll and popped after. The node stays on top of the stack for the duration of every poll cycle. If no active stack scope exists, push/pop operations are no-ops.

This is how response nodes become the context for their handler — any nested future polled during handling will get a `Touches` edge to the response node.

### `ensure(future)`

Ensures the future runs with a task-local stack, preserving the parent context. Used when spawning tasks that should inherit stack context.

## How it works in practice

Consider an RPC handler:

1. A roam RPC arrives. A `Response` node is created.
2. `scope(response_id, handler_future)` wraps the handler.
3. The handler awaits a database query (a `Future` node).
4. On poll, `push` fires — the response node is on the stack, so a `Touches` edge is emitted from the response to the query future.
5. The query future awaits a channel recv. `with_top` provides the query future's ID, and a `Needs` edge is emitted from the query to the channel.

The result: a causal chain from "request arrived" through "handler polled query" through "query waited on channel" — all built automatically from the stack.
