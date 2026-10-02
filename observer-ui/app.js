import { renderVolumeChart, renderOutcomes } from './charts.js';
import { clockCanAdvance, createClock, filterCalls, formatBytes, formatDuration, formatUptime, renderInFlight, renderOrigins, renderRecentCalls, renderSecurityWindow, sessionLabel, tickClock } from './activity.js';
export { filterCalls, formatBytes, formatDuration, formatUptime } from './activity.js';

const state = { snapshot: null, paused: false, loading: false, connected: false, timer: null,
  clock: null, clockReady: false, clockGeneration: -1, refreshGeneration: 0 };
const element = id => document.getElementById(id);
const setText = (id, value) => { element(id).textContent = value; };

function number(value) { return value.toLocaleString(); }
function clock(milliseconds) { return new Date(milliseconds).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }); }
export function formatSuccessRate(rate) { return rate === null ? '—' : `${(rate * 100).toFixed(1)}%`; }

export function storageLabels(storage, retention) {
  const persisted = storage?.kind === 'sqlite';
  const scope = persisted ? 'across recorded history' : 'this session';
  return {
    totalScope: persisted ? 'Across recorded history' : 'This server session',
    metricsScope: persisted ? 'Historical usage and current activity' : 'Session metrics',
    healthScope: persisted ? 'HISTORY HEALTH' : 'SESSION HEALTH',
    toolCaption: `Tool usage and performance ${scope}. Select a tool to filter the call stream.`,
    outcomeScope: scope,
    storageInfo: persisted ? `SQLite · schema ${number(storage.schema_version)} · History persists` : 'Memory only · Resets on restart',
    callNote: persisted ? `${number(retention.recent_call_count)} latest completed calls from saved history. Totals span all history; saved calls survive restarts.` : `${number(retention.recent_call_count)} completed calls retained in memory. Metrics reset when the server restarts.`,
    exportDescription: `Export aggregate metrics and up to ${number(retention.recent_call_limit)} recent calls${persisted ? '; not a full history archive' : ''}.`,
    warning: storageWarning(storage),
  };
}

function storageWarning(storage) {
  const failures = storage?.write_failures || 0;
  if (failures) return `${number(failures)} ${failures === 1 ? 'call' : 'calls'} failed to save during the current server process. Unsaved calls may disappear after a restart.`;
  return storage?.healthy === false ? 'History storage is unhealthy. Some calls may not be saved.' : '';
}

function node(tag, className, content) {
  const value = document.createElement(tag);
  if (className) value.className = className;
  if (content !== undefined) value.textContent = content;
  return value;
}

function connectionState() {
  const status = state.paused ? 'paused' : state.connected ? 'connected' : state.snapshot ? 'disconnected' : 'loading';
  element('connection-badge').className = `connection-badge ${status}`;
  setText('connection-label', { paused: 'Paused', connected: 'Live', disconnected: 'Disconnected', loading: 'Connecting' }[status]);
  setText('pause-label', state.paused ? 'Resume' : 'Pause');
  element('pause-button').setAttribute('aria-label', `${state.paused ? 'Resume' : 'Pause'} automatic refresh`);
  element('pause-button').setAttribute('title', `${state.paused ? 'Resume' : 'Pause'} automatic refresh`);
  element('pause-icon').firstElementChild.setAttribute('d', state.paused ? 'm7 4 8 6-8 6V4Z' : 'M7 5v10M13 5v10');
  setText('refresh-status', state.paused ? 'Automatic refresh paused' : 'Updates every 2 seconds');
  renderClock();
}

function notice(message, isError = false) {
  const target = element('connection-notice');
  target.hidden = !message;
  target.className = `connection-notice${isError ? ' error' : ''}`;
  target.textContent = message;
}

function renderMetrics(snapshot) {
  const { summary, server, retention } = snapshot;
  setText('server-name', server.name);
  setText('server-pid', server.pid);
  setText('process-session', sessionLabel(server.session_id));
  setText('health-session', sessionLabel(server.session_id));
  setText('total-calls', number(summary.total_calls));
  setText('success-rate', formatSuccessRate(summary.success_rate));
  setText('success-foot', `${number(summary.successes)} successful · ${number(summary.errors)} failed`);
  setText('average-duration', summary.successes + summary.errors ? formatDuration(summary.average_duration_ms) : '—');
  setText('p95-duration', retention.recent_call_count ? formatDuration(summary.recent_p95_duration_ms) : '—');
  setText('p95-foot', `${number(retention.recent_call_count)} retained calls`);
  setText('active-count', number(summary.active_calls));
  setText('active-foot', summary.active_calls ? 'Currently executing' : 'Ready for the next call');
}

function renderTraffic(snapshot) {
  const { summary } = snapshot;
  setText('request-bytes', formatBytes(summary.request_bytes));
  setText('response-bytes', formatBytes(summary.response_bytes));
  setText('outcome-total', number(summary.successes + summary.errors));
  setText('outcome-successes', number(summary.successes));
  setText('outcome-failures', number(summary.errors));
  renderOutcomes(summary);
  renderVolumeChart(element('volume-chart'), element('chart-detail'), snapshot.timeline);
}

function toolButton(tool) {
  const button = node('button', 'tool-name', tool.name);
  button.type = 'button';
  button.dataset.tool = tool.name;
  button.setAttribute('aria-pressed', String(element('tool-filter').value === tool.name));
  button.setAttribute('title', `Show recent calls for ${tool.name}`);
  button.addEventListener('click', () => selectTool(tool.name));
  return button;
}

function toolRow(tool) {
  const row = node('tr');
  const name = node('td');
  name.append(toolButton(tool));
  const access = node('td');
  const accessLabel = tool.name === 'unknown_tool' ? 'UNKNOWN' : tool.read_only ? 'READ' : 'WRITE';
  access.append(node('span', `access-label${tool.read_only ? '' : ' write'}`, accessLabel));
  row.append(name, access, node('td', 'numeric', number(tool.calls)), node('td', 'numeric', number(tool.successes)));
  row.append(node('td', `numeric${tool.errors ? ' error-number' : ' muted-value'}`, number(tool.errors)));
  row.append(node('td', 'numeric', tool.successes + tool.errors ? formatDuration(tool.average_duration_ms) : '—'));
  row.append(node('td', 'last-called', tool.last_called_at_ms ? clock(tool.last_called_at_ms) : 'Not yet called'));
  return row;
}

function renderToolFilters(tools) {
  const selected = element('tool-filter').value;
  const ordered = [...tools].sort((a, b) => a.name.localeCompare(b.name));
  const values = [...element('tool-filter').options].map(option => option.value);
  if (values.join('\n') === ['', ...ordered.map(tool => tool.name)].join('\n')) return;
  const options = [node('option', '', 'All tools')];
  options[0].value = '';
  for (const tool of ordered) {
    const option = node('option', '', tool.name);
    option.value = tool.name;
    options.push(option);
  }
  element('tool-filter').replaceChildren(...options);
  element('tool-filter').value = options.some(option => option.value === selected) ? selected : '';
}

function renderTools(tools) {
  const activeTool = document.activeElement?.dataset.tool;
  const sorted = [...tools].sort((a, b) => b.calls - a.calls || a.name.localeCompare(b.name));
  renderToolFilters(sorted);
  setText('tool-count', `${number(tools.filter(tool => tool.name !== 'unknown_tool').length)} TOOLS`);
  element('tool-rows').replaceChildren(...sorted.map(toolRow));
  if (activeTool) [...element('tool-rows').querySelectorAll('button')].find(button => button.dataset.tool === activeTool)?.focus({ preventScroll: true });
}

function renderCalls() {
  if (!state.snapshot) return;
  const query = element('call-search').value;
  const tool = element('tool-filter').value;
  const status = element('status-filter').value;
  const source = element('source-filter').value;
  const access = element('capability-filter').value;
  const calls = filterCalls(state.snapshot.recent_calls, query, tool, status, source, access);
  const filtered = Boolean(query || tool || status || source || access);
  element('clear-filters').hidden = !filtered;
  setText('filtered-count', `${number(calls.length)} calls`);
  renderRecentCalls(calls, filtered);
  for (const button of element('tool-rows').querySelectorAll('button')) button.setAttribute('aria-pressed', String(button.dataset.tool === tool));
}

function renderSnapshot(snapshot) {
  renderMetrics(snapshot);
  renderTraffic(snapshot);
  renderTools(snapshot.tools);
  renderCalls();
  renderStorage(snapshot);
  renderOrigins(snapshot);
  renderSecurityWindow(snapshot.recent_calls);
  renderClock();
  setText('nav-count', number(snapshot.summary.total_calls));
  setText('call-retention', `UP TO ${number(snapshot.retention.recent_call_limit)} RECENT CALLS`);
  setText('last-updated', `Last snapshot · ${clock(Date.now())}`);
}

function clockAdvancing() { return clockCanAdvance(state); }

function renderClock() {
  if (!state.snapshot || !state.clock) return;
  state.clock = tickClock(state.clock, performance.now(), clockAdvancing());
  setText('server-uptime', formatUptime(state.clock.displayed_uptime_ms));
  renderInFlight(state.snapshot, state.clock, clockAdvancing());
}

function renderStorage(snapshot) {
  const labels = storageLabels(snapshot.storage, snapshot.retention);
  setText('storage-info', labels.storageInfo);
  setText('total-scope', labels.totalScope);
  setText('health-scope', labels.healthScope);
  setText('tool-caption', labels.toolCaption);
  setText('call-note', labels.callNote);
  setText('export-description', labels.exportDescription);
  element('metrics-group').setAttribute('aria-label', labels.metricsScope);
  element('export-button').title = labels.exportDescription;
  setText('outcome-title', `${number(snapshot.summary.successes)} successful calls and ${number(snapshot.summary.errors)} failed calls ${labels.outcomeScope}`);
  if (element('storage-notice').textContent !== labels.warning) setText('storage-notice', labels.warning);
  element('storage-notice').hidden = !labels.warning;
}

function validSnapshot(snapshot) {
  return snapshot && snapshot.server && snapshot.summary && snapshot.retention
    && ['tools', 'recent_calls', 'timeline', 'active_calls'].every(key => Array.isArray(snapshot[key]));
}

async function refresh() {
  if (state.loading) return;
  const generation = state.refreshGeneration;
  state.loading = true;
  element('refresh-button').disabled = true;
  try {
    const response = await fetch('/api/snapshot', { cache: 'no-store', signal: AbortSignal.timeout(5000) });
    if (!response.ok) throw new Error('Snapshot unavailable');
    const snapshot = await response.json();
    if (!validSnapshot(snapshot)) throw new Error('Invalid snapshot');
    const now = performance.now();
    state.clock = createClock(snapshot, now, tickClock(state.clock, now, clockAdvancing()));
    state.clockReady = !state.paused;
    state.clockGeneration = generation;
    state.snapshot = snapshot;
    state.connected = true;
    renderSnapshot(snapshot);
    notice(state.paused ? 'Automatic refresh is paused. Select Resume to follow new activity.' : '');
  } catch {
    state.connected = false;
    state.clockReady = false;
    element('connection-badge').className = 'connection-badge disconnected';
    notice(state.snapshot ? 'Connection lost. Showing the last snapshot; reconnecting automatically while refresh is enabled.' : 'The observer is unavailable. Check that the MCP server is running; this dashboard will retry automatically.', true);
  } finally { finishRefresh(); }
}

function finishRefresh() {
  state.loading = false;
  element('refresh-button').disabled = false;
  connectionState();
  if (!state.connected && !state.paused) {
    element('connection-badge').className = 'connection-badge disconnected';
    setText('connection-label', 'Disconnected');
  }
  clearTimeout(state.timer);
  if (!state.paused) state.timer = setTimeout(refresh, 2000);
}

function togglePause() {
  state.clock = tickClock(state.clock, performance.now(), clockAdvancing());
  state.paused = !state.paused;
  state.clockReady = false;
  state.refreshGeneration += 1;
  clearTimeout(state.timer);
  connectionState();
  if (state.paused) notice('Automatic refresh is paused. Select Resume to follow new activity.');
  else { notice(''); refresh(); }
}

function selectTool(name) {
  element('tool-filter').value = name;
  renderCalls();
  element('calls').scrollIntoView({ behavior: 'smooth', block: 'start' });
  element('tool-filter').focus({ preventScroll: true });
}

function clearFilters() {
  element('call-search').value = '';
  element('tool-filter').value = '';
  element('status-filter').value = '';
  element('source-filter').value = '';
  element('capability-filter').value = '';
  renderCalls();
  element('call-search').focus();
}

function bindNavigation() {
  const sections = document.querySelectorAll('main section[id]');
  const observer = new IntersectionObserver(entries => {
    const visible = entries.filter(entry => entry.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
    if (!visible) return;
    for (const link of document.querySelectorAll('.nav-link')) {
      const active = link.hash === `#${visible.target.id}`;
      link.classList.toggle('active', active);
      if (active) link.setAttribute('aria-current', 'location');
      else link.removeAttribute('aria-current');
    }
  }, { rootMargin: '-90px 0px -55% 0px' });
  sections.forEach(section => observer.observe(section));
}

function start() {
  element('pause-button').addEventListener('click', togglePause);
  element('refresh-button').addEventListener('click', refresh);
  element('clear-filters').addEventListener('click', clearFilters);
  element('call-search').addEventListener('input', renderCalls);
  element('tool-filter').addEventListener('change', renderCalls);
  element('status-filter').addEventListener('change', renderCalls);
  element('source-filter').addEventListener('change', renderCalls);
  element('capability-filter').addEventListener('change', renderCalls);
  bindNavigation();
  setInterval(renderClock, 1000);
  refresh();
}

if (typeof document !== 'undefined') start();
