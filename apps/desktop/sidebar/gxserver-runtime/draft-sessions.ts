/*
CDXC:Drafts 2026-08-28:
The sidebar's share of the draft-session lifecycle. gxserver owns the marker,
promotion, and the draft-derived display title; what is left here is the
boot-time heal of the per-keystroke composer cache below.

CDXC:Drafts 2026-08-29 (drafts are durable):
There is deliberately no navigate-away discard: a draft the user created stays
in the sidebar whether or not anything has been typed into it, and leaves only
by being deleted or promoted.
*/
import type { GpuiSidebarRuntime } from './core';
import { postAppModalHostMessage } from '@/packages/core-ui/app-modal-host-bridge';
import { reconcileSessionChatDraftsFromServer } from '@/packages/core-ui/chat/session-chat-draft-storage';
import type { GxserverListSessionChatDraftsResult } from '@/packages/shared/gxserver-protocol';

export interface GpuiSidebarRuntimeDraftSessionMethods {
  reconcileSessionChatDraftCache(): void;
}

/*
CDXC:Drafts 2026-08-28:
Once per sidebar page load, not per (re)connect: the reconcile heals a store
that only decays when the whole app dies, so re-running it on every daemon
reconnect would be pure churn against a cache the composers are actively
writing.
*/
let didReconcileSessionChatDrafts = false;

export const gpuiSidebarRuntimeDraftSessionMethods = {
  /**
   * Heals the shared per-keystroke draft cache from gxserver's durable
   * `session_chat_drafts` copy. The cache lives in Chromium's batched
   * localStorage, so an app kill without a clean shutdown drops its newest
   * writes — text the composer's debounced sync already got to the daemon.
   * Writing the daemon's copy back here makes it reachable again everywhere
   * the cache is read: the Saved Prompts "Recovered" list and the composer's
   * own mount-time restore. The per-key freshness rule lives in
   * `reconcileSessionChatDraftsFromServer`; this method only fetches.
   */
  reconcileSessionChatDraftCache(this: GpuiSidebarRuntime): void {
    if (didReconcileSessionChatDrafts) {
      return;
    }
    const client = this.client;
    if (!client) {
      return;
    }
    didReconcileSessionChatDrafts = true;
    void client
      .rpc<GxserverListSessionChatDraftsResult>('/api/listSessionChatDrafts')
      .then((result) => {
        reconcileSessionChatDraftsFromServer(result.drafts ?? [], '', (event, details) => {
          try {
            postAppModalHostMessage(
              { type: 'sidebarDiagnosticLog', scenarioId: 'gpui.sessionChat.drafts', event, details },
              'Drafts:reconcile'
            );
          } catch {
            // Diagnostic delivery must not interrupt draft recovery.
          }
        });
      })
      .catch(() => {
        // An old daemon without the endpoint, or a transient failure: retry on
        // the next page load rather than surfacing anything.
        didReconcileSessionChatDrafts = false;
      });
  },
};

const gpuiSidebarRuntimeDraftSessionMethodsShapeCheck: GpuiSidebarRuntimeDraftSessionMethods =
  gpuiSidebarRuntimeDraftSessionMethods;
void gpuiSidebarRuntimeDraftSessionMethodsShapeCheck;
