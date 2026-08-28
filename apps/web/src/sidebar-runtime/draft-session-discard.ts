/*
CDXC:DraftSessions 2026-08-28:
The web app's half of the navigate-away discard for empty draft sessions. It is
the desktop sidebar runtime's `discardEmptyDraftAfterFocusAway` with two
substitutions — the presentation comes from the machine connection states, and
the composer cache is keyed the way the web chat host keys it — so both clients
answer "is this draft empty?" through the same shared predicate and can never
disagree about it.
*/
import {
  readStoredSessionChatDraft,
  reconcileSessionChatDraftsFromServer,
} from '@/packages/core-ui/chat/session-chat-draft-storage';
import { DISCARD_EMPTY_DRAFT_SESSION_REASON, isDiscardableEmptyDraftSession } from '@/packages/shared/draft-sessions';
import type { GxserverListSessionChatDraftsResult } from '@/packages/shared/gxserver-protocol';
import { rpcForMachine } from '../connections/connection-registry';
import type { MachineConnectionState } from '../connections/types';
import type { SidebarSessionReference } from './sidebar-ids';

/*
CDXC:DraftCrashSafety 2026-08-28:
The web half of the boot-time draft-cache heal (see the desktop's
`reconcileSessionChatDraftCache`): each machine's daemon holds the durable copy
of that machine's composer drafts, and this browser's per-keystroke cache keys
them `<machineId>:<projectId>:<sessionId>`. Once per machine per page load —
the cache only decays when the page's storage does, so re-running on every
reconnect would be churn.
*/
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

/**
 * Remove the session the user just navigated AWAY from, if and only if it is
 * still a draft with no text anywhere. Anything else is left untouched.
 *
 * `previous` is whatever was focused before the caller moved focus; `next` is
 * where focus landed, so a re-focus of the same row is a no-op rather than a
 * deletion.
 */
/*
CDXC:DraftSessionsDiscardOwnership 2026-08-28:
The sessions whose chat surface THIS browser session has mounted. A draft is a
real cross-client gxserver row, so an empty composer cache here says nothing
about a draft that was being typed into somewhere else: the per-keystroke cache
is per-client and the synced copy only lands on blur/switch/unmount. Discarding
on an empty cache alone would therefore delete another client's unsynced text.
Vouching first turns "I see no text" into evidence.

In memory, not localStorage, on purpose: a reload restarts the app session and
the browser then simply stops discarding drafts it can no longer vouch for,
which is the safe direction. The desktop app cannot use a set like this — its
composer and its sidebar are different CEF pages — so it vouches through the
shared store instead (`markSessionChatComposerOpened`).
*/
const openedChatSurfaceKeys = new Set<string>();

/** Called when the web workspace mounts a session's chat surface. */
export function recordWebSessionChatSurfaceOpened(sessionKey: string): void {
  openedChatSurfaceKeys.add(sessionKey);
}

export function discardEmptyDraftAfterFocusAway(
  previous: SidebarSessionReference | undefined,
  next: SidebarSessionReference | undefined,
  states: readonly MachineConnectionState[]
): void {
  if (!previous || isSameSession(previous, next)) {
    return;
  }
  /*
  Look the row up in the snapshot rather than trusting the reference: a session
  that has already left the presentation (closed, deleted, or promoted between
  the click and here) must not be removed a second time, and the row is also
  where `isDraft` and `titleSource` come from.
  */
  const session = states
    .find((state) => state.machine.machineId === previous.machineId)
    ?.presentation?.sessions.find(
      (candidate) => candidate.projectId === previous.projectId && candidate.sessionId === previous.sessionId
    );
  if (!session) {
    return;
  }
  /*
  The web chat host keys its per-keystroke composer cache by
  `<machineId>:<projectId>:<sessionId>` (see `app/session-chat-host.tsx`). That
  cache is what covers the gap before the composer's blur/unmount push reaches
  the daemon.
  */
  const sessionKey = `${previous.machineId}:${previous.projectId}:${previous.sessionId}`;
  if (
    !isDiscardableEmptyDraftSession(session, {
      didOpenComposerHere: openedChatSurfaceKeys.has(sessionKey),
      hasLocalComposerText: readStoredSessionChatDraft(sessionKey).trim() !== '',
    })
  ) {
    return;
  }
  /*
  gxserver kills the draft's background provider as part of `/api/removeSession`,
  so there is no client-side kill choreography here. The row disappears with the
  next presentation delta.
  */
  void rpcForMachine(previous.machineId, '/api/removeSession', {
    projectId: previous.projectId,
    reason: DISCARD_EMPTY_DRAFT_SESSION_REASON,
    sessionId: previous.sessionId,
  }).catch(() => undefined);
}

function isSameSession(left: SidebarSessionReference, right: SidebarSessionReference | undefined): boolean {
  return (
    right !== undefined &&
    left.machineId === right.machineId &&
    left.projectId === right.projectId &&
    left.sessionId === right.sessionId
  );
}
