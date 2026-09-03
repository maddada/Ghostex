/*
CDXC:Drafts 2026-08-28:
The web half of the boot-time draft-cache heal (see the desktop's
`reconcileSessionChatDraftCache`): each machine's daemon holds the durable copy
of that machine's composer drafts, and this browser's per-keystroke cache keys
them `<machineId>:<projectId>:<sessionId>`. Once per machine per page load —
the cache only decays when the page's storage does, so re-running on every
reconnect would be churn.
*/
import { reconcileSessionChatDraftsFromServer } from '@/packages/core-ui/chat/session-chat-draft-storage';
import type { GxserverListSessionChatDraftsResult } from '@/packages/shared/gxserver-protocol';
import { rpcForMachine } from '../connections/connection-registry';

const reconciledDraftMachineIds = new Set<string>();

export function reconcileWebSessionChatDraftCache(machineId: string): void {
  if (reconciledDraftMachineIds.has(machineId)) {
    return;
  }
  reconciledDraftMachineIds.add(machineId);
  void rpcForMachine<GxserverListSessionChatDraftsResult>(machineId, '/api/listSessionChatDrafts')
    .then((result) => {
      reconcileSessionChatDraftsFromServer(result.drafts ?? [], `${machineId}:`);
    })
    .catch(() => {
      // An old daemon without the endpoint, or a transient failure: retry on
      // the next page load rather than surfacing anything.
      reconciledDraftMachineIds.delete(machineId);
    });
}
