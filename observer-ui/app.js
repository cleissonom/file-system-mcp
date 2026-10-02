import { renderVolumeChart, renderOutcomes } from './charts.js';

const state = { snapshot: null, paused: false, loading: false, connected: false, timer: null };
const outcomeLabels = { success: 'Success', tool_error: 'Tool error', protocol_error: 'Protocol error' };
const element = id => document.getElementById(id);
const setText = (id, value) => { element(id).textContent = value; };

export function formatDuration(milliseconds) {
  if (milliseconds >= 1000) return `${(milliseconds / 1000).toFixed(2)} s`;
  return `${milliseconds < 10 ? milliseconds.toFixed(1) : Math.round(milliseconds)} ms`;
}

export function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatUptime(milliseconds) {
  const seconds = Math.floor(milliseconds / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

export function filterCalls(calls, query, tool, status) {
  const normalized = query.trim().toLowerCase();
  return calls.filter(call => (!tool || call.tool === tool)
    && (!status || (status === 'error' ? call.outcome !== 'success' : call.outcome === status))
    && (!normalized || `${call.tool} ${outcomeLabels[call.outcome] || call.outcome}`.toLowerCase().includes(normalized)));
}

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

function emptyRow(columns, title, description) {
  const row = node('tr');
  const cell = node('td', 'table-empty');
  cell.colSpan = columns;
  cell.append(node('strong', 'empty-title', title), node('span', '', description));
  row.append(cell);
  return row;
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
  setText('server-uptime', formatUptime(server.uptime_ms));
  setText('server-pid', server.pid);
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
  const { summary, active_calls } = snapshot;
  setText('request-bytes', formatBytes(summary.request_bytes));
  setText('response-bytes', formatBytes(summary.response_bytes));
  setText('outcome-total', number(summary.successes + summary.errors));
  setText('outcome-successes', number(summary.successes));
  setText('outcome-failures', number(summary.errors));
  renderOutcomes(summary);
  renderVolumeChart(element('volume-chart'), element('chart-detail'), snapshot.timeline);
  const rows = active_calls.slice(0, 3).map(call => activeRow(call, snapshot.server));
  if (!rows.length) rows.push(node('span', '', 'No calls in flight'));
  if (active_calls.length > 3) rows.push(node('span', '', `+ ${active_calls.length - 3} more active calls`));
  element('active-tools').replaceChildren(...rows);
}

function activeRow(call, server) {
  const row = node('div', 'active-item');
  const now = server.started_at_ms + server.uptime_ms;
  row.append(node('span', '', call.tool), node('span', '', formatDuration(Math.max(0, now - call.started_at_ms))));
  return row;
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

function callRow(call) {
  const row = node('tr');
  const outcome = node('td');
  outcome.append(node('span', `call-outcome${call.outcome === 'success' ? '' : ' failure'}`, outcomeLabels[call.outcome] || 'Unknown outcome'));
  const tool = node('td');
  tool.append(node('span', 'call-tool', call.tool), node('span', 'call-sequence', `#${call.sequence}`));
  const time = node('td', 'call-time', clock(call.started_at_ms));
  time.title = new Date(call.started_at_ms).toLocaleString();
  row.append(outcome, tool, time, node('td', 'numeric', formatDuration(call.duration_ms)));
  row.append(node('td', 'numeric muted-value', formatBytes(call.request_bytes)), node('td', 'numeric muted-value', formatBytes(call.response_bytes)));
  return row;
}

function renderCalls() {
  if (!state.snapshot) return;
  const query = element('call-search').value;
  const tool = element('tool-filter').value;
  const status = element('status-filter').value;
  const calls = filterCalls(state.snapshot.recent_calls, query, tool, status);
  element('clear-filters').hidden = !(query || tool || status);
  setText('filtered-count', `${number(calls.length)} calls`);
  if (calls.length) element('call-rows').replaceChildren(...calls.map(callRow));
  else renderCallEmpty(Boolean(query || tool || status));
  for (const button of element('tool-rows').querySelectorAll('button')) button.setAttribute('aria-pressed', String(button.dataset.tool === tool));
}

function renderCallEmpty(filtered) {
  const title = filtered ? 'No matching calls' : 'Quiet for now';
  const description = filtered ? 'Try a different tool, outcome or search term.' : 'Run an MCP tool from ChatGPT. Its usage metadata will appear here.';
  element('call-rows').replaceChildren(emptyRow(6, title, description));
}

function renderSnapshot(snapshot) {
  renderMetrics(snapshot);
  renderTraffic(snapshot);
  renderTools(snapshot.tools);
  renderCalls();
  renderStorage(snapshot);
  setText('nav-count', number(snapshot.summary.total_calls));
  setText('call-retention', `UP TO ${number(snapshot.retention.recent_call_limit)} RECENT CALLS`);
  setText('last-updated', `Last snapshot · ${clock(Date.now())}`);
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
  state.loading = true;
  element('refresh-button').disabled = true;
  try {
    const response = await fetch('/api/snapshot', { cache: 'no-store', signal: AbortSignal.timeout(5000) });
    if (!response.ok) throw new Error('Snapshot unavailable');
    const snapshot = await response.json();
    if (!validSnapshot(snapshot)) throw new Error('Invalid snapshot');
    state.snapshot = snapshot;
    state.connected = true;
    renderSnapshot(snapshot);
    notice(state.paused ? 'Automatic refresh is paused. Select Resume to follow new activity.' : '');
  } catch {
    state.connected = false;
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
  state.paused = !state.paused;
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
  bindNavigation();
  refresh();
}

if (typeof document !== 'undefined') start();
