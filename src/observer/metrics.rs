use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::{io, path::Path};

#[path = "metrics_storage.rs"]
mod metrics_storage;

use super::storage::CallRecord;
use metrics_storage::Persistence;

const RECENT_LIMIT: usize = 1_000;
const CHART_MINUTES: u64 = 60;
const MINUTE_MS: u64 = 60_000;

pub struct Observer {
    started_at_ms: u64,
    started: Instant,
    identity: Arc<()>,
    state: Mutex<State>,
    storage: Option<Persistence>,
}

pub struct CallToken {
    sequence: u64,
    started: Instant,
    identity: Arc<()>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    ToolError,
    ProtocolError,
}

#[derive(Clone, Default)]
struct Totals {
    calls: u64,
    successes: u64,
    errors: u64,
    duration_ms: f64,
}

#[derive(Clone)]
struct ToolStats {
    read_only: bool,
    totals: Totals,
    last_called_at_ms: Option<u64>,
}

#[derive(Clone, Serialize)]
struct ActiveCall {
    sequence: u64,
    started_at_ms: u64,
    tool: &'static str,
    #[serde(skip)]
    request_bytes: u64,
}

#[derive(Clone, Serialize)]
struct RecentCall {
    sequence: u64,
    started_at_ms: u64,
    tool: &'static str,
    outcome: Outcome,
    duration_ms: f64,
    request_bytes: u64,
    response_bytes: u64,
}

#[derive(Clone, Default)]
struct MinuteBucket {
    calls: u64,
    errors: u64,
    duration_ms: f64,
}

#[derive(Clone)]
struct State {
    last_sequence: u64,
    totals: Totals,
    request_bytes: u64,
    response_bytes: u64,
    tools: BTreeMap<&'static str, ToolStats>,
    active: BTreeMap<u64, ActiveCall>,
    recent: VecDeque<RecentCall>,
    minutes: BTreeMap<u64, MinuteBucket>,
}

impl Observer {
    pub fn persistent(path: &Path) -> io::Result<Self> {
        let (storage, history) = Persistence::open(path, unix_ms())?;
        Ok(Self::from_state(State::restore(history), Some(storage)))
    }

    #[cfg(test)]
    pub fn new() -> Self {
        Self::from_state(State::new(), None)
    }

    fn from_state(state: State, storage: Option<Persistence>) -> Self {
        Self {
            started_at_ms: unix_ms(),
            started: Instant::now(),
            identity: Arc::new(()),
            state: Mutex::new(state),
            storage,
        }
    }

    pub fn begin(&self, name: &str, request_bytes: u64) -> CallToken {
        let started = Instant::now();
        let timestamp = unix_ms();
        let mut state = self.state();
        let tool = state.normalize_tool(name);
        state.totals.calls = state.totals.calls.saturating_add(1);
        state.last_sequence = state.last_sequence.saturating_add(1);
        let sequence = state.last_sequence;
        state.request_bytes = state.request_bytes.saturating_add(request_bytes);
        let stats = state.tools.get_mut(tool).expect("Fixed tool catalog");
        stats.totals.calls = stats.totals.calls.saturating_add(1);
        stats.last_called_at_ms = Some(timestamp);
        state.active.insert(
            sequence,
            ActiveCall {
                sequence,
                started_at_ms: timestamp,
                tool,
                request_bytes,
            },
        );
        CallToken {
            sequence,
            started,
            identity: Arc::clone(&self.identity),
        }
    }

    pub fn finish(&self, token: CallToken, outcome: Outcome, response_bytes: u64) {
        if !Arc::ptr_eq(&self.identity, &token.identity) {
            return;
        }
        let duration_ms = token.started.elapsed().as_secs_f64() * 1_000.0;
        let record = self.state().complete(
            token.sequence,
            outcome,
            duration_ms,
            response_bytes,
            unix_ms(),
        );
        if let (Some(storage), Some(record)) = (&self.storage, record) {
            storage.append(&record);
        }
    }

    pub fn snapshot(&self) -> Value {
        let state = self.state().clone();
        let mut snapshot = state.snapshot(
            self.started_at_ms,
            self.started.elapsed().as_millis(),
            unix_ms(),
        );
        snapshot["storage"] = Persistence::snapshot(self.storage.as_ref());
        snapshot
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl State {
    fn new() -> Self {
        let mut tools: BTreeMap<_, _> = crate::tools::get_available_tools()
            .into_iter()
            .map(|tool| (tool.name, ToolStats::new(tool.annotations.read_only_hint)))
            .collect();
        tools.insert("unknown_tool", ToolStats::new(false));
        Self {
            last_sequence: 0,
            totals: Totals::default(),
            request_bytes: 0,
            response_bytes: 0,
            tools,
            active: BTreeMap::new(),
            recent: VecDeque::new(),
            minutes: BTreeMap::new(),
        }
    }

    fn normalize_tool(&self, name: &str) -> &'static str {
        self.tools
            .get_key_value(name)
            .map(|(name, _)| *name)
            .unwrap_or("unknown_tool")
    }

    fn complete(
        &mut self,
        sequence: u64,
        outcome: Outcome,
        duration_ms: f64,
        response_bytes: u64,
        now: u64,
    ) -> Option<CallRecord> {
        let active = self.active.remove(&sequence)?;
        let record = CallRecord {
            sequence,
            started_at_ms: active.started_at_ms,
            completed_at_ms: now,
            tool: active.tool.to_owned(),
            outcome,
            duration_ms,
            request_bytes: active.request_bytes,
            response_bytes,
        };
        self.record_completed(&record, active.tool);
        Some(record)
    }

    fn record_completed(&mut self, record: &CallRecord, tool: &'static str) {
        self.totals.complete(record.outcome, record.duration_ms);
        self.tools
            .get_mut(tool)
            .expect("Fixed tool catalog")
            .totals
            .complete(record.outcome, record.duration_ms);
        self.response_bytes = self.response_bytes.saturating_add(record.response_bytes);
        self.record_minute(record.outcome, record.duration_ms, record.completed_at_ms);
        if self.recent.len() == RECENT_LIMIT {
            self.recent.pop_front();
        }
        self.recent.push_back(RecentCall::restore(record, tool));
    }

    fn record_minute(&mut self, outcome: Outcome, duration_ms: f64, now: u64) {
        let minute = now / MINUTE_MS * MINUTE_MS;
        let bucket = self.minutes.entry(minute).or_default();
        bucket.calls = bucket.calls.saturating_add(1);
        bucket.errors = bucket
            .errors
            .saturating_add(u64::from(!matches!(outcome, Outcome::Success)));
        bucket.duration_ms += duration_ms;
        let oldest = minute.saturating_sub((CHART_MINUTES - 1) * MINUTE_MS);
        self.minutes
            .retain(|timestamp, _| *timestamp >= oldest && *timestamp <= minute);
    }

    fn snapshot(&self, started_at_ms: u64, uptime_ms: u128, now: u64) -> Value {
        let completed = self.totals.successes + self.totals.errors;
        json!({
            "server": { "name": "file-system-mcp", "started_at_ms": started_at_ms, "uptime_ms": uptime_ms, "pid": std::process::id() },
            "summary": {
                "total_calls": self.totals.calls,
                "successes": self.totals.successes,
                "errors": self.totals.errors,
                "active_calls": self.active.len(),
                "success_rate": (completed > 0).then(|| self.totals.successes as f64 / completed as f64),
                "average_duration_ms": self.totals.average(),
                "recent_p95_duration_ms": self.recent_p95(),
                "request_bytes": self.request_bytes,
                "response_bytes": self.response_bytes
            },
            "retention": { "recent_call_limit": RECENT_LIMIT, "chart_minutes": CHART_MINUTES, "recent_call_count": self.recent.len() },
            "tools": self.tool_snapshot(),
            "recent_calls": self.recent.iter().rev().collect::<Vec<_>>(),
            "timeline": self.timeline(now),
            "active_calls": self.active.values().collect::<Vec<_>>()
        })
    }

    fn tool_snapshot(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|(name, stats)| {
                json!({
                    "name": name,
                    "read_only": stats.read_only,
                    "calls": stats.totals.calls,
                    "successes": stats.totals.successes,
                    "errors": stats.totals.errors,
                    "average_duration_ms": stats.totals.average(),
                    "last_called_at_ms": stats.last_called_at_ms
                })
            })
            .collect()
    }

    fn recent_p95(&self) -> f64 {
        let mut durations: Vec<_> = self.recent.iter().map(|call| call.duration_ms).collect();
        durations.sort_by(f64::total_cmp);
        let rank = (durations.len() * 95).div_ceil(100);
        rank.checked_sub(1)
            .map(|index| durations[index])
            .unwrap_or(0.0)
    }

    fn timeline(&self, now: u64) -> Vec<Value> {
        let current = now / MINUTE_MS * MINUTE_MS;
        (0..CHART_MINUTES).rev().map(|offset| {
            let timestamp = current.saturating_sub(offset * MINUTE_MS);
            let bucket = self.minutes.get(&timestamp).cloned().unwrap_or_default();
            let average = if bucket.calls == 0 { 0.0 } else { bucket.duration_ms / bucket.calls as f64 };
            json!({ "minute_start_ms": timestamp, "calls": bucket.calls, "errors": bucket.errors, "average_duration_ms": average })
        }).collect()
    }
}

impl ToolStats {
    fn new(read_only: bool) -> Self {
        Self {
            read_only,
            totals: Totals::default(),
            last_called_at_ms: None,
        }
    }
}

impl Totals {
    fn complete(&mut self, outcome: Outcome, duration_ms: f64) {
        match outcome {
            Outcome::Success => self.successes = self.successes.saturating_add(1),
            _ => self.errors = self.errors.saturating_add(1),
        }
        self.duration_ms += duration_ms;
    }

    fn average(&self) -> f64 {
        let completed = self.successes + self.errors;
        if completed == 0 {
            0.0
        } else {
            self.duration_ms / completed as f64
        }
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

#[cfg(test)]
#[path = "metrics_tests.rs"]
mod metrics_tests;

#[cfg(test)]
#[path = "persistence_tests.rs"]
mod persistence_tests;
