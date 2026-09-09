// CDXC:AgentProviders 2026-09-08 DECISION: Keep the Claude/Codex extension popup layout, using the saved cswap/xswap account snapshot instead of the extension's credential and transcript readers.
let account = __ACCOUNT_JSON__;
const byId = (id) => document.getElementById(id);
const duration = (ms) => {
  if (ms <= 0) return 'now';
  const hours = Math.floor(ms / 36e5),
    days = Math.floor(hours / 24);
  return days > 0
    ? `${days}d ${hours % 24}h`
    : hours > 0
      ? `${hours}h ${Math.floor((ms % 36e5) / 6e4)}m`
      : `${Math.max(1, Math.floor(ms / 6e4))}m`;
};
function resetText(value) {
  const ms = new Date(value).getTime() - Date.now();
  return value && Number.isFinite(ms) ? (ms <= 0 ? 'resets now' : `resets in ${duration(ms)}`) : 'reset unavailable';
}
function paceWarning(bar) {
  if (!bar?.resetsAt || !bar.limitWindowSeconds || bar.usedPercent < 5) return '';
  const reset = Date.parse(bar.resetsAt),
    period = bar.limitWindowSeconds * 1000;
  const elapsed = Date.now() - (reset - period);
  if (elapsed < Math.max(6e4, period * 0.01) || Date.now() >= reset) return '';
  const projected = (bar.usedPercent / elapsed) * period;
  if (projected <= 100) return '';
  const eta = (100 - bar.usedPercent) / (projected / period);
  return eta > 0 && eta < reset - Date.now() ? `🔥 Limit in ${duration(eta)}` : '';
}
function renderBars(hostId, values, labels, codex) {
  const host = byId(hostId);
  host.replaceChildren();
  values.forEach((bar, index) => {
    const row = document.createElement('div'),
      head = document.createElement('div');
    const label = document.createElement('span'),
      meta = document.createElement('span');
    const track = document.createElement('div'),
      fill = document.createElement('div');
    const warning = codex ? paceWarning(bar) : '';
    head.className = 'bar-head';
    label.className = 'bar-label';
    meta.className = 'bar-meta';
    label.textContent = `${labels[index] || bar?.label || 'Usage'}${bar ? `: ${Math.round(bar.usedPercent)}%` : ''}`;
    meta.textContent = codex
      ? warning
      : bar
        ? resetText(bar.resetsAt)
        : 'No live data';
    head.append(label, meta);
    track.className = 'track';
    fill.className = 'fill' + ((codex ? warning : bar?.usedPercent >= 80) ? ' warning' : '');
    fill.style.width = `${Math.min(100, Math.max(0, bar?.usedPercent ?? 0))}%`;
    track.append(fill);
    row.append(head, track);
    if (codex) {
      const foot = document.createElement('div'),
        reset = document.createElement('span');
      foot.className = 'bar-foot';
      reset.textContent = bar ? resetText(bar.resetsAt) : 'No data';
      foot.append(reset);
      row.append(foot);
    }
    host.append(row);
  });
}
function render() {
  const codex = account.provider === 'codex',
    windows = account.usage || [];
  const main = windows.filter((w) => !w.model && w.id !== 'spend');
  const session = main.find((w) => w.id === 'fiveHour' || w.limitWindowSeconds === 18000);
  const weekly = main.find((w) => w.id === 'sevenDay' || w.limitWindowSeconds >= 604800);
  const models = windows.filter((w) => w.model);
  byId('plan').textContent = account.displayName || account.name;
  byId('updated').textContent = account.usageUpdatedAt
    ? `Updated ${new Date(account.usageUpdatedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`
    : '';
  byId('notice').textContent =
    account.usageError || (account.status === 'loading' ? 'Loading account usage…' : account.status === 'ready' ? '' : 'Reconnect this account in Settings > Accounts.');
  const logo = document.querySelector('.logo');
  let indicator = logo.querySelector('.account-indicator');
  const mark = account.indicator || account.selector;
  if (mark && mark !== '-') {
    if (!indicator) {
      indicator = document.createElement('span');
      indicator.className = 'account-indicator';
      indicator.style.cssText =
        'position:absolute;top:-5px;left:-5px;min-width:13px;height:13px;padding:0 2px;display:grid;place-items:center;border-radius:50%;background:white;color:#171717;font:600 9.9px/13px ui-monospace,SFMono-Regular,monospace';
      logo.style.position = 'relative';
      logo.append(indicator);
    }
    indicator.textContent = mark;
  } else indicator?.remove();
  if (codex) {
    renderBars('coreBars', [session, weekly], ['5h', '7d'], true);
    const spark = models.filter((w) => /spark/i.test(w.model));
    renderBars(
      'sparkBars',
      [spark.find((w) => w.limitWindowSeconds === 18000), spark.find((w) => w.limitWindowSeconds >= 604800)],
      ['Spark 5h', 'Spark 7d'],
      true
    );
    byId('trend').textContent = 'No data';
    byId('resets').replaceChildren();
    if (account.resetCredits != null) {
      const dot = document.createElement('span');
      dot.className = 'reset-dot';
      byId('resets').append(dot, document.createTextNode(`${account.resetCredits} available`));
    } else byId('resets').textContent = 'No data';
  } else {
    renderBars('bars', [session, weekly, models[0]], ['5h', '7d', models[0]?.model ? `${models[0].model} 7d` : 'Top model'], false);
    byId('trendTotal').textContent = 'No data';
    const extra = windows.find((w) => w.id === 'spend');
    byId('extra').textContent = extra ? `${Math.round(extra.usedPercent)}%` : 'No data';
    ['today', 'yesterday', 'thirty'].forEach((id) => {
      byId(`${id}Cost`).textContent = '';
      byId(`${id}Tokens`).textContent = 'No data';
    });
  }
}
window.ghostexUpdateAccountUsage = (value) => {
  account = value;
  render();
};
render();
setInterval(render, 30000);
