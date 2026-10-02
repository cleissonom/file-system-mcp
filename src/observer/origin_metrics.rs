use super::{ActiveCall, Origin, State};
use serde_json::{Value, json};
use std::time::Instant;

impl State {
    pub(super) fn begin_call(
        &mut self,
        name: &str,
        request_bytes: u64,
        origin: Origin,
        started: Instant,
        timestamp: u64,
    ) -> u64 {
        let tool = self.normalize_tool(name);
        self.record_begin(tool, origin, request_bytes, timestamp);
        self.active.insert(
            self.last_sequence,
            ActiveCall {
                sequence: self.last_sequence,
                started_at_ms: timestamp,
                tool,
                origin,
                session_id: self.session_id,
                capability: self.capability(tool),
                started,
                request_bytes,
            },
        );
        self.last_sequence
    }

    fn record_begin(
        &mut self,
        tool: &'static str,
        origin: Origin,
        request_bytes: u64,
        timestamp: u64,
    ) {
        self.totals.calls = self.totals.calls.saturating_add(1);
        self.last_sequence = self.last_sequence.saturating_add(1);
        self.request_bytes = self.request_bytes.saturating_add(request_bytes);
        let stats = self.tools.get_mut(tool).expect("Fixed tool catalog");
        stats.totals.calls = stats.totals.calls.saturating_add(1);
        stats.last_called_at_ms = Some(timestamp);
        let totals = self.origins.entry(origin).or_default();
        totals.calls = totals.calls.saturating_add(1);
    }

    pub(super) fn capability(&self, tool: &str) -> &'static str {
        match self.tools.get(tool) {
            _ if tool == "unknown_tool" => "unknown",
            Some(stats) if stats.read_only => "read",
            Some(_) => "write",
            None => "unknown",
        }
    }

    pub(super) fn active_snapshot_at(&self, now: Instant) -> Vec<Value> {
        self.active
            .values()
            .map(|active| {
                let mut snapshot = json!(active);
                snapshot["elapsed_ms"] =
                    json!(now.saturating_duration_since(active.started).as_secs_f64() * 1_000.0);
                snapshot
            })
            .collect()
    }

    pub(super) fn origin_snapshot(&self) -> Vec<Value> {
        self.origins.iter().map(|(origin, totals)| {
            let active = self.active.values().filter(|call| call.origin == *origin).count();
            json!({ "origin": origin, "calls": totals.calls, "successes": totals.successes, "errors": totals.errors, "active_calls": active })
        }).collect()
    }
}
