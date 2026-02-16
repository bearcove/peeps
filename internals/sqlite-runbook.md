# peeps-web SQLite raw query runbook

Use this when you need to inspect exactly what was persisted for a node/edge in a snapshot.

## Ports

- Frontend UI: `http://127.0.0.1:9131`
- Backend API: `http://127.0.0.1:9130`
- TCP ingest (framed JSON, non-HTTP): `127.0.0.1:9119`

## 1) Find the live DB file

`peeps-web` writes `peeps-web.sqlite` in its current working directory.

```bash
ps -p <PID> -o pid=,command=
lsof -a -p <PID> -d cwd
```

Then query:

```bash
sqlite3 /path/from-cwd/peeps-web.sqlite '.tables'
```

## 2) Check schema before querying

```bash
sqlite3 /path/to/peeps-web.sqlite "PRAGMA table_info(snapshots);"
sqlite3 /path/to/peeps-web.sqlite "PRAGMA table_info(nodes);"
sqlite3 /path/to/peeps-web.sqlite "PRAGMA table_info(edges);"
```

Current schema (at time of writing):
- `snapshots(snapshot_id, requested_at_ns, completed_at_ns, timeout_ms)`
- `nodes(snapshot_id, id, kind, process, proc_key, attrs_json)`
- `edges(snapshot_id, src_id, dst_id, kind, attrs_json)`

## 3) Pull latest snapshots

```bash
sqlite3 /path/to/peeps-web.sqlite \
  "SELECT snapshot_id, requested_at_ns, completed_at_ns, timeout_ms \
   FROM snapshots ORDER BY snapshot_id DESC LIMIT 20;"
```

## 4) Inspect one node across snapshots

```bash
NODE_ID='request:...'
sqlite3 /path/to/peeps-web.sqlite \
  "SELECT snapshot_id, id, kind, process, proc_key \
   FROM nodes \
   WHERE id='${NODE_ID}' \
   ORDER BY snapshot_id DESC;"
```

## 5) Inspect all edges touching a node

```bash
NODE_ID='request:...'
sqlite3 /path/to/peeps-web.sqlite \
  "SELECT snapshot_id, src_id, dst_id, kind \
   FROM edges \
   WHERE src_id='${NODE_ID}' OR dst_id='${NODE_ID}' \
   ORDER BY snapshot_id DESC;"
```

## 6) Quick sanity checks for missing links

Count edge kinds in latest snapshot:

```bash
sqlite3 /path/to/peeps-web.sqlite \
  "SELECT kind, COUNT(*) \
   FROM edges \
   WHERE snapshot_id=(SELECT MAX(snapshot_id) FROM snapshots) \
   GROUP BY kind \
   ORDER BY COUNT(*) DESC;"
```

Count nodes by kind in latest snapshot:

```bash
sqlite3 /path/to/peeps-web.sqlite \
  "SELECT kind, COUNT(*) \
   FROM nodes \
   WHERE snapshot_id=(SELECT MAX(snapshot_id) FROM snapshots) \
   GROUP BY kind \
   ORDER BY COUNT(*) DESC;"
```
