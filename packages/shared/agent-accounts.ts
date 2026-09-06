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
  resetsAt?: string;
  model?: string;
}
export interface AgentAccount {
  id: string;
  provider: AccountProvider;
  selector: string;
  name: string;
  email: string;
  color: AccountIconColor;
  eligible: boolean;
  registered: boolean;
  sharedHistory: boolean;
  status: 'ready' | 'loginRequired' | 'unavailable' | 'identityChanged';
  usage: AccountUsageWindow[];
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
export type AgentAccountsRequest =
  | { operation: 'list'; refresh?: boolean }
  | { operation: 'register'; provider: AccountProvider; selector: string; shareHistory: true; id?: string }
  | { operation: 'update'; id: string; name: string; color: AccountIconColor; eligible: boolean }
  | { operation: 'remove'; id: string }
  | { operation: 'defaults'; provider: AccountProvider; policy: AccountPolicy }
  | { operation: 'defaultAccount'; provider: AccountProvider; accountId: string | null }
  | { operation: 'session'; refresh?: boolean }
  | { operation: 'sessionPolicy'; policy: AccountPolicy | null }
  | { operation: 'select'; accountId: string | null }
  | { operation: 'stopRecovery' };
export type AccountsTransport = (request: AgentAccountsRequest) => Promise<AgentAccountsState>;
