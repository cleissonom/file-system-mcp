import assert from 'node:assert/strict';
import test from 'node:test';
import { activeElapsed, clockCanAdvance, createClock, filterCalls, formatUptime, originView, securityWindow, tickClock } from '../observer-ui/activity.js';

function snapshot(uptime = 0, pid = 42, started = 100_000) {
  return { server: { pid, started_at_ms: started, uptime_ms: uptime, session_id: pid } };
}

test('uptime follows a monotonic clock between two-second snapshots', () => {
  const clock = createClock(snapshot(3_600_500), 100);
  assert.equal(tickClock(clock, 1_100, true).displayed_uptime_ms, 3_601_500);
  assert.equal(tickClock(clock, 50, true).displayed_uptime_ms, 3_600_500);
  assert.equal(formatUptime(3_661_000), '1h 1m 1s');
  assert.equal(formatUptime(90_061_000), '1d 1h 1m 1s');
});

test('paused and disconnected clocks retain their last display', () => {
  const clock = tickClock(createClock(snapshot(100), 10), 1_010, true);
  assert.equal(tickClock(clock, 5_010, false).displayed_uptime_ms, 1_100);
  assert.equal(activeElapsed({ elapsed_ms: 80 }, tickClock(clock, 9_010, false)), 1_080);
});

test('resuming requires a validated fresh snapshot before clocks can advance', () => {
  assert.equal(clockCanAdvance({ paused: false, connected: true, clockReady: false }), false);
  assert.equal(clockCanAdvance({ paused: true, connected: true, clockReady: true }), false);
  assert.equal(clockCanAdvance({ paused: false, connected: false, clockReady: true }), false);
  assert.equal(clockCanAdvance({ paused: false, connected: true, clockReady: true }), true);
  assert.equal(clockCanAdvance({ paused: false, connected: true, clockReady: true, clockGeneration: 1, refreshGeneration: 2 }), false);
});

test('snapshots reanchor the same process without backward display and reset on restart', () => {
  const clock = tickClock(createClock(snapshot(2_000), 100), 1_100, true);
  assert.equal(createClock(snapshot(2_900), 1_100, clock).displayed_uptime_ms, 3_000);
  assert.equal(createClock(snapshot(400), 1_100, clock).displayed_uptime_ms, 3_000);
  assert.equal(createClock(snapshot(100, 43), 1_100, clock).displayed_uptime_ms, 100);
  assert.equal(createClock(snapshot(100, 42, 200_000), 1_100, clock).displayed_uptime_ms, 100);
});

test('active elapsed prefers server elapsed and safely supports older snapshots', () => {
  const clock = tickClock(createClock(snapshot(5_000), 100), 1_100, true);
  assert.equal(activeElapsed({ elapsed_ms: 75, started_at_ms: 999_999 }, clock), 1_075);
  assert.equal(activeElapsed({ started_at_ms: 104_000 }, clock), 2_000);
  assert.equal(activeElapsed({ started_at_ms: 999_999 }, createClock(snapshot(), 0)), 0);
});

test('origins require evidence and never brand unknown or legacy calls', () => {
  assert.equal(originView().source, 'unknown');
  assert.equal(originView({ source: 'chatgpt', attribution: 'unknown' }).source, 'unknown');
  assert.equal(originView({ source: 'forged-secret', attribution: 'client_reported' }).label, 'Origin unavailable');
  assert.equal(originView({ source: 'codex', attribution: 'client_reported', transport: 'stdio' }).evidence, 'Client-reported · unverified');
  assert.equal(originView({ source: 'chatgpt', attribution: 'operator_configured', transport: 'tunnel' }).evidence, 'Operator connection label');
});

const calls = [
  { tool: 'read_file', outcome: 'success', capability: 'read', session_id: 1, origin: { source: 'chatgpt', attribution: 'client_reported', transport: 'tunnel' } },
  { tool: 'write_file', outcome: 'tool_error', capability: 'write', session_id: 2, origin: { source: 'codex', attribution: 'operator_configured', transport: 'stdio' } },
  { tool: 'read_file', outcome: 'protocol_error' },
];

test('source, capability, tool and outcome filters compose without guessing legacy metadata', () => {
  assert.equal(filterCalls(calls, '', '', '', 'chatgpt', 'read').length, 1);
  assert.equal(filterCalls(calls, '', 'read_file', 'error', 'unknown', 'unknown').length, 1);
  assert.equal(filterCalls(calls, 'codex', '', 'error', '', 'write').length, 1);
  assert.equal(filterCalls(calls, '', '', '', 'unknown', 'read').length, 0);
  assert.equal(filterCalls(calls, '', '', 'success').length, 1);
});

test('security counts describe only the supplied retained window', () => {
  assert.deepEqual(securityWindow(calls), { calls: 3, write_calls: 1, error_calls: 2, unknown_origin_calls: 1 });
  assert.deepEqual(securityWindow([]), { calls: 0, write_calls: 0, error_calls: 0, unknown_origin_calls: 0 });
});
