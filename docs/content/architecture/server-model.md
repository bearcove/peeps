+++
title = "Server Model"
weight = 2
+++

## Dumb server, smart client

peeps-web deliberately keeps the server thin. The server's job is: accept connections, collect snapshots, persist to SQLite, serve the frontend. That's it.

The frontend issues raw SQL queries against the SQLite database via `POST /api/sql`. This means:

- **Rapid UI iteration** — change a query, reload the page. No backend rebuild needed.
- **Ad hoc debugging** — when the built-in views don't show what you need, write a query.
- **LLM-assisted investigation** — paste your schema and ask an LLM to write diagnostic queries. It works because the query interface is just SQL.

### Safety model

Queries run through several layers of protection:

- **Read-only authorizer** — an authorizer callback allows only `Read`, `Select`, `Function`, and `Recursive` operations. All writes are blocked. Access to `sqlite_master` and `sqlite_temp_master` is also blocked.
- **Snapshot scoping** — queries run against TEMP VIEWs (`nodes`, `edges`, `unresolved_edges`, `snapshot_processes`) that filter to a single `snapshot_id`. Direct access to `main.*` tables is rejected.
- **Execution timeout** — a progress handler checks a deadline every 1000 SQLite VM operations, enforcing a 750ms execution limit.
- **Multi-statement rejection** — only single SQL statements are accepted.
- **Result caps** — responses are limited to 5000 rows and 4 MiB of serialized JSON.

### API

`POST /api/jump-now` — trigger a new snapshot.

`POST /api/sql` — execute a SQL query against the current snapshot.

Request body:

```json
{
  "snapshot_id": 42,
  "sql": "SELECT id, kind, attrs_json FROM nodes WHERE kind = 'HttpConnection'"
}
```

Response:

```json
{
  "columns": ["id", "kind", "attrs_json"],
  "rows": [["HttpConnection:01J...", "HttpConnection", "{...}"]],
  "truncated": false
}
```
