import { postAppModalHostMessage } from '../app-modal-host-bridge';
import { GXSERVER_PROTOCOL_VERSION } from '@/packages/shared/gxserver-protocol';
import { createAppToastRequest } from '@/packages/shared/app-toast-contract';
import type { AccountsTransport, AgentAccountsState } from '@/packages/shared/agent-accounts';
export interface AccountsConnection {
  id: string;
  label: string;
  request: AccountsTransport;
}
let connectionSource: (() => AccountsConnection[]) | undefined;
const connectionListeners = new Set<() => void>();
let connectionRevision = 0;
export function notifyAccountsConnectionsChanged(): void {
  connectionRevision++;
  connectionListeners.forEach((listener) => listener());
}
export function subscribeAccountsConnections(listener: () => void): () => void {
  connectionListeners.add(listener);
  return () => {
    connectionListeners.delete(listener);
  };
}
export function getAccountsConnectionRevision(): number {
  return connectionRevision;
}
export function setAccountsConnectionSource(source: () => AccountsConnection[]) {
  connectionSource = source;
  notifyAccountsConnectionsChanged();
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

export function showAccountFlowToast(title: string, description: string): void {
  postAppModalHostMessage(
    createAppToastRequest('info', title, description, {
      durationMs: 12000,
      toastId: 'account-connection-flow',
    }),
    'Accounts:connectionFlow'
  );
}
