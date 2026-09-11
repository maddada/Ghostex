// CDXC:AgentProviders 2026-09-11 DECISION: User approved per-account live limits above always-visible shared provider history, replacing the earlier extension layout. Both sections use gxserver snapshots.
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
  return eta > 0 && eta < reset - Date.now() ? `At this pace the limit is hit in ${duration(eta)}` : '';
}
function renderBars(values, labels, codex, hostId = 'bars') {
  const host = byId(hostId);
  host.replaceChildren();
  values.forEach((bar, index) => {
    const row = document.createElement('div');
    const head = document.createElement('div');
    const label = document.createElement('span');
    const meta = document.createElement('span');
    const track = document.createElement('div');
    const fill = document.createElement('div');
    const percent = Number.isFinite(bar?.usedPercent) ? bar.usedPercent : null;
    const warning = codex ? paceWarning(bar) : '';
    head.className = 'bar-head';
    label.className = 'bar-label';
    meta.className = 'bar-meta';
    label.textContent = labels[index] || bar?.label || 'Usage';
    const reset = bar?.resetsAt ? resetText(bar.resetsAt) : '';
    if (percent !== null) {
      const value = document.createElement('strong');
      value.textContent = `${Math.round(percent)}%`;
      meta.append(value, reset ? ` · ${reset}` : '');
    } else {
      meta.textContent = reset || 'Not reported';
    }
    head.append(label, meta);
    track.className = 'track';
    fill.className = 'fill' + (percent >= 95 ? ' danger' : warning || percent >= 80 ? ' warning' : '');
    fill.style.width = `${Math.min(100, Math.max(0, percent ?? 0))}%`;
    track.append(fill);
    row.append(head, track);
    if (warning) {
      const note = document.createElement('div');
      note.className = 'pace-warning';
      note.textContent = warning;
      row.append(note);
    }
    host.append(row);
  });
}
const compactTokens = new Intl.NumberFormat(undefined, { notation: 'compact', maximumFractionDigits: 1 });
const exactTokens = new Intl.NumberFormat();
function renderHistory(provider) {
  const history = account.usageHistory;
  const days = Array.isArray(history?.days) ? history.days : [];
  const known = history?.hasData === true;
  const remote = account.titlebarMachine && account.titlebarMachine !== 'local';
  byId('historyHeading').textContent = `Shared ${provider} history`;
  const count = account.providerAccountCount;
  byId('historyScope').textContent = count > 0 ? `${count} saved account${count === 1 ? '' : 's'}` : 'All accounts';
  byId('historyDescription').textContent =
    `Combined stats from all ${provider} conversations on ${remote ? 'the remote computer' : 'this computer'}.`;
  const totals = [days.at(-1)?.tokens ?? 0, days.at(-2)?.tokens ?? 0, days.reduce((sum, day) => sum + day.tokens, 0)];
  ['todayTokens', 'yesterdayTokens', 'thirtyTokens'].forEach((id, index) => {
    byId(id).textContent = known ? compactTokens.format(totals[index]) : 'No data';
    byId(id).title = known ? `${exactTokens.format(totals[index])} tokens` : '';
  });
  const chart = byId('historyChart');
  chart.replaceChildren();
  const peak = Math.max(1, ...days.map((day) => day.tokens));
  for (const day of days) {
    const bar = document.createElement('div');
    bar.className = 'history-day';
    bar.style.height = `${day.tokens > 0 ? Math.max(3, (day.tokens / peak) * 100) : 0}%`;
    bar.title = `${day.date}: ${exactTokens.format(day.tokens)} tokens`;
    chart.append(bar);
  }
  chart.setAttribute(
    'aria-label',
    known
      ? `Last 30 days: ${exactTokens.format(totals[2])} tokens. Today: ${exactTokens.format(totals[0])}. Yesterday: ${exactTokens.format(totals[1])}.`
      : 'No token history available'
  );
  byId('historySource').textContent =
    `Conversation logs · Cached tokens included${history?.timeZone ? ` · Days in ${history.timeZone}` : ''}`;
  byId('historyNotice').textContent =
    !history || history.status === 'loading'
      ? 'Reading shared history…'
      : history.status === 'unavailable'
        ? 'Update Ghostex on this computer to read shared history.'
        : history.status === 'partial'
          ? 'Some history could not be read. Totals may be incomplete.'
          : !known
            ? 'No recorded token usage in the last 30 days.'
            : '';
}
function render() {
  const codex = account.provider === 'codex';
  const provider = codex ? 'Codex' : 'Claude';
  const windows = account.usage || [];
  const main = windows.filter((w) => !w.model && w.id !== 'spend');
  const session = main.find((w) => w.id === 'fiveHour' || w.limitWindowSeconds === 18000);
  const weekly = main.find((w) => w.id === 'sevenDay' || w.limitWindowSeconds >= 604800);
  // CDXC:AgentProviders 2026-09-11 DECISION: User: the Fable limit is the most important Claude number and must be visible wherever Claude usage bars are shown, so it is a main bar next to the five-hour and weekly limits instead of a collapsed model limit.
  const scoped = windows.filter((w) => w.model);
  const fable = codex ? undefined : (scoped.find((w) => String(w.model).toLowerCase().includes('fable')) ?? scoped[0]);
  const other = windows.filter((w) => w.id !== 'spend' && w !== session && w !== weekly && w !== fable);
  document.body.dataset.provider = account.provider;
  document.title = `${provider} usage`;
  byId('heading').textContent = `${provider} usage`;
  byId('plan').textContent = account.displayName || account.name || 'Account';
  byId('updated').textContent = account.usageUpdatedAt
    ? `Updated ${new Date(account.usageUpdatedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`
    : '';
  byId('notice').textContent =
    account.usageError ||
    (account.status === 'loading'
      ? 'Loading account usage…'
      : account.status === 'ready'
        ? ''
        : 'Reconnect this account in Settings > Accounts.');
  const mark = account.indicator || account.selector;
  byId('indicator').textContent = mark && mark !== '-' ? mark : '';
  byId('indicator').style.display = mark && mark !== '-' ? 'grid' : 'none';
  renderBars(
    fable ? [session, weekly, fable] : [session, weekly],
    fable ? ['5-hour limit', 'Weekly limit', `${fable.model} weekly limit`] : ['5-hour limit', 'Weekly limit'],
    codex
  );
  byId('modelLimits').hidden = other.length === 0;
  byId('modelSummary').textContent = `More model limits (${other.length})`;
  renderBars(
    other,
    other.map((w) => w.label),
    codex,
    'modelBars'
  );
  byId('extraLabel').textContent = codex ? 'Rate limit resets' : 'Extra usage';
  const extra = windows.find((w) => w.id === 'spend');
  byId('extra').textContent = codex
    ? account.resetCredits != null
      ? `${account.resetCredits} available`
      : 'Not reported'
    : extra
      ? `${Math.round(extra.usedPercent)}% used`
      : 'Not reported';
  byId('extra').hidden = codex;
  byId('resetTrigger').hidden = !codex;
  byId('resetTrigger').textContent = byId('extra').textContent;
  if (!codex) setResetMenu(false);
  renderResets();
  byId('statusLink').href = codex ? 'https://status.openai.com' : 'https://status.anthropic.com';
  byId('dashboardLink').href = codex ? 'https://chatgpt.com/codex/settings/usage' : 'https://claude.ai/settings/usage';
  renderHistory(provider);
}
let redeemPending = false;
function setResetMenu(open) {
  byId('resetMenu').hidden = !open;
  byId('resetTrigger').setAttribute('aria-expanded', String(open));
}
function renderResets() {
  const credits = [...(account.resetCreditDetails || [])].sort(
    (a, b) => (Date.parse(a.expiresAt) || Infinity) - (Date.parse(b.expiresAt) || Infinity)
  );
  byId('resetList').replaceChildren();
  for (const [index, credit] of credits.entries()) {
    const row = document.createElement('li');
    const date = new Date(credit.expiresAt);
    const expires = credit.expiresAt && Number.isFinite(date.getTime());
    const when = document.createElement('span');
    when.className = 'reset-when';
    when.textContent = expires ? `Expires in ${duration(date.getTime() - Date.now())}` : 'No expiry reported';
    const detail = document.createElement('span');
    detail.className = 'reset-date';
    detail.textContent = expires
      ? date.toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })
      : 'Expiry unavailable';
    row.append(when, detail);
    if (index === 0) {
      const chip = document.createElement('span');
      chip.className = 'reset-chip';
      chip.textContent = 'Used first';
      row.append(chip);
    }
    byId('resetList').append(row);
  }
  byId('resetNotice').textContent =
    account.resetCreditsError ||
    (!Array.isArray(account.resetCreditDetails)
      ? 'Reset expiry details are unavailable. Refreshing usage may help.'
      : credits.length
        ? ''
        : 'No resets available.');
  byId('redeemReset').disabled = redeemPending || !credits.length || account.status !== 'ready';
  byId('redeemReset').textContent = redeemPending ? 'Opening reset chat…' : 'Redeem a reset';
}
byId('resetTrigger').addEventListener('click', () => setResetMenu(byId('resetMenu').hidden));
byId('redeemReset').addEventListener('click', () => {
  if (byId('redeemReset').disabled) return;
  redeemPending = true;
  renderResets();
  window.open('ghostex-account:redeem-reset', '_blank');
});
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape' && !byId('resetMenu').hidden) {
    setResetMenu(false);
    byId('resetTrigger').focus();
    event.stopPropagation();
  }
});
document.addEventListener('click', (event) => {
  if (!byId('resetMenu').contains(event.target) && !byId('resetTrigger').contains(event.target)) setResetMenu(false);
});
window.ghostexUpdateAccountUsage = (value) => {
  account = value;
  render();
};
render();
setInterval(render, 30000);
