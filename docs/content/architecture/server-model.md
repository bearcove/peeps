+++
title = "Server Model"
weight = 2
+++

## Dumb server, smart client

peeps-web keeps the backend intentionally small. Its job is to collect snapshots, persist them, and expose a query surface. Most exploration logic lives in the client.

This design optimizes for iteration speed and debuggability:

- **Rapid UI iteration** — change a query, reload the page. No backend rebuild needed.
- **Ad hoc investigation** — if a built-in view is missing, SQL is the escape hatch.
- **Low ceremony** — no large server-side query layer to maintain.

### Safety model

`/api/sql` is intentionally constrained:

- Read-only query execution.
- Single-snapshot scoping.
- Time and result-size limits.
- Rejection of unsafe SQL patterns.

### HTTP surface

- `POST /api/jump-now`: request a fresh snapshot.
- `POST /api/sql`: run a read-only query within one snapshot scope.
