use super::*;
use migrations::{MIGRATIONS, Migration};
use rusqlite::Connection;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

#[test]
fn pending_migration_upgrades_version_one_without_replacing_data() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch(MIGRATIONS[0].sql).unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    connection.execute_batch("INSERT INTO completed_calls (sequence, started_at_ms, completed_at_ms, tool, outcome, duration_ms, request_bytes, response_bytes) VALUES (51, 100, 101, 'read_file', 'success', 0.5, 2, 3)").unwrap();
    drop(connection);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.schema_version(), SCHEMA_VERSION);
    assert_eq!(store.load(101).unwrap().recent_calls[0].sequence, 51);
    let connection = Connection::open(&path).unwrap();
    let indexes: u32 = connection.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE type = 'index' AND name LIKE 'completed_calls_%_idx'", [], |row| row.get(0)).unwrap();
    assert_eq!(indexes, 2);
}

#[test]
fn failed_migration_rolls_back_ddl_and_version_and_can_resume() {
    let mut connection = Connection::open_in_memory().unwrap();
    let broken = [
        Migration {
            version: 1,
            sql: MIGRATIONS[0].sql,
        },
        Migration {
            version: 2,
            sql: "CREATE TABLE temporary_step (value INTEGER); INSERT INTO missing_table VALUES (1);",
        },
    ];
    assert!(migrations::apply(&mut connection, &broken).is_err());
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
    assert!(connection.prepare("SELECT * FROM completed_calls").is_ok());
    assert!(connection.prepare("SELECT * FROM temporary_step").is_err());
    migrations::apply(&mut connection, MIGRATIONS).unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}

#[test]
fn migration_registry_rejects_gaps_before_applying_sql() {
    let mut connection = Connection::open_in_memory().unwrap();
    let gap = [Migration {
        version: 2,
        sql: "CREATE TABLE forbidden (value INTEGER);",
    }];
    assert!(migrations::apply(&mut connection, &gap).is_err());
    assert!(connection.prepare("SELECT * FROM forbidden").is_err());
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 0);
}

#[test]
fn database_checks_reject_invalid_metadata_and_loaded_names_are_normalized() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("observer.sqlite3");
    let store = Store::open(&path).unwrap();
    let connection = Connection::open(&path).unwrap();
    let insert = "INSERT INTO completed_calls (sequence, started_at_ms, completed_at_ms, tool, outcome, duration_ms, request_bytes, response_bytes) VALUES (1, 100, 101, 'read_file', ?1, ?2, ?3, 3)";
    for (outcome, duration, bytes) in [
        ("other", 0.1, 0),
        ("success", -1.0, 0),
        ("success", 0.1, -1),
    ] {
        assert!(
            connection
                .execute(insert, rusqlite::params![outcome, duration, bytes])
                .is_err()
        );
    }
    connection
        .execute(insert, rusqlite::params!["success", 0.5, 2])
        .unwrap();
    connection
        .execute(
            "UPDATE completed_calls SET tool = ?1",
            ["private-token-/Users/private"],
        )
        .unwrap();
    let history = store.load(101).unwrap();
    assert_eq!(history.recent_calls[0].tool, "unknown_tool");
    assert_eq!(history.tools[0].tool, "unknown_tool");
}
