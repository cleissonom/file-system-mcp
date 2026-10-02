use super::{MinuteBucket, RecentCall, State, Totals};
use crate::observer::storage::{
    Aggregate, CallRecord, History, MinuteAggregate, Store, ToolAggregate,
};
use serde_json::{Value, json};
use std::io::Write;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::{io, path::Path};

pub(super) struct Persistence {
    store: Mutex<Store>,
    schema_version: u32,
    write_failures: AtomicU64,
    healthy: AtomicBool,
}

impl Persistence {
    pub(super) fn open(path: &Path, now_ms: u64) -> io::Result<(Self, History)> {
        let store = Store::open(path)?;
        let schema_version = store.schema_version();
        let history = store.load(now_ms)?;
        Ok((
            Self {
                store: Mutex::new(store),
                schema_version,
                write_failures: AtomicU64::new(0),
                healthy: AtomicBool::new(true),
            },
            history,
        ))
    }

    pub(super) fn append(&self, record: &CallRecord) {
        let store = self.store.lock().unwrap_or_else(|error| error.into_inner());
        let result = store.append(record);
        self.healthy.store(result.is_ok(), Ordering::Relaxed);
        if result.is_err() && self.write_failures.fetch_add(1, Ordering::Relaxed) == 0 {
            let _ = writeln!(
                io::stderr().lock(),
                "[file-system-mcp] Observer SQLite write failed; live metrics remain available. Check storage health in the dashboard."
            );
        }
    }

    pub(super) fn snapshot(storage: Option<&Self>) -> Value {
        json!({
            "kind": if storage.is_some() { "sqlite" } else { "memory" },
            "schema_version": storage.map_or(0, |value| value.schema_version),
            "write_failures": storage.map_or(0, |value| value.write_failures.load(Ordering::Relaxed)),
            "healthy": storage.is_none_or(|value| value.healthy.load(Ordering::Relaxed))
        })
    }
}

impl State {
    pub(super) fn restore(history: History) -> Self {
        let mut state = Self::new();
        state.last_sequence = history.last_sequence;
        state.totals = history.totals.into();
        state.request_bytes = history.request_bytes;
        state.response_bytes = history.response_bytes;
        state.restore_tools(history.tools);
        for call in history.recent_calls {
            let tool = state.normalize_tool(&call.tool);
            state.recent.push_back(RecentCall::restore(&call, tool));
        }
        state.minutes = history
            .minutes
            .into_iter()
            .map(|minute| (minute.minute_start_ms, minute.into()))
            .collect();
        state
    }

    fn restore_tools(&mut self, tools: Vec<ToolAggregate>) {
        for aggregate in tools {
            let name = self.normalize_tool(&aggregate.tool);
            let stats = self.tools.get_mut(name).expect("Fixed tool catalog");
            stats.totals.add(aggregate.totals);
            stats.last_called_at_ms = Some(
                stats
                    .last_called_at_ms
                    .unwrap_or(0)
                    .max(aggregate.last_called_at_ms),
            );
        }
    }
}

impl From<MinuteAggregate> for MinuteBucket {
    fn from(value: MinuteAggregate) -> Self {
        Self {
            calls: value.calls,
            errors: value.errors,
            duration_ms: value.duration_ms,
        }
    }
}

impl RecentCall {
    pub(super) fn restore(call: &CallRecord, tool: &'static str) -> Self {
        Self {
            sequence: call.sequence,
            started_at_ms: call.started_at_ms,
            tool,
            outcome: call.outcome,
            duration_ms: call.duration_ms,
            request_bytes: call.request_bytes,
            response_bytes: call.response_bytes,
        }
    }
}

impl From<Aggregate> for Totals {
    fn from(value: Aggregate) -> Self {
        Self {
            calls: value.calls,
            successes: value.successes,
            errors: value.errors,
            duration_ms: value.duration_ms,
        }
    }
}

impl Totals {
    fn add(&mut self, value: Aggregate) {
        self.calls = self.calls.saturating_add(value.calls);
        self.successes = self.successes.saturating_add(value.successes);
        self.errors = self.errors.saturating_add(value.errors);
        self.duration_ms += value.duration_ms;
    }
}
