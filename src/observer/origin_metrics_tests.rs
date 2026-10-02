use super::*;
use crate::observer::origin::{Attribution, Source, Transport};
use std::time::Duration;

#[test]
fn calls_without_attribution_report_unknown_origin_and_process_run() {
    let observer = Observer::new();
    let token = observer.begin("read_file", 12);
    let active = observer.snapshot();
    assert_eq!(active["server"]["session_id"], 1);
    assert_eq!(active["active_calls"][0]["session_id"], 1);
    assert_eq!(active["active_calls"][0]["capability"], "read");
    assert_eq!(active["active_calls"][0]["origin"], unknown_origin());
    assert!(
        active["active_calls"][0]["elapsed_ms"]
            .as_f64()
            .unwrap()
            .is_finite()
    );
    assert_eq!(active["origins"][0]["active_calls"], 1);
    observer.finish(token, Outcome::Success, 20);
    let completed = observer.snapshot();
    assert_eq!(completed["recent_calls"][0]["origin"], unknown_origin());
    assert_eq!(completed["recent_calls"][0]["session_id"], 1);
    assert_eq!(completed["origins"][0]["successes"], 1);
    assert_eq!(completed["origins"][0]["active_calls"], 0);
}

#[test]
fn tool_capabilities_describe_the_catalog_without_exposing_unknown_names() {
    let observer = Observer::new();
    for (name, expected) in [
        ("read_file", "read"),
        ("write_file", "write"),
        ("private-request-metadata", "unknown"),
    ] {
        observer.finish(observer.begin(name, 0), Outcome::ToolError, 0);
        let snapshot = observer.snapshot();
        assert_eq!(snapshot["recent_calls"][0]["capability"], expected);
        assert!(!snapshot.to_string().contains("private-request-metadata"));
    }
}

fn unknown_origin() -> Value {
    json!({"source": "unknown", "attribution": "unknown", "transport": "unknown"})
}

#[test]
fn origins_are_captured_at_begin_and_all_call_states_contribute_to_breakdown() {
    let observer = Observer::new();
    let codex = origin(Source::Codex);
    let chatgpt = origin(Source::ChatgptWork);
    let first = observer.begin_with_origin("read_file", 1, codex);
    let second = observer.begin_with_origin("write_file", 2, chatgpt);
    let third = observer.begin_with_origin("read_file", 3, codex);
    let active = observer.snapshot();
    assert_eq!(active["active_calls"][0]["origin"], json!(codex));
    assert_eq!(active["active_calls"][1]["origin"], json!(chatgpt));
    assert_eq!(active["active_calls"][0]["request_bytes"], 1);
    observer.finish(first, Outcome::Success, 4);
    observer.finish(second, Outcome::ToolError, 5);
    let snapshot = observer.snapshot();
    let codex_stats = origin_stats(&snapshot, codex);
    assert_eq!(codex_stats["calls"], 2);
    assert_eq!(codex_stats["successes"], 1);
    assert_eq!(codex_stats["errors"], 0);
    assert_eq!(codex_stats["active_calls"], 1);
    let chatgpt_stats = origin_stats(&snapshot, chatgpt);
    assert_eq!(chatgpt_stats["calls"], 1);
    assert_eq!(chatgpt_stats["errors"], 1);
    assert_eq!(chatgpt_stats["active_calls"], 0);
    assert_eq!(snapshot["recent_calls"][1]["origin"], json!(codex));
    observer.finish(third, Outcome::ProtocolError, 6);
    assert_eq!(origin_stats(&observer.snapshot(), codex)["errors"], 1);
}

#[test]
fn elapsed_time_uses_monotonic_clock_even_if_wall_timestamp_is_in_the_future() {
    let observer = Observer::new();
    let token = observer.begin("read_file", 0);
    let now = Instant::now() + Duration::from_millis(500);
    let mut state = observer.state();
    state.active.get_mut(&token.sequence).unwrap().started_at_ms = u64::MAX;
    let elapsed = state.active_snapshot_at(now)[0]["elapsed_ms"]
        .as_f64()
        .unwrap();
    assert!(elapsed >= 500.0 && elapsed.is_finite());
    drop(state);
    observer.finish(token, Outcome::Success, 0);
}

#[test]
fn persisted_origin_groups_and_server_runs_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("observer.sqlite3");
    let codex = origin(Source::CodexCloud);
    let first = Observer::persistent(&path).unwrap();
    first.finish(first.begin("read_file", 0), Outcome::Success, 0);
    first.finish(
        first.begin_with_origin("write_file", 0, codex),
        Outcome::ToolError,
        0,
    );
    let history = first.snapshot();
    assert_eq!(history["server"]["session_id"], 1);
    drop(first);
    let second = Observer::persistent(&path).unwrap();
    let restored = second.snapshot();
    assert_eq!(restored["server"]["session_id"], 2);
    assert_eq!(restored["origins"], history["origins"]);
    assert_eq!(restored["recent_calls"], history["recent_calls"]);
    let active = second.begin_with_origin("read_file", 0, codex);
    assert_eq!(second.snapshot()["active_calls"][0]["session_id"], 2);
    second.finish(active, Outcome::Success, 0);
    assert_eq!(second.snapshot()["recent_calls"][0]["session_id"], 2);
    assert_eq!(origin_stats(&second.snapshot(), codex)["calls"], 2);
}

fn origin(source: Source) -> Origin {
    Origin {
        source,
        attribution: Attribution::ClientReported,
        transport: Transport::Stdio,
    }
}

fn origin_stats(snapshot: &Value, origin: Origin) -> &Value {
    snapshot["origins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["origin"] == json!(origin))
        .unwrap()
}
