import type { AccountUsageWindow, AgentAccount } from './agent-accounts';

export const isWeeklyWindow = (window: AccountUsageWindow) =>
  window.id === 'sevenDay' || (window.limitWindowSeconds ?? 0) >= 604800;
export const isFiveHourWindow = (window: AccountUsageWindow) =>
  window.id === 'fiveHour' || window.limitWindowSeconds === 18000;
/** The Fable model window when the helper reports one, else the first model-scoped window. */
export function fableWindow(usage: AccountUsageWindow[]): AccountUsageWindow | undefined {
  const scoped = usage.filter((window) => window.model);
  return scoped.find((window) => window.model?.toLowerCase().includes('fable')) ?? scoped[0];
}

/**
 * CDXC:AgentProviders 2026-09-11 DECISION:
 * User: for Claude accounts the Fable limit is the most important number and must never be hidden. Wherever a Claude account shows two percentages (titlebar buttons, launcher and picker rows, Settings figures), show the two tightest of the weekly, five-hour, and Fable limits, in that fixed order, so the number about to run out is always one of them. Codex keeps its weekly window and five-hour window.
 * SEE-ALSO: apps/desktop/src/app/titlebar/account_usage.rs `claude_headline_windows`, apps/desktop/src/app/window/account_usage/limits.rs.
 */
export function accountHeadlineWindows(account: AgentAccount): AccountUsageWindow[] {
  const main = account.usage.filter((window) => !window.model);
  const weekly = main.find(isWeeklyWindow);
  const fiveHour = main.find(isFiveHourWindow);
  if (account.provider !== 'claude') {
    return [weekly, fiveHour].filter((window): window is AccountUsageWindow => window !== undefined);
  }
  const candidates = [weekly, fiveHour, fableWindow(account.usage)].filter(
    (window): window is AccountUsageWindow => window !== undefined
  );
  const tightest = new Set(candidates.toSorted((left, right) => right.usedPercent - left.usedPercent).slice(0, 2));
  return candidates.filter((window) => tightest.has(window));
}
