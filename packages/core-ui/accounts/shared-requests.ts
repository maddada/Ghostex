import type { AccountsTransport, AgentAccountsRequest, AgentAccountsState } from '@/packages/shared/agent-accounts';

type Requests = {
  version: number;
  reads: Map<string, Promise<AgentAccountsState>>;
  writes: Set<Promise<AgentAccountsState>>;
};
const requests = new WeakMap<AccountsTransport, Requests>();

/**
 * CDXC:AgentProviders 2026-09-11 WHY:
 * Account transports capture machine and session routing, so only identical transports and provider contexts can share reads.
 * Share in-flight work only: completed session replies can become stale after a draft changes provider or another view changes accounts.
 * Forced refreshes and mutations invalidate reads before and after they settle so an earlier poll cannot win over their result.
 */
export async function requestSharedAccounts(
  transport: AccountsTransport,
  params: AgentAccountsRequest,
  sessionAgentId?: string | null
): Promise<AgentAccountsState> {
  let state = requests.get(transport);
  if (!state) {
    state = { version: 0, reads: new Map(), writes: new Set() };
    requests.set(transport, state);
  }
  const shared = state;
  const ordinaryRead = (params.operation === 'list' || params.operation === 'session') && !params.refresh;
  if (!ordinaryRead) {
    shared.version++;
    shared.reads.clear();
    const operation = Promise.resolve().then(() => transport(params));
    shared.writes.add(operation);
    try {
      return await operation;
    } finally {
      shared.writes.delete(operation);
      shared.version++;
      shared.reads.clear();
    }
  }

  while (shared.writes.size) await Promise.allSettled([...shared.writes]);
  const key = JSON.stringify([params.operation, sessionAgentId ?? null]);
  const existing = shared.reads.get(key);
  if (existing) return existing;
  const version = shared.version;
  const operation = Promise.resolve()
    .then(() => transport(params))
    .then(
      (result) => (version === shared.version ? result : requestSharedAccounts(transport, params, sessionAgentId)),
      (error: unknown) => {
        if (version !== shared.version) return requestSharedAccounts(transport, params, sessionAgentId);
        throw error;
      }
    );
  shared.reads.set(key, operation);
  try {
    return await operation;
  } finally {
    if (shared.reads.get(key) === operation) shared.reads.delete(key);
  }
}
