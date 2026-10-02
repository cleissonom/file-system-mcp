const sources = { chatgpt: 'ChatGPT', chatgpt_work: 'ChatGPT Work', codex: 'Codex', codex_cloud: 'Codex Cloud', openai_dot: 'OpenAI Dot' };
const outcomes = { success: 'Success', tool_error: 'Tool error', protocol_error: 'Protocol error' };
const element = id => document.getElementById(id);
const count = value => value.toLocaleString();

export function createClock(snapshot, now, previous = null) {
  const server = snapshot.server;
  const key = `${server.pid}:${server.started_at_ms}:${server.session_id ?? ''}`;
  const base = Math.max(server.uptime_ms, previous?.key === key ? previous.displayed_uptime_ms : 0);
  return { key, anchored_at_ms: now, base_uptime_ms: base, displayed_uptime_ms: base,
    snapshot_uptime_ms: server.uptime_ms, snapshot_server_ms: server.started_at_ms + server.uptime_ms };
}

export function tickClock(clock, now, advancing) {
  if (!clock || !advancing) return clock;
  const elapsed = Math.max(0, now - clock.anchored_at_ms);
  return { ...clock, displayed_uptime_ms: Math.max(clock.displayed_uptime_ms, clock.base_uptime_ms + elapsed) };
}

export function clockCanAdvance(state) {
  return state.clockReady && state.connected && !state.paused && state.clockGeneration === state.refreshGeneration;
}

export function activeElapsed(call, clock) {
  const base = Number.isFinite(call.elapsed_ms) ? Math.max(0, call.elapsed_ms) : Math.max(0, clock.snapshot_server_ms - call.started_at_ms);
  return base + Math.max(0, clock.displayed_uptime_ms - clock.snapshot_uptime_ms);
}

export function formatUptime(milliseconds) {
  const seconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  if (seconds < 60) return `${seconds}s`;
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  if (hours < 24) return `${hours}h ${minutes % 60}m ${seconds % 60}s`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h ${minutes % 60}m ${seconds % 60}s`;
}

export function formatDuration(milliseconds) {
  if (milliseconds >= 1000) return `${(milliseconds / 1000).toFixed(2)} s`;
  return `${milliseconds < 10 ? milliseconds.toFixed(1) : Math.round(milliseconds)} ms`;
}

export function formatBytes(bytes) {
  if (!Number.isFinite(bytes)) return '—';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function originView(origin) {
  const attribution = ['client_reported', 'operator_configured'].includes(origin?.attribution) ? origin.attribution : 'unknown';
  const source = attribution !== 'unknown' && Object.hasOwn(sources, origin?.source) ? origin.source : 'unknown';
  const transport = ['stdio', 'tunnel'].includes(origin?.transport) ? origin.transport : 'unknown';
  const evidence = { client_reported: 'Client-reported · unverified', operator_configured: 'Operator connection label', unknown: 'No origin evidence' }[attribution];
  return { source, label: sources[source] || 'Origin unavailable', evidence, transport };
}

export function capability(call) {
  return ['read', 'write'].includes(call.capability) ? call.capability : 'unknown';
}

export function sessionLabel(session) {
  return Number.isSafeInteger(session) && session > 0 ? `Run #${session}` : 'Run unavailable';
}

export function filterCalls(calls, query, tool, status, source = '', access = '') {
  const normalized = query.trim().toLowerCase();
  return calls.filter(call => {
    const origin = originView(call.origin);
    const searchable = `${call.tool} ${outcomes[call.outcome] || call.outcome} ${origin.label} ${sessionLabel(call.session_id)}`;
    return (!tool || call.tool === tool) && (!source || origin.source === source)
      && (!access || capability(call) === access)
      && (!status || (status === 'error' ? call.outcome !== 'success' : call.outcome === status))
      && (!normalized || searchable.toLowerCase().includes(normalized));
  });
}

export function securityWindow(calls) {
  return { calls: calls.length, write_calls: calls.filter(call => capability(call) === 'write').length,
    error_calls: calls.filter(call => call.outcome !== 'success').length,
    unknown_origin_calls: calls.filter(call => originView(call.origin).source === 'unknown').length };
}

function node(tag, className = '', content) {
  const result = document.createElement(tag);
  if (className) result.className = className;
  if (content !== undefined) result.textContent = content;
  return result;
}

function emptyRow(columns, title, description) {
  const row = node('tr');
  const cell = node('td', 'table-empty');
  cell.colSpan = columns;
  cell.append(node('strong', 'empty-title', title), node('span', '', description));
  row.append(cell);
  return row;
}

function capabilityCell(call) {
  const cell = node('td');
  const access = capability(call);
  cell.append(node('span', `access-label${access === 'write' ? ' write' : ''}`, access.toUpperCase()));
  cell.title = 'Tool capability; this does not indicate whether a call changed files.';
  return cell;
}

function originCell(origin, showTransport = true) {
  const view = originView(origin);
  const cell = node('td', 'origin-cell');
  const transport = { stdio: 'Stdio', tunnel: 'Tunnel', unknown: 'Transport unknown' }[view.transport];
  const evidence = `${view.evidence}${showTransport ? ` · ${transport}` : ''}`;
  cell.append(node('span', 'origin-name', view.label), node('span', 'origin-evidence', evidence));
  return cell;
}

function toolCell(call) {
  const cell = node('td');
  cell.append(node('span', 'call-tool', call.tool), node('span', 'call-sequence', `#${call.sequence}`));
  return cell;
}

function clockLabel(milliseconds) {
  return new Date(milliseconds).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function callRow(call) {
  const row = node('tr');
  const outcome = node('td');
  outcome.append(node('span', `call-outcome${call.outcome === 'success' ? '' : ' failure'}`, outcomes[call.outcome] || 'Unknown outcome'));
  const time = node('td', 'call-time', clockLabel(call.started_at_ms));
  time.title = new Date(call.started_at_ms).toLocaleString();
  row.append(outcome, toolCell(call), capabilityCell(call), originCell(call.origin), node('td', 'session-cell', sessionLabel(call.session_id)), time);
  row.append(node('td', 'numeric', formatDuration(call.duration_ms)), node('td', 'numeric muted-value', formatBytes(call.request_bytes)), node('td', 'numeric muted-value', formatBytes(call.response_bytes)));
  return row;
}

export function renderRecentCalls(calls, filtered) {
  if (calls.length) element('call-rows').replaceChildren(...calls.map(callRow));
  else element('call-rows').replaceChildren(emptyRow(9, filtered ? 'No matching calls' : 'Quiet for now', filtered ? 'Try another search, source, capability or outcome.' : 'Completed MCP requests will appear here with their available origin evidence.'));
}

function activeRow(call, clock) {
  const row = node('tr');
  const tool = toolCell(call);
  tool.className = 'executing-tool';
  const spinner = node('span', 'request-spinner');
  spinner.setAttribute('aria-hidden', 'true');
  tool.prepend(spinner);
  row.append(tool, capabilityCell(call), originCell(call.origin), node('td', 'session-cell', sessionLabel(call.session_id)));
  row.append(node('td', 'numeric active-elapsed', formatDuration(activeElapsed(call, clock))), node('td', 'numeric muted-value', formatBytes(call.request_bytes)));
  return row;
}

export function renderInFlight(snapshot, clock, fresh) {
  const calls = snapshot.active_calls;
  element('inflight').classList.toggle('is-frozen', !fresh);
  element('inflight-count').textContent = `${count(calls.length)} ACTIVE`;
  element('inflight-oldest').textContent = calls.length ? `Oldest · ${formatDuration(Math.max(...calls.map(call => activeElapsed(call, clock))))}` : 'No requests executing';
  element('inflight-status').textContent = fresh ? 'Live · elapsed updates every second' : `Frozen snapshot · ${clockLabel(clock.snapshot_server_ms)} · resume or reconnect to verify`;
  element('inflight-rows').replaceChildren(...(calls.length ? calls.map(call => activeRow(call, clock)) : [emptyRow(6, 'No requests in flight', 'Requests appear here while the server executes them.')]));
}

function originRow(bucket) {
  const row = node('tr');
  const view = originView(bucket.origin);
  row.append(originCell(bucket.origin, false), node('td', 'origin-transport', { stdio: 'Stdio', tunnel: 'Tunnel', unknown: 'Unknown' }[view.transport]));
  row.append(node('td', 'numeric', count(bucket.calls)), node('td', `numeric${bucket.errors ? ' error-number' : ''}`, count(bucket.errors)), node('td', 'numeric', count(bucket.active_calls)));
  return row;
}

export function renderOrigins(snapshot) {
  const origins = snapshot.origins || [];
  const sorted = [...origins].sort((a, b) => b.calls - a.calls);
  element('origin-rows').replaceChildren(...(sorted.length ? sorted.map(originRow) : [emptyRow(5, 'Origin metadata unavailable', 'Older calls remain unattributed. The observer never infers caller identity.')]));
  const scope = snapshot.storage?.kind === 'sqlite' ? 'Across recorded history' : 'Current server run';
  element('origin-scope').textContent = scope;
}

export function renderSecurityWindow(calls) {
  const stats = securityWindow(calls);
  element('security-window').textContent = `${count(stats.calls)} retained completed calls · unfiltered`;
  element('security-writes').textContent = count(stats.write_calls);
  element('security-errors').textContent = count(stats.error_calls);
  element('security-unknown').textContent = count(stats.unknown_origin_calls);
}
