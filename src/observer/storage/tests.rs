use super::*;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use tempfile::TempDir;

fn record(sequence: u64) -> CallRecord {
    CallRecord {
        sequence,
        started_at_ms: 7_000_000,
        completed_at_ms: 7_060_000,
        tool: "read_file".into(),
        outcome: Outcome::Success,
        duration_ms: 12.5,
        request_bytes: 10,
        response_bytes: 20,
    }
}

#[test]
fn startup_initializes_embedded_migrations_once_and_reopens_existing_data() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("volume/nested/observer.sqlite3");
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version(), SCHEMA_VERSION);
    store.append(&record(7)).unwrap();
    drop(store);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.schema_version(), SCHEMA_VERSION);
    let history = reopened.load(7_100_000).unwrap();
    assert_eq!(history.totals.calls, 1);
    assert_eq!(history.last_sequence, 7);
    assert_eq!(history.recent_calls[0].sequence, 7);
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
}

#[test]
fn persisted_history_keeps_lifetime_totals_and_bounded_completion_order() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let store = Store::open(&path).unwrap();
    for sequence in 1..=1_205 {
        let mut call = record(sequence);
        call.outcome = if sequence % 2 == 0 {
            Outcome::ToolError
        } else {
            Outcome::Success
        };
        store.append(&call).unwrap();
    }
    let mut out_of_order = record(1_999);
    out_of_order.completed_at_ms = 6_000_000;
    store.append(&out_of_order).unwrap();
    store.append(&record(1_300)).unwrap();
    drop(store);
    let history = Store::open(&path).unwrap().load(7_100_000).unwrap();
    assert_eq!(history.totals.calls, 1_207);
    assert_eq!(history.totals.successes, 605);
    assert_eq!(history.totals.errors, 602);
    assert_eq!(history.request_bytes, 12_070);
    assert_eq!(history.response_bytes, 24_140);
    assert_eq!(history.totals.duration_ms, 15_087.5);
    assert_eq!(history.last_sequence, 1_999);
    assert_eq!(history.recent_calls.len(), 1_000);
    assert_eq!(history.recent_calls[0].sequence, 208);
    assert_eq!(history.recent_calls[999].sequence, 1_300);
    assert_eq!(history.tools[0].totals.calls, 1_207);
    assert_eq!(
        history
            .minutes
            .iter()
            .map(|minute| minute.calls)
            .sum::<u64>(),
        1_207
    );
}

#[test]
fn unknown_names_are_normalized_before_storage_and_invalid_metadata_is_rejected() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let store = Store::open(&path).unwrap();
    let mut call = record(1);
    call.tool = "sk-secret-/Users/private/.env".into();
    call.outcome = Outcome::ProtocolError;
    store.append(&call).unwrap();
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        call.sequence += 1;
        call.duration_ms = invalid;
        assert!(store.append(&call).is_err());
    }
    call.duration_ms = 0.0;
    call.sequence = u64::MAX;
    assert!(store.append(&call).is_err());
    let history = store.load(7_100_000).unwrap();
    assert_eq!(history.totals.calls, 1);
    assert_eq!(history.recent_calls[0].tool, "unknown_tool");
    assert!(matches!(
        history.recent_calls[0].outcome,
        Outcome::ProtocolError
    ));
    drop(store);
    assert!(!String::from_utf8_lossy(&fs::read(path).unwrap()).contains("sk-secret"));
}

#[test]
fn database_lock_is_nonblocking_rejects_hardlinks_and_is_released_on_drop() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let store = Store::open(&path).unwrap();
    let start = std::time::Instant::now();
    let error = Store::open(&path).err().expect("second writer must fail");
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert!(error.to_string().contains("another MCP observer"));
    assert_eq!(
        Store::open(&path).err().unwrap().kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    let alias = root.path().join("alias.sqlite3");
    fs::hard_link(&path, &alias).unwrap();
    let error = Store::open(&alias).err().expect("hardlink alias must fail");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.to_string().contains("hardlinks"));
    fs::remove_file(alias).unwrap();
    drop(store);
    assert!(Store::open(&path).is_ok());
}

#[test]
fn database_and_sidecar_symlinks_and_public_permissions_are_rejected() {
    let root = TempDir::new().unwrap();
    let private = root.path().join("private");
    fs::write(&private, b"private-secret").unwrap();
    let path = root.path().join("observer.sqlite3");
    symlink(&private, &path).unwrap();
    assert!(Store::open(&path).is_err());
    fs::remove_file(&path).unwrap();
    symlink(&private, path.with_file_name("observer.sqlite3-wal")).unwrap();
    assert!(Store::open(&path).is_err());
    fs::remove_file(path.with_file_name("observer.sqlite3-wal")).unwrap();
    fs::write(&path, b"").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(fs::read(&private).unwrap(), b"private-secret");
}

#[test]
fn startup_rejects_newer_schema_without_changing_it() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.pragma_update(None, "user_version", 99).unwrap();
    drop(connection);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let error = Store::open(&path).err().expect("future schema must fail");
    assert!(error.to_string().contains("newer"));
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 99);
}

#[test]
fn non_sqlite_file_is_preserved_when_initialization_fails() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    fs::write(&path, b"this is not sqlite and must stay untouched").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(
        fs::read(&path).unwrap(),
        b"this is not sqlite and must stay untouched"
    );
}

#[test]
fn startup_rejects_an_unrelated_sqlite_database_without_modifying_it() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("another-project.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE accounts (name TEXT); INSERT INTO accounts VALUES ('preserved');",
        )
        .unwrap();
    drop(connection);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let error = Store::open(&path)
        .err()
        .expect("unrelated database must fail");
    assert!(error.to_string().contains("belongs"));
    let connection = rusqlite::Connection::open(&path).unwrap();
    let name: String = connection
        .query_row("SELECT name FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(name, "preserved");
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 0);
    assert!(connection.prepare("SELECT * FROM completed_calls").is_err());
}
