export type AccountProvider = 'claude' | 'codex';
export const ACCOUNT_ICON_COLORS = [
  { id: 'neutral', label: 'Default', color: '#dddddd' },
  { id: 'slate', label: 'Slate', color: '#a8b4c3' },
  { id: 'coral', label: 'Coral', color: '#db967e' },
  { id: 'rose', label: 'Rose', color: '#d598b2' },
  { id: 'lavender', label: 'Lavender', color: '#b5a0d6' },
  { id: 'sky', label: 'Sky', color: '#8db7dc' },
  { id: 'teal', label: 'Teal', color: '#81b8b2' },
  { id: 'sage', label: 'Sage', color: '#a6bc91' },
  { id: 'sand', label: 'Sand', color: '#d1bd8b' },
] as const;
export type AccountIconColor = (typeof ACCOUNT_ICON_COLORS)[number]['id'];
export const accountIconColor = (id?: string) => ACCOUNT_ICON_COLORS.find((c) => c.id === id)?.color ?? '#dddddd';
export interface AccountPolicy {
  enabled: boolean;
  atLimit: 'wait' | 'switch';
  priority: 'leastUsed' | 'mostUsed' | 'soonestReset' | 'latestReset';
  retryErrors: boolean;
}
export const DEFAULT_ACCOUNT_POLICY: AccountPolicy = {
  enabled: false,
  atLimit: 'wait',
  priority: 'soonestReset',
  retryErrors: true,
};
export interface AccountUsageWindow {
  id: string;
  label: string;
  usedPercent: number;
  limitWindowSeconds?: number;
  resetsAt?: string;
  model?: string;
}
export interface AgentAccount {
  id: string;
  provider: AccountProvider;
  selector: string;
  indicator?: string;
  name: string;
  email: string;
  color: AccountIconColor;
  eligible: boolean;
  registered: boolean;
  sharedHistory: boolean;
  status: 'ready' | 'loginRequired' | 'unavailable' | 'identityChanged';
  usage: AccountUsageWindow[];
  resetCredits?: number;
  showInTitlebar?: boolean;
  usageUpdatedAt?: string;
  usageError?: string;
  sessionCount: number;
}
export interface AccountHelper {
  provider: AccountProvider;
  installed: boolean;
  cliInstalled: boolean;
  error?: string;
  installCommand: string;
  loginCommand: string;
}
export interface AccountRecovery {
  status: 'waiting' | 'retrying' | 'resumed' | 'needsAttention';
  reason: string;
  attempt: number;
  nextAttemptAt?: string;
  updatedAt: string;
}
export interface AgentAccountsState {
  setupJobs?: AccountSetupJob[];
  accounts: AgentAccount[];
  helpers: AccountHelper[];
  defaults: Record<AccountProvider, AccountPolicy>;
  defaultAccounts: Partial<Record<AccountProvider, string>>;
  session?: {
    provider: AccountProvider;
    accountId: string | null;
    policy: AccountPolicy;
    override: AccountPolicy | null;
    recovery?: AccountRecovery;
  };
}

/** CDXC:AgentProviders 2026-09-09 DECISION: User: quick launch starts with the saved default account. If that account has any usage window at 100%, the provider's saved switch/wait and account-preference rules choose the account for the new session. SEE-ALSO: server/src/accounts/recovery.rs. */
export function quickLaunchAccountId(
  state: AgentAccountsState,
  provider: AccountProvider
): string | undefined {
  const accounts = state.accounts.filter((account) => account.registered && account.provider === provider);
  const selected =
    accounts.find((account) => account.id === state.defaultAccounts[provider]) ??
    accounts.toSorted((left, right) => Number(left.selector) - Number(right.selector))[0];
  if (!selected || !selected.usage.some((window) => window.usedPercent >= 100)) return selected?.id;

  const policy = state.defaults[provider] ?? DEFAULT_ACCOUNT_POLICY;
  if (!policy.enabled || policy.atLimit !== 'switch') return selected.id;

  const score = (account: AgentAccount) =>
    account.usage.reduce((highest, window) => Math.max(highest, window.usedPercent), 0);
  const reset = (account: AgentAccount) =>
    account.usage.reduce<number | undefined>((earliest, window) => {
      const timestamp = window.resetsAt ? Date.parse(window.resetsAt) : Number.NaN;
      if (!Number.isFinite(timestamp)) return earliest;
      return earliest === undefined ? timestamp : Math.min(earliest, timestamp);
    }, undefined);
  const candidates = accounts
    .filter(
      (account) =>
        account.id !== selected.id &&
        account.eligible &&
        account.status === 'ready' &&
        !account.usageError &&
        account.usage.length > 0 &&
        account.usage.every((window) => window.usedPercent < 100)
    )
    .toSorted((left, right) => {
      if (policy.priority === 'leastUsed') return score(left) - score(right) || left.id.localeCompare(right.id);
      if (policy.priority === 'mostUsed') return score(right) - score(left) || left.id.localeCompare(right.id);
      const leftReset = reset(left);
      const rightReset = reset(right);
      const resetOrder =
        leftReset === undefined
          ? rightReset === undefined
            ? 0
            : 1
          : rightReset === undefined
            ? -1
            : policy.priority === 'latestReset'
              ? rightReset - leftReset
              : leftReset - rightReset;
      return resetOrder || left.id.localeCompare(right.id);
    });
  return candidates[0]?.id ?? selected.id;
}

export type AgentAccountsRequest =
  | { operation: 'setTitlebar'; id: string; shown: boolean }
  | { operation: 'setupStart'; owner: string; provider: AccountProvider; email: string; shareHistory: true; accountId?: string; selector?: string }
  | { operation: 'setupStatus'; owner: string }
  | { operation: 'setupInput'; owner: string; jobId: string; input: string }
  | { operation: 'setupCancel' | 'setupAcknowledge'; owner: string; jobId: string }
  | { operation: 'list'; refresh?: boolean }
  | { operation: 'register'; provider: AccountProvider; selector: string; shareHistory: true; id?: string }
  | { operation: 'update'; id: string; name: string; color: AccountIconColor; eligible: boolean; indicator?: string }
  | { operation: 'remove'; id: string }
  | { operation: 'swapSlots'; firstId: string; secondId: string }
  | { operation: 'defaults'; provider: AccountProvider; policy: AccountPolicy }
  | { operation: 'defaultAccount'; provider: AccountProvider; accountId: string | null }
  | { operation: 'session'; refresh?: boolean }
  | { operation: 'sessionPolicy'; policy: AccountPolicy | null }
  | { operation: 'select'; accountId: string | null }
  | { operation: 'stopRecovery' };
export type AccountsTransport = (request: AgentAccountsRequest) => Promise<AgentAccountsState>;
export interface AccountSetupJob {
  createdAt: number;
  id: string;
  provider: AccountProvider;
  email: string;
  status: 'signingIn' | 'saving' | 'complete' | 'failed';
  accountId?: string;
  url?: string;
  output: string;
  error?: string;
  acknowledged: boolean;
}

/** Account labels contain up to two letters or digits; a lone hyphen hides the indicator. */
export function normalizeAccountIndicatorInput(value: string): string {
  return value === '-' ? '-' : (value.match(/[\p{L}\p{N}]/gu) ?? []).slice(0, 2).join('');
}
