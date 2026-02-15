//! Correctness validation checks for snapshot data.
//!
//! These checks verify canonical graph integrity after snapshot ingest:
//! - Edge endpoint integrity (every edge src/dst exists as a node)
//! - Edge model integrity (all edges have kind='needs')
//! - Unresolved edge classification (only map to non-responded processes)
//! - Node ID conventions
//! - Node kind coverage

use rusqlite::{Connection, params};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub snapshot_id: i64,
    pub checks: Vec<CheckResult>,
    pub all_passed: bool,
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub name: &'static str,
    pub passed: bool,
    pub details: String,
}

pub fn validate_snapshot(conn: &Connection, snapshot_id: i64) -> ValidationResult {
    let mut checks = Vec::new();

    checks.push(check_snapshot_exists(conn, snapshot_id));
    checks.push(check_snapshot_processes_recorded(conn, snapshot_id));
    checks.push(check_write_integrity(conn, snapshot_id));
    checks.push(check_edge_endpoint_integrity(conn, snapshot_id));
    checks.push(check_edge_kind_integrity(conn, snapshot_id));
    checks.push(check_unresolved_edge_classification(conn, snapshot_id));
    checks.push(check_node_id_conventions(conn, snapshot_id));
    checks.push(check_node_kind_coverage(conn, snapshot_id));

    let all_passed = checks.iter().all(|c| c.passed);

    ValidationResult {
        snapshot_id,
        checks,
        all_passed,
    }
}

/// Every edge src_id and dst_id must exist as a node in the same snapshot.
fn check_edge_endpoint_integrity(conn: &Connection, snapshot_id: i64) -> CheckResult {
    let mut stmt = conn
        .prepare(
            "SELECT e.src_id, e.dst_id
             FROM edges e
             LEFT JOIN nodes ns ON ns.snapshot_id = e.snapshot_id AND ns.id = e.src_id
             LEFT JOIN nodes nd ON nd.snapshot_id = e.snapshot_id AND nd.id = e.dst_id
             WHERE e.snapshot_id = ?1 AND (ns.id IS NULL OR nd.id IS NULL)
             LIMIT 10",
        )
        .unwrap();

    let dangling: Vec<(String, String)> = stmt
        .query_map(params![snapshot_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    if dangling.is_empty() {
        CheckResult {
            name: "edge_endpoint_integrity",
            passed: true,
            details: "all edge endpoints exist as nodes".into(),
        }
    } else {
        let examples: Vec<String> = dangling
            .iter()
            .map(|(s, d)| format!("{s} -> {d}"))
            .collect();
        CheckResult {
            name: "edge_endpoint_integrity",
            passed: false,
            details: format!("{} dangling edge(s): {}", dangling.len(), examples.join(", ")),
        }
    }
}

/// All edges must have kind='needs'. The CHECK constraint enforces this at write time,
/// but we verify it holds in the data.
fn check_edge_kind_integrity(conn: &Connection, snapshot_id: i64) -> CheckResult {
    let mut stmt = conn
        .prepare(
            "SELECT kind, COUNT(*)
             FROM edges
             WHERE snapshot_id = ?1
             GROUP BY kind
             HAVING kind <> 'needs'",
        )
        .unwrap();

    let violations: Vec<(String, i64)> = stmt
        .query_map(params![snapshot_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    if violations.is_empty() {
        CheckResult {
            name: "edge_kind_integrity",
            passed: true,
            details: "all edges have kind='needs'".into(),
        }
    } else {
        let bad: Vec<String> = violations
            .iter()
            .map(|(k, c)| format!("{k}: {c}"))
            .collect();
        CheckResult {
            name: "edge_kind_integrity",
            passed: false,
            details: format!("non-needs edge kinds: {}", bad.join(", ")),
        }
    }
}

/// Unresolved edges should only reference processes that did NOT respond.
/// If referenced_proc_key maps to a process with status='responded', that's a bug.
fn check_unresolved_edge_classification(conn: &Connection, snapshot_id: i64) -> CheckResult {
    let mut stmt = conn
        .prepare(
            "SELECT ue.src_id, ue.dst_id, ue.reason, sp.status
             FROM unresolved_edges ue
             LEFT JOIN snapshot_processes sp
               ON sp.snapshot_id = ue.snapshot_id
              AND sp.proc_key = ue.referenced_proc_key
             WHERE ue.snapshot_id = ?1
               AND sp.status = 'responded'
             LIMIT 10",
        )
        .unwrap();

    let violations: Vec<(String, String, String, String)> = stmt
        .query_map(params![snapshot_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect();

    if violations.is_empty() {
        CheckResult {
            name: "unresolved_edge_classification",
            passed: true,
            details: "unresolved edges correctly reference non-responded processes".into(),
        }
    } else {
        CheckResult {
            name: "unresolved_edge_classification",
            passed: false,
            details: format!(
                "{} unresolved edge(s) reference responded processes",
                violations.len()
            ),
        }
    }
}

/// Node IDs should follow the convention: `{kind}:{proc_key}:{rest...}`
fn check_node_id_conventions(conn: &Connection, snapshot_id: i64) -> CheckResult {
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, proc_key
             FROM nodes
             WHERE snapshot_id = ?1",
        )
        .unwrap();

    let mut bad_ids = Vec::new();
    let rows = stmt
        .query_map(params![snapshot_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap();

    for row in rows.filter_map(Result::ok) {
        let (id, kind, proc_key) = row;
        // ID should start with "{kind}:{proc_key}:"
        let expected_prefix = format!("{kind}:{proc_key}:");
        if !id.starts_with(&expected_prefix) {
            bad_ids.push(id);
            if bad_ids.len() >= 10 {
                break;
            }
        }
    }

    if bad_ids.is_empty() {
        CheckResult {
            name: "node_id_conventions",
            passed: true,
            details: "all node IDs follow {kind}:{proc_key}:... convention".into(),
        }
    } else {
        CheckResult {
            name: "node_id_conventions",
            passed: false,
            details: format!(
                "{} node(s) with non-conforming IDs: {}",
                bad_ids.len(),
                bad_ids.join(", ")
            ),
        }
    }
}

/// Every snapshot should have snapshot_processes entries recorded.
fn check_snapshot_processes_recorded(conn: &Connection, snapshot_id: i64) -> CheckResult {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snapshot_processes WHERE snapshot_id = ?1",
            params![snapshot_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if count > 0 {
        CheckResult {
            name: "snapshot_processes_recorded",
            passed: true,
            details: format!("{count} process(es) recorded for snapshot"),
        }
    } else {
        CheckResult {
            name: "snapshot_processes_recorded",
            passed: false,
            details: "no processes recorded for snapshot".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            PRAGMA journal_mode=WAL;

            CREATE TABLE snapshots (
                snapshot_id INTEGER PRIMARY KEY,
                requested_at_ns INTEGER NOT NULL,
                completed_at_ns INTEGER,
                timeout_ms INTEGER NOT NULL
            );

            CREATE TABLE snapshot_processes (
                snapshot_id INTEGER NOT NULL,
                process TEXT NOT NULL,
                pid INTEGER,
                proc_key TEXT NOT NULL,
                status TEXT NOT NULL,
                recv_at_ns INTEGER,
                error_text TEXT,
                PRIMARY KEY (snapshot_id, proc_key)
            );

            CREATE TABLE nodes (
                snapshot_id INTEGER NOT NULL,
                id TEXT NOT NULL,
                kind TEXT NOT NULL,
                process TEXT NOT NULL,
                proc_key TEXT NOT NULL,
                attrs_json TEXT NOT NULL,
                PRIMARY KEY (snapshot_id, id)
            );

            CREATE TABLE edges (
                snapshot_id INTEGER NOT NULL,
                src_id TEXT NOT NULL,
                dst_id TEXT NOT NULL,
                kind TEXT NOT NULL CHECK (kind = 'needs'),
                attrs_json TEXT NOT NULL,
                PRIMARY KEY (snapshot_id, src_id, dst_id)
            );

            CREATE TABLE unresolved_edges (
                snapshot_id INTEGER NOT NULL,
                src_id TEXT NOT NULL,
                dst_id TEXT NOT NULL,
                missing_side TEXT NOT NULL,
                reason TEXT NOT NULL,
                referenced_proc_key TEXT,
                attrs_json TEXT NOT NULL,
                PRIMARY KEY (snapshot_id, src_id, dst_id)
            );

            CREATE TABLE ingest_events (
                event_id INTEGER PRIMARY KEY,
                event_at_ns INTEGER NOT NULL,
                snapshot_id INTEGER,
                process TEXT,
                pid INTEGER,
                proc_key TEXT,
                event_kind TEXT NOT NULL,
                detail TEXT NOT NULL
            );
            ",
        )
        .unwrap();
        conn
    }

    #[test]
    fn valid_snapshot_passes_all_checks() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO snapshots (snapshot_id, requested_at_ns, completed_at_ns, timeout_ms) VALUES (1, 100, 200, 1500)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshot_processes (snapshot_id, process, pid, proc_key, status, recv_at_ns) VALUES (1, 'app', 1234, 'app-1234', 'responded', 150)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO nodes (snapshot_id, id, kind, process, proc_key, attrs_json) VALUES (1, 'task:app-1234:1', 'task', 'app', 'app-1234', '{}')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO nodes (snapshot_id, id, kind, process, proc_key, attrs_json) VALUES (1, 'future:app-1234:10', 'future', 'app', 'app-1234', '{}')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO edges (snapshot_id, src_id, dst_id, kind, attrs_json) VALUES (1, 'task:app-1234:1', 'future:app-1234:10', 'needs', '{}')",
            [],
        ).unwrap();

        let result = validate_snapshot(&conn, 1);
        assert!(result.all_passed, "checks: {:?}", result.checks);
    }

    #[test]
    fn detects_bad_node_id_convention() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO snapshots (snapshot_id, requested_at_ns, completed_at_ns, timeout_ms) VALUES (1, 100, 200, 1500)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshot_processes (snapshot_id, process, pid, proc_key, status) VALUES (1, 'app', 1234, 'app-1234', 'responded')",
            [],
        ).unwrap();
        // Node ID doesn't match kind:proc_key convention
        conn.execute(
            "INSERT INTO nodes (snapshot_id, id, kind, process, proc_key, attrs_json) VALUES (1, 'bad-id-format', 'task', 'app', 'app-1234', '{}')",
            [],
        ).unwrap();

        let result = validate_snapshot(&conn, 1);
        let id_check = result.checks.iter().find(|c| c.name == "node_id_conventions").unwrap();
        assert!(!id_check.passed);
    }

    #[test]
    fn empty_snapshot_reports_no_processes() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO snapshots (snapshot_id, requested_at_ns, timeout_ms) VALUES (1, 100, 1500)",
            [],
        ).unwrap();

        let result = validate_snapshot(&conn, 1);
        let proc_check = result.checks.iter().find(|c| c.name == "snapshot_processes_recorded").unwrap();
        assert!(!proc_check.passed);
    }

    #[test]
    fn detects_unresolved_edge_pointing_to_responded_process() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO snapshots (snapshot_id, requested_at_ns, completed_at_ns, timeout_ms) VALUES (1, 100, 200, 1500)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO snapshot_processes (snapshot_id, process, pid, proc_key, status) VALUES (1, 'app', 1234, 'app-1234', 'responded')",
            [],
        ).unwrap();
        // This unresolved edge points to a process that responded — should be flagged
        conn.execute(
            "INSERT INTO unresolved_edges (snapshot_id, src_id, dst_id, missing_side, reason, referenced_proc_key, attrs_json)
             VALUES (1, 'task:app-1234:1', 'future:app-1234:99', 'dst', 'referenced_proc_missing', 'app-1234', '{}')",
            [],
        ).unwrap();

        let result = validate_snapshot(&conn, 1);
        let ue_check = result.checks.iter().find(|c| c.name == "unresolved_edge_classification").unwrap();
        assert!(!ue_check.passed);
    }
}
