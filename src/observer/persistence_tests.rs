use super::*;

#[test]
fn completed_history_restores_aggregates_and_continues_sequence_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("observer.sqlite3");
    let first = Observer::persistent(&database).unwrap();
    first.finish(first.begin("read_file", 12), Outcome::Success, 20);
    let unfinished = first.begin("write_file", 100);
    first.finish(
        first.begin("private-tool-name", 30),
        Outcome::ProtocolError,
        40,
    );
    let before_restart = first.snapshot();
    assert_eq!(before_restart["summary"]["total_calls"], 3);
    drop(unfinished);
    drop(first);

    let restored = Observer::persistent(&database).unwrap();
    let snapshot = restored.snapshot();
    assert_eq!(snapshot["summary"]["total_calls"], 2);
    assert_eq!(snapshot["summary"]["successes"], 1);
    assert_eq!(snapshot["summary"]["errors"], 1);
    assert_eq!(snapshot["summary"]["active_calls"], 0);
    assert_eq!(snapshot["summary"]["request_bytes"], 42);
    assert_eq!(snapshot["summary"]["response_bytes"], 60);
    assert_eq!(
        snapshot["summary"]["average_duration_ms"],
        before_restart["summary"]["average_duration_ms"]
    );
    assert_eq!(
        snapshot["summary"]["recent_p95_duration_ms"],
        before_restart["summary"]["recent_p95_duration_ms"]
    );
    assert_eq!(snapshot["recent_calls"], before_restart["recent_calls"]);
    let timeline_calls: u64 = snapshot["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|minute| minute["calls"].as_u64().unwrap())
        .sum();
    assert_eq!(timeline_calls, 2);
    assert_eq!(snapshot["recent_calls"][0]["tool"], "unknown_tool");
    assert!(!snapshot.to_string().contains("private-tool-name"));
    assert_eq!(snapshot["storage"]["kind"], "sqlite");
    assert_eq!(snapshot["storage"]["write_failures"], 0);
    assert_eq!(snapshot["storage"]["healthy"], true);
    assert!(snapshot["storage"]["schema_version"].as_u64().unwrap() > 0);
    assert!(
        !snapshot
            .to_string()
            .contains(&database.to_string_lossy().to_string())
    );

    restored.finish(restored.begin("read_file", 5), Outcome::ToolError, 6);
    let latest = restored.snapshot();
    assert_eq!(latest["recent_calls"][0]["sequence"], 4);
    assert_eq!(latest["summary"]["total_calls"], 3);
    assert_eq!(tool(&latest, "read_file")["calls"], 2);
    assert_eq!(tool(&latest, "read_file")["successes"], 1);
    assert_eq!(tool(&latest, "read_file")["errors"], 1);
    assert_eq!(tool(&latest, "unknown_tool")["calls"], 1);
}

#[test]
fn in_memory_observers_report_storage_health_without_claiming_persistence() {
    let observer = Observer::new();
    assert_eq!(
        observer.snapshot()["storage"],
        json!({"kind": "memory", "schema_version": 0, "write_failures": 0, "healthy": true})
    );
}

#[test]
fn write_failures_preserve_live_metrics_and_report_recovery_without_private_errors() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("observer.sqlite3");
    let observer = Observer::persistent(&database).unwrap();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_observation BEFORE INSERT ON completed_calls
         BEGIN SELECT RAISE(ABORT, 'private-runtime-storage-message'); END;",
        )
        .unwrap();

    observer.finish(observer.begin("read_file", 2), Outcome::Success, 3);
    let failed = observer.snapshot();
    assert_eq!(failed["summary"]["successes"], 1);
    assert_eq!(failed["summary"]["active_calls"], 0);
    assert_eq!(failed["storage"]["write_failures"], 1);
    assert_eq!(failed["storage"]["healthy"], false);
    assert!(
        !failed
            .to_string()
            .contains("private-runtime-storage-message")
    );

    connection
        .execute_batch("DROP TRIGGER fail_observation")
        .unwrap();
    observer.finish(observer.begin("read_file", 5), Outcome::Success, 6);
    let recovered = observer.snapshot();
    assert_eq!(recovered["summary"]["successes"], 2);
    assert_eq!(recovered["storage"]["write_failures"], 1);
    assert_eq!(recovered["storage"]["healthy"], true);
    drop(observer);
    let restored = Observer::persistent(&database).unwrap().snapshot();
    assert_eq!(restored["summary"]["successes"], 1);
    assert_eq!(restored["recent_calls"][0]["sequence"], 2);
}

fn tool<'a>(snapshot: &'a Value, name: &str) -> &'a Value {
    snapshot["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap()
}
