use super::*;

#[test]
fn latency_uses_completed_calls_and_the_recent_window_for_percentiles() {
    let observer = Observer::new();
    for duration in 1..=100 {
        let token = observer.begin("read_file", 0);
        let _ = observer.state().complete(
            token.sequence,
            Outcome::Success,
            duration as f64,
            0,
            unix_ms(),
        );
    }
    let active = observer.begin("read_file", 0);
    let snapshot = observer.snapshot();
    assert_eq!(snapshot["summary"]["average_duration_ms"], 50.5);
    assert_eq!(snapshot["summary"]["recent_p95_duration_ms"], 95.0);
    assert_eq!(snapshot["summary"]["success_rate"], 1.0);
    let tool = snapshot["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "read_file")
        .unwrap();
    assert_eq!(tool["calls"], 101);
    assert_eq!(tool["average_duration_ms"], 50.5);
    observer.finish(active, Outcome::Success, 0);
}

#[test]
fn minute_chart_expires_old_calls_and_includes_empty_minutes() {
    let observer = Observer::new();
    let base = 120 * MINUTE_MS;
    for minute in 0..=60 {
        let token = observer.begin("file_info", 0);
        let _ = observer.state().complete(
            token.sequence,
            Outcome::ToolError,
            12.0,
            0,
            base + minute * MINUTE_MS,
        );
    }
    let state = observer.state().clone();
    let timeline = state.timeline(base + 61 * MINUTE_MS);
    assert_eq!(timeline.len(), 60);
    assert_eq!(
        timeline
            .iter()
            .map(|bucket| bucket["calls"].as_u64().unwrap())
            .sum::<u64>(),
        59
    );
    assert_eq!(timeline[59]["calls"], 0);
    assert_eq!(timeline[0]["errors"], 1);
    assert_eq!(timeline[0]["average_duration_ms"], 12.0);
}

#[test]
fn concurrent_calls_keep_session_counts_and_active_calls_consistent() {
    let observer = std::sync::Arc::new(Observer::new());
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let observer = std::sync::Arc::clone(&observer);
            std::thread::spawn(move || {
                for _ in 0..50 {
                    observer.finish(observer.begin("read_file", 2), Outcome::Success, 3);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let snapshot = observer.snapshot();
    assert_eq!(snapshot["summary"]["total_calls"], 400);
    assert_eq!(snapshot["summary"]["successes"], 400);
    assert_eq!(snapshot["summary"]["active_calls"], 0);
    assert_eq!(snapshot["summary"]["request_bytes"], 800);
    assert_eq!(snapshot["summary"]["response_bytes"], 1_200);
}

#[test]
fn active_calls_are_visible_without_exposing_inputs() {
    let observer = Observer::new();
    let token = observer.begin("read_file", 123);
    let snapshot = observer.snapshot();
    assert_eq!(snapshot["summary"]["total_calls"], 1);
    assert_eq!(snapshot["summary"]["active_calls"], 1);
    assert_eq!(snapshot["summary"]["request_bytes"], 123);
    assert!(snapshot["summary"]["success_rate"].is_null());
    assert_eq!(snapshot["active_calls"][0]["tool"], "read_file");
    assert_eq!(snapshot["retention"]["recent_call_count"], 0);
    observer.finish(token, Outcome::Success, 42);
}

#[test]
fn outcomes_and_bytes_are_counted_for_completed_calls() {
    let observer = Observer::new();
    for outcome in [Outcome::Success, Outcome::ToolError, Outcome::ProtocolError] {
        observer.finish(observer.begin("write_file", 10), outcome, 20);
    }
    let snapshot = observer.snapshot();
    assert_eq!(snapshot["summary"]["total_calls"], 3);
    assert_eq!(snapshot["summary"]["successes"], 1);
    assert_eq!(snapshot["summary"]["errors"], 2);
    assert_eq!(snapshot["summary"]["active_calls"], 0);
    assert_eq!(snapshot["summary"]["request_bytes"], 30);
    assert_eq!(snapshot["summary"]["response_bytes"], 60);
    assert_eq!(
        snapshot["summary"]["success_rate"].as_f64(),
        Some(1.0 / 3.0)
    );
    assert_eq!(snapshot["recent_calls"][0]["outcome"], "protocol_error");
    assert_eq!(snapshot["recent_calls"][1]["outcome"], "tool_error");
    assert_eq!(snapshot["recent_calls"][2]["outcome"], "success");
}

#[test]
fn unknown_tool_names_cannot_leak_paths_or_secrets() {
    let observer = Observer::new();
    let private = "sk-private-/Users/private/.env";
    observer.finish(observer.begin(private, 100), Outcome::ProtocolError, 50);
    let snapshot = observer.snapshot();
    assert!(!snapshot.to_string().contains(private));
    assert!(!snapshot.to_string().contains("/Users"));
    assert_eq!(snapshot["recent_calls"][0]["tool"], "unknown_tool");
    let catalog = snapshot["tools"].as_array().unwrap();
    assert_eq!(catalog.len(), crate::tools::get_available_tools().len() + 1);
    let read = catalog
        .iter()
        .find(|tool| tool["name"] == "read_file")
        .unwrap();
    assert_eq!(read["read_only"], true);
}

#[test]
fn recent_calls_are_bounded_while_session_totals_keep_growing() {
    let observer = Observer::new();
    for _ in 0..1_205 {
        observer.finish(observer.begin("file_info", 1), Outcome::Success, 2);
    }
    let snapshot = observer.snapshot();
    assert_eq!(snapshot["summary"]["total_calls"], 1_205);
    assert_eq!(snapshot["retention"]["recent_call_count"], 1_000);
    assert_eq!(snapshot["recent_calls"][0]["sequence"], 1_205);
    assert_eq!(snapshot["recent_calls"][999]["sequence"], 206);
    let timeline = snapshot["timeline"].as_array().unwrap();
    assert_eq!(timeline.len(), 60);
    for pair in timeline.windows(2) {
        assert_eq!(
            pair[1]["minute_start_ms"].as_u64().unwrap()
                - pair[0]["minute_start_ms"].as_u64().unwrap(),
            60_000
        );
    }
    assert_eq!(
        timeline
            .iter()
            .map(|bucket| bucket["calls"].as_u64().unwrap())
            .sum::<u64>(),
        1_205
    );
}
