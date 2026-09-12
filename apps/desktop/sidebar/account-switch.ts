import type { AccountsTransport, AgentAccountsState } from '@/packages/shared/agent-accounts';

export interface AccountSwitchProgress {
  provider: 'claude' | 'codex';
  email: string;
  indicator: string;
}

/** CDXC:AgentProviders 2026-09-07 DECISION: Show the selected provider and account email during a switch, starting before the request and clearing on success or failure. */
export function createAccountSwitchTransport(
  transport: AccountsTransport,
  publish: (progress: AccountSwitchProgress | null) => void
): AccountsTransport {
  let accounts: AgentAccountsState | undefined;
  return async (params) => {
    const account =
      params.operation === 'select' ? accounts?.accounts.find((row) => row.id === params.accountId) : undefined;
    const progress = account
      ? {
          provider: account.provider,
          email: account.email || account.name,
          indicator: account.indicator || account.selector,
        }
      : null;
    const switching = params.operation === 'select' && progress && accounts?.session?.accountId !== params.accountId;
    if (switching) publish(progress);
    try {
      const result = await transport(params);
      accounts = result;
      return result;
    } finally {
      if (switching) publish(null);
    }
  };
}
