import type { AccountPolicy, AccountUsageWindow, AgentAccount } from '@/packages/shared/agent-accounts';
import { accountHeadlineWindows } from '@/packages/shared/account-usage-windows';
import { formatResetCountdown } from '@/packages/shared/reset-countdown';

/*
Account copy and figures for the Settings account controls (controls.tsx). The chat's own
account rows and switch card are drawn from the Rust chat core
(packages/gx-chat-core/src/menus/accounts_presentation.rs), which carries the same rules.
Labels are returned unmasked; each renderer applies Hide emails itself.
*/

/** One of the two small figures stacked beside an account's logo. */
export interface AccountFigure {
  label?: string;
  value: string;
}

/**
 * CDXC:AgentProviders 2026-09-08 DECISION:
 * User: Codex account badges show the five-hour percentage on the second line when that limit exists; otherwise show available resets as "2rs" or "0rs".
 * Use the main account windows so Spark's separate five-hour limit does not stand in for an absent account limit.
 * Claude figures are the two tightest of weekly, five-hour, and Fable (see accountHeadlineWindows).
 */
export function accountFigureWindows(
  account: AgentAccount
): [AccountUsageWindow | undefined, AccountUsageWindow | undefined] {
  const [first, second] = accountHeadlineWindows(account);
  return [first, second];
}

export function accountFigures(account: AgentAccount): [AccountFigure, AccountFigure] {
  const [first, second] = accountFigureWindows(account);
  return [
    { label: first?.label, value: first ? `${Math.round(first.usedPercent)}%` : '·' },
    second
      ? { label: second.label, value: `${Math.round(second.usedPercent)}%` }
      : account.provider === 'codex' && account.resetCredits != null
        ? { label: 'Available usage resets', value: `${account.resetCredits}rs` }
        : { label: undefined, value: '·' },
  ];
}

export function accountResetLabel(value: string | undefined, now = Date.now()): string {
  if (!value) return 'Reset time unavailable';
  const time = new Date(value);
  if (!Number.isFinite(time.getTime())) return 'Reset time unavailable';
  const remainingMs = time.getTime() - now;
  return remainingMs > 0 ? `Resets ${formatResetCountdown(remainingMs)}` : 'Reset due';
}

/** One "Resets 2h 14m · 3d 6h" line for several limits, in the order given, skipping limits without a reset time. */
export function accountResetsLine(windows: AccountUsageWindow[], now = Date.now()): string {
  const remaining = windows
    .map((window) => (window.resetsAt ? new Date(window.resetsAt).getTime() - now : Number.NaN))
    .filter((ms) => Number.isFinite(ms));
  if (remaining.length === 0) return 'Reset time unavailable';
  return `Resets ${remaining.map((ms) => (ms > 0 ? formatResetCountdown(ms) : 'due')).join(' · ')}`;
}

export const ACCOUNT_POLICY_PRIORITY_OPTIONS: { value: AccountPolicy['priority']; label: string }[] = [
  { value: 'leastUsed', label: 'Lowest usage first' },
  { value: 'mostUsed', label: 'Highest usage first' },
  { value: 'soonestReset', label: 'Earliest reset first' },
  { value: 'latestReset', label: 'Latest reset first' },
];

export function accountPolicyAtLimitDescription(policy: AccountPolicy): string {
  return policy.atLimit === 'wait'
    ? 'Pick up on this account when its usage resets.'
    : 'Use another eligible login for this model. Wait when every account is at its limit.';
}

export const ACCOUNT_POLICY_RETRY_DESCRIPTION =
  'Retry after 5, 10, 20, 40, then every 60 minutes. Login and permission requests need your attention. Stop cancels recovery for the current task.';
