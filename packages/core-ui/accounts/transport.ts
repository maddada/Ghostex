import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import type { AccountsTransport, AgentAccountsState } from '@/packages/shared/agent-accounts';
export interface AccountsConnection {
  id: string;
  label: string;
  request: AccountsTransport;
}
let connectionSource: (() => AccountsConnection[]) | undefined;
export function setAccountsConnectionSource(source: () => AccountsConnection[]) {
  connectionSource = source;
}
export function getAccountsConnections(): AccountsConnection[] {
  if (connectionSource) return connectionSource();
  const bootstrap = (
    window as unknown as { ghostexGpui?: { gxserverBootstrap?: { baseUrl: string; authToken: string } } }
  ).ghostexGpui?.gxserverBootstrap;
  if (!bootstrap?.baseUrl || !bootstrap.authToken) return [];
  return [
    {
      id: 'local',
      label: 'This computer',
      request: async (params) => {
        const response = await fetch(`${bootstrap.baseUrl}/api/agentAccounts`, {
          method: 'POST',
          headers: {
            authorization: `Bearer ${bootstrap.authToken}`,
            'content-type': 'application/json',
            'x-gxserver-protocol-version': String(GXSERVER_PROTOCOL_VERSION),
          },
          body: JSON.stringify({ params, protocolVersion: GXSERVER_PROTOCOL_VERSION }),
        });
        const envelope = (await response.json()) as {
          ok: boolean;
          result: AgentAccountsState;
          error?: { message?: string };
        };
        if (!response.ok || !envelope.ok) throw new Error(envelope.error?.message || 'The account request failed.');
        return envelope.result;
      },
    },
  ];
}
