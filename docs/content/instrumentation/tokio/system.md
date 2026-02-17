+++
title = "Commands, file ops, net ops"
weight = 6
+++

System instrumentation covers async boundaries where work leaves pure in-memory execution.

## Command operations

Process launch/wait is represented so external process latency and failures are visible in the same causal graph.

## File operations

File I/O operations become nodes that can explain blocking and throughput constraints.
Use `peeps::fs` wrappers (`write`, `read_*`, `File`, `OpenOptions`) so operations are represented in the graph.

## Network operations

Connection setup and readiness waits become graph-visible so transport-level stalls can be traced back to callers.

These nodes are typically short-lived and most useful when correlated with upstream futures via edges.
