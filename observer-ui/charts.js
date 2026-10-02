const SVG_NS = 'http://www.w3.org/2000/svg';
const HEIGHT = 190;
const LEFT = 34;
const TOP = 12;
const RIGHT = 10;
const BOTTOM = 30;

function svgNode(name, attributes = {}, content) {
  const node = document.createElementNS(SVG_NS, name);
  for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
  if (content !== undefined) node.textContent = content;
  return node;
}

function timeLabel(milliseconds) {
  return new Date(milliseconds).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function detailText(bucket) {
  const latency = bucket.calls ? ` · ${Math.round(bucket.average_duration_ms)} ms average` : '';
  return `${timeLabel(bucket.minute_start_ms)} · ${bucket.calls} calls · ${bucket.errors} failed${latency}`;
}

function addGrid(chart, maximum, width) {
  for (let step = 0; step <= 3; step += 1) {
    const y = TOP + (HEIGHT - TOP - BOTTOM) * step / 3;
    chart.append(svgNode('line', { x1: LEFT, x2: width - RIGHT, y1: y, y2: y, class: 'chart-grid' }));
    const value = Math.round(maximum * (1 - step / 3));
    chart.append(svgNode('text', { x: LEFT - 9, y: y + 3, 'text-anchor': 'end', class: 'chart-label' }, value));
  }
}

function addAxis(chart, buckets, width) {
  if (!buckets.length) return;
  const positions = [0, Math.floor((buckets.length - 1) / 2), buckets.length - 1];
  for (const [index, position] of positions.entries()) {
    const x = LEFT + (width - LEFT - RIGHT) * index / 2;
    const anchor = index === 0 ? 'start' : index === 2 ? 'end' : 'middle';
    chart.append(svgNode('text', { x, y: HEIGHT - 9, 'text-anchor': anchor, class: 'chart-label' }, timeLabel(buckets[position].minute_start_ms)));
  }
}

function chartBar(bucket, index, count, maximum, chartWidth) {
  const plotHeight = HEIGHT - TOP - BOTTOM;
  const step = (chartWidth - LEFT - RIGHT) / count;
  const width = Math.max(2, step * .52);
  const x = LEFT + index * step + (step - width) / 2;
  const baseline = HEIGHT - BOTTOM;
  const totalHeight = bucket.calls / maximum * plotHeight;
  const errorHeight = bucket.errors / maximum * plotHeight;
  const group = svgNode('g');
  group.append(svgNode('rect', { x, y: baseline - totalHeight, width, height: Math.max(0, totalHeight - errorHeight), rx: 1, class: 'chart-bar-success' }));
  group.append(svgNode('rect', { x, y: baseline - errorHeight, width, height: errorHeight, rx: 1, class: 'chart-bar-error' }));
  return { group, x: LEFT + index * step, step };
}

function navigateChart(event, hits, index) {
  const direction = { ArrowLeft: -1, ArrowRight: 1, Home: -index, End: hits.length - index - 1 }[event.key];
  if (direction === undefined) return;
  event.preventDefault();
  const next = Math.max(0, Math.min(hits.length - 1, index + direction));
  hits[index].setAttribute('tabindex', '-1');
  hits[next].setAttribute('tabindex', '0');
  hits[next].focus();
}

function interactiveBar(bucket, bar, index, hits, detail, container) {
  const hit = svgNode('rect', { x: bar.x, y: TOP, width: bar.step, height: HEIGHT - TOP - BOTTOM, class: 'chart-hit', tabindex: index === 0 ? '0' : '-1', role: 'img', 'aria-label': detailText(bucket) });
  hit.dataset.minute = bucket.minute_start_ms;
  const select = () => { detail.textContent = detailText(bucket); container.dataset.selectedMinute = bucket.minute_start_ms; };
  hit.addEventListener('mouseenter', select);
  hit.addEventListener('focus', select);
  hit.addEventListener('keydown', event => navigateChart(event, hits, index));
  bar.group.append(hit);
  hits.push(hit);
  return bar.group;
}

export function renderVolumeChart(container, detail, timeline) {
  const focusedMinute = document.activeElement?.dataset.minute;
  const style = getComputedStyle(container);
  const width = Math.max(260, container.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight));
  const buckets = timeline.slice(-60);
  const chart = svgNode('svg', { viewBox: `0 0 ${width} ${HEIGHT}`, role: 'group', 'aria-label': 'Tool call volume by minute. Use left and right arrow keys to inspect minutes.' });
  const maximum = Math.max(3, ...buckets.map(bucket => bucket.calls));
  const hits = [];
  addGrid(chart, maximum, width);
  addAxis(chart, buckets, width);
  buckets.forEach((bucket, index) => chart.append(interactiveBar(bucket, chartBar(bucket, index, buckets.length, maximum, width), index, hits, detail, container)));
  if (!buckets.some(bucket => bucket.calls)) chart.append(svgNode('text', { x: width / 2, y: HEIGHT / 2, 'text-anchor': 'middle', class: 'chart-label' }, 'No tool calls in the last 60 minutes'));
  container.replaceChildren(chart);
  const selected = buckets.find(bucket => String(bucket.minute_start_ms) === container.dataset.selectedMinute);
  detail.textContent = selected ? detailText(selected) : 'Hover a minute to inspect its activity. Use arrow keys when the chart is focused.';
  if (focusedMinute) {
    const focused = hits.find(hit => hit.dataset.minute === focusedMinute);
    if (focused) { hits.forEach(hit => hit.setAttribute('tabindex', hit === focused ? '0' : '-1')); focused.focus({ preventScroll: true }); }
  }
}

export function renderOutcomes(summary) {
  const completed = summary.successes + summary.errors;
  const circumference = 2 * Math.PI * 63;
  const errorLength = completed ? summary.errors / completed * circumference : 0;
  document.getElementById('outcome-errors').setAttribute('stroke-dasharray', `${errorLength} ${circumference - errorLength}`);
  document.getElementById('outcome-title').textContent = `${summary.successes} successful calls and ${summary.errors} failed calls this session`;
}
