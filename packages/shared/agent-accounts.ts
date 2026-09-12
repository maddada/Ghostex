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
export interface AccountUsageHistory {
  status: 'ready' | 'partial';
  hasData: boolean;
  days: { date: string; tokens: number }[];
  updatedAt: string;
  timeZone: string;
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
  resetCreditDetails?: { id: string; expiresAt: string | null }[] | null;
  resetCreditsError?: string | null;
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
/** Real account-switch stages carried by chat reads, snapshots and state frames. Omission leaves the last value; null clears it. */
export interface AccountSwitchProgress {
  id: string;
  provider: AccountProvider;
  source: 'manual' | 'automatic';
  phase: 'switching' | 'resuming' | 'continuing' | 'success' | 'failed' | 'cancelled';
  accountReady?: boolean;
  fromAccountId: string | null;
  toAccountId: string | null;
  updatedAt: string;
  reason?: string;
}
export interface AccountRecovery {
  status: 'waiting' | 'retrying' | 'resumed' | 'needsAttention';
  reason: string;
  attempt: number;
  nextAttemptAt?: string;
  updatedAt: string;
}
export type NewSessionAccountRule = 'auto' | 'mostRemaining' | 'soonestReset' | 'mostUsed' | 'lastUsed';
export type NewSessionAccountChoice = { rule: NewSessionAccountRule } | { rule: 'pinned'; id: string };
/** Automatic rules for the account that starts new sessions, in the order the Settings dropdown lists them. */
export const NEW_SESSION_ACCOUNT_RULES: { rule: NewSessionAccountRule; label: string }[] = [
  { rule: 'auto', label: 'Auto (recommended)' },
  { rule: 'mostRemaining', label: 'Most limit remaining' },
  { rule: 'soonestReset', label: 'Soonest reset' },
  { rule: 'mostUsed', label: 'Most used first' },
  { rule: 'lastUsed', label: 'Same as last session' },
];
export interface AgentAccountsState {
  usageHistory?: Partial<Record<AccountProvider, AccountUsageHistory>> | null;
  accountCounts?: Record<AccountProvider, number>;
  setupJobs?: AccountSetupJob[];
  accounts: AgentAccount[];
  helpers: AccountHelper[];
  defaults: Record<AccountProvider, AccountPolicy>;
  /** Effective account for new sessions per provider, resolved by gxserver from the provider's rule. */
  defaultAccounts: Partial<Record<AccountProvider, string>>;
  /** Per-provider rule from Settings; absent means Auto. */
  newSessionAccounts?: Partial<Record<AccountProvider, NewSessionAccountChoice>>;
  session?: {
    provider: AccountProvider;
    accountId: string | null;
    policy: AccountPolicy;
    override: AccountPolicy | null;
    recovery?: AccountRecovery;
    accountSwitch?: AccountSwitchProgress | null;
  };
}

/**
 * CDXC:AgentProviders 2026-09-11 DECISION:
 * User: quick launch starts new sessions with the account chosen by the provider's Account for new sessions rule (Auto by default, or Most limit remaining, Soonest reset, Most used first, Same as last session, or one pinned account). This supersedes the 2026-09-09 saved default account and its at-limit switch step. gxserver resolves the rule once from the registry and the live usage snapshot and publishes the result as `defaultAccounts`, so every surface reads the same answer instead of ranking accounts itself. When the rule yields no account there is no entry and the launch keeps the current CLI login.
 * SEE-ALSO: server/src/accounts/default_account.rs.
 */
export function quickLaunchAccountId(state: AgentAccountsState, provider: AccountProvider): string | undefined {
  return state.defaultAccounts[provider];
}

export type AgentAccountsRequest =
  | { operation: 'prepareReset'; id: string }
  | { operation: 'titlebar'; cachedOnly?: boolean }
  | { operation: 'setTitlebar'; id: string; shown: boolean }
  | {
      operation: 'setupStart';
      owner: string;
      provider: AccountProvider;
      email: string;
      shareHistory: true;
      accountId?: string;
      selector?: string;
    }
  | { operation: 'setupStatus'; owner: string }
  | { operation: 'setupInput'; owner: string; jobId: string; input: string }
  | { operation: 'setupCancel' | 'setupAcknowledge'; owner: string; jobId: string }
  | { operation: 'list'; refresh?: boolean }
  | { operation: 'register'; provider: AccountProvider; selector: string; shareHistory: true; id?: string }
  | { operation: 'update'; id: string; name: string; color: AccountIconColor; eligible: boolean; indicator?: string }
  | { operation: 'remove'; id: string }
  | { operation: 'swapSlots'; firstId: string; secondId: string }
  | { operation: 'defaults'; provider: AccountProvider; policy: AccountPolicy }
  | { operation: 'defaultAccount'; provider: AccountProvider; choice: NewSessionAccountChoice }
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
