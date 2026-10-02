use super::*;
use rusqlite::{Connection, params};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

fn record(sequence: u64, source: &str, session_id: u64) -> CallRecord {
    CallRecord {
        sequence,
        started_at_ms: 7_000_000,
        completed_at_ms: 7_060_000,
        tool: "read_file".into(),
        outcome: Outcome::Success,
        duration_ms: 12.5,
        request_bytes: 10,
        response_bytes: 20,
        origin: Origin::from_storage(source, "client_reported", "tunnel"),
        session_id,
    }
}

fn version_two_database(path: &Path) {
    let mut connection = Connection::open(path).unwrap();
    migrations::apply(&mut connection, &migrations::MIGRATIONS[..2]).unwrap();
    connection.execute_batch("INSERT INTO completed_calls (sequence, started_at_ms, completed_at_ms, tool, outcome, duration_ms, request_bytes, response_bytes) VALUES (51, 100, 101, 'read_file', 'success', 0.5, 2, 3)").unwrap();
    drop(connection);
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn version_two_upgrade_preserves_records_as_unknown_origin_and_legacy_run() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    version_two_database(&path);
    let store = Store::open(&path).unwrap();
    let history = store.load(101).unwrap();
    assert_eq!(store.schema_version(), 3);
    assert_eq!(history.totals.calls, 1);
    assert_eq!(history.request_bytes, 2);
    assert_eq!(history.last_sequence, 51);
    assert_eq!(history.last_session_id, 0);
    assert!(history.recent_calls[0].origin == Origin::default());
    assert_eq!(history.recent_calls[0].session_id, 0);
    assert_eq!(history.origins.len(), 1);
    assert!(history.origins[0].origin == Origin::default());
    assert_eq!(history.origins[0].totals.calls, 1);
    drop(store);
    let reopened = Store::open(&path).unwrap().load(101).unwrap();
    assert_eq!(reopened.recent_calls.len(), 1);
    assert_eq!(reopened.last_sequence, 51);
}

#[test]
fn origin_and_run_restore_exactly_and_lifetime_groups_cover_expired_records() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let store = Store::open(&path).unwrap();
    let mut first = record(1, "codex_cloud", 37);
    first.origin = Origin::from_storage("codex_cloud", "operator_configured", "stdio");
    first.outcome = Outcome::ToolError;
    store.append(&first).unwrap();
    for sequence in 2..=1_004 {
        store.append(&record(sequence, "chatgpt_work", 8)).unwrap();
    }
    drop(store);
    let history = Store::open(&path).unwrap().load(7_100_000).unwrap();
    assert_eq!(history.last_session_id, 37);
    assert_eq!(history.recent_calls.len(), 1_000);
    assert_eq!(history.recent_calls[0].sequence, 5);
    assert_eq!(history.recent_calls[999].session_id, 8);
    assert!(history.recent_calls[999].origin == record(2, "chatgpt_work", 8).origin);
    assert_eq!(history.origins.len(), 2);
    let cloud = history
        .origins
        .iter()
        .find(|group| group.origin.source.as_str() == "codex_cloud")
        .unwrap();
    assert!(cloud.origin == first.origin);
    assert_eq!(cloud.totals.calls, 1);
    assert_eq!(cloud.totals.errors, 1);
    assert_eq!(cloud.totals.duration_ms, 12.5);
    let work = history
        .origins
        .iter()
        .find(|group| group.origin.source.as_str() == "chatgpt_work")
        .unwrap();
    assert_eq!(work.totals.calls, 1_003);
    assert_eq!(work.totals.successes, 1_003);
}

#[test]
fn arbitrary_origin_values_are_never_written_and_restore_merges_fixed_keys() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let store = Store::open(&path).unwrap();
    let mut call = record(1, "sk-private-source", 1);
    call.origin = Origin::from_storage(
        "sk-private-source",
        "private-attribution",
        "private-transport",
    );
    store.append(&call).unwrap();
    store.append(&record(2, "unknown", 2)).unwrap();
    let connection = Connection::open(&path).unwrap();
    let saved: (String, String, String) = connection
        .query_row(
            "SELECT source, evidence, transport FROM completed_calls WHERE sequence = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        saved,
        ("unknown".into(), "unknown".into(), "unknown".into())
    );
    connection
        .pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    connection.execute("UPDATE completed_calls SET source = 'sk-private-db-' || sequence, evidence = 'private-db-attribution', transport = 'private-db-transport'", []).unwrap();
    let restored = store.load(7_100_000).unwrap();
    assert!(
        restored
            .recent_calls
            .iter()
            .all(|call| call.origin == Origin::default())
    );
    assert_eq!(restored.origins.len(), 1);
    assert!(restored.origins[0].origin == Origin::default());
    assert_eq!(restored.origins[0].totals.calls, 2);
}

#[test]
fn origin_constraints_and_session_numeric_ranges_reject_invalid_metadata() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let store = Store::open(&path).unwrap();
    let connection = Connection::open(&path).unwrap();
    for column in ["source", "evidence", "transport"] {
        let sql = format!(
            "INSERT INTO completed_calls (sequence, started_at_ms, completed_at_ms, tool, outcome, duration_ms, request_bytes, response_bytes, {column}) VALUES (1, 100, 101, 'read_file', 'success', 0.5, 2, 3, ?1)"
        );
        let error = connection.execute(&sql, ["private-invalid"]);
        assert!(
            matches!(error, Err(rusqlite::Error::SqliteFailure(_, Some(ref message))) if message.contains("CHECK constraint failed"))
        );
    }
    let insert = "INSERT INTO completed_calls (sequence, started_at_ms, completed_at_ms, tool, outcome, duration_ms, request_bytes, response_bytes, session_id) VALUES (1, 100, 101, 'read_file', 'success', 0.5, 2, 3, ?1)";
    assert!(connection.execute(insert, params![-1]).is_err());
    assert!(store.append(&record(1, "codex", u64::MAX)).is_err());
    assert_eq!(store.load(7_100_000).unwrap().totals.calls, 0);
}

#[test]
fn every_supported_source_roundtrips_through_the_database_constraints() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let store = Store::open(&path).unwrap();
    let sources = [
        "unknown",
        "chatgpt",
        "chatgpt_work",
        "codex",
        "codex_cloud",
        "openai_dot",
    ];
    for (index, source) in sources.iter().enumerate() {
        store.append(&record(index as u64 + 1, source, 1)).unwrap();
    }
    drop(store);
    let history = Store::open(&path).unwrap().load(7_100_000).unwrap();
    assert_eq!(history.origins.len(), sources.len());
    for (call, source) in history.recent_calls.iter().zip(sources) {
        assert_eq!(call.origin.source.as_str(), source);
        assert_eq!(call.session_id, 1);
    }
}
