/*
CDXC:DraftSessions 2026-08-28:
Navigate-away discard for empty draft sessions. gxserver owns every other part
of the draft lifecycle (the marker, promotion, the draft-derived display title,
the daemon-start sweep), but this one has to live in the client: only the client
knows that the user just left the session, and only the client can see the
per-keystroke composer cache that the synced copy has not caught up with yet.
*/
import type { GpuiSidebarRuntime } from './core';
import { normalizeNonEmptyString } from './helpers/records';
import { parseGpuiRemotePresentationSessionId } from './helpers/remote-presentation';
import {
  hasSessionChatComposerOpened,
  readStoredSessionChatDraft,
  reconcileSessionChatDraftsFromServer,
} from '@/packages/core-ui/chat/session-chat-draft-storage';
import { DISCARD_EMPTY_DRAFT_SESSION_REASON, isDiscardableEmptyDraftSession } from '@/packages/shared/draft-sessions';
import type {
  GxserverListSessionChatDraftsResult,
  GxserverRemoveSessionResult,
} from '@/packages/shared/gxserver-protocol';

export interface GpuiSidebarRuntimeDraftSessionMethods {
  discardEmptyDraftAfterFocusAway(previousFocusedSessionId: string | undefined): void;
  reconcileSessionChatDraftCache(): void;
}

/*
CDXC:DraftCrashSafety 2026-08-28:
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
        reconcileSessionChatDraftsFromServer(result.drafts ?? []);
      })
      .catch(() => {
        // An old daemon without the endpoint, or a transient failure: retry on
        // the next page load rather than surfacing anything.
        didReconcileSessionChatDrafts = false;
      });
  },

  /**
   * Called by the two focus writers with whatever session was focused BEFORE
   * they moved focus. If that session was a draft with no text anywhere, it is
   * removed; anything else is left completely alone.
   */
  discardEmptyDraftAfterFocusAway(this: GpuiSidebarRuntime, previousFocusedSessionId: string | undefined): void {
    const previous = normalizeNonEmptyString(previousFocusedSessionId);
    if (!previous || previous === this.focusedSessionId) {
      return;
    }
    /*
    Remote drafts are deliberately not discarded from here. The desktop chat page
    that owns a session's composer cache is the LOCAL one, so this client cannot
    see a remote draft's unsent text and would be guessing. The owning machine's
    own daemon-start sweep is what collects those.
    */
    if (parseGpuiRemotePresentationSessionId(previous)) {
      return;
    }
    const client = this.client;
    const presentation = this.presentation;
    if (!client || !presentation) {
      return;
    }
    /*
    Look the row up in the snapshot rather than trusting the id alone: a session
    that has already left the presentation (closed, deleted, or promoted between
    the click and here) must not be removed a second time, and the row is also
    where `isDraft` and `titleSource` come from.
    */
    const session = presentation.sessions.find((candidate) => candidate.sessionId === previous);
    if (!session) {
      return;
    }
    /*
    The desktop chat page keys its per-keystroke cache by `<projectId>:<sessionId>`
    (see `chat-main.tsx`), and the bundled pages share one localStorage — the
    Saved Prompts "Recovered" list in the modal host reads the very same entries.
    */
    const sessionKey = `${session.projectId}:${session.sessionId}`;
    if (
      !isDiscardableEmptyDraftSession(session, {
        /*
        CDXC:DraftSessionsDiscardOwnership 2026-08-28:
        Only drafts whose composer THIS installation has hosted may be
        discarded. Persistence across restarts is covered — the mark and the
        draft cache live in the same shared CEF store and expire together — but
        a local draft may also have been opened in the web app or on mobile
        against this same daemon, where unsynced text would be invisible here.
        Those are left to the owning client, or to gxserver's daemon-start
        sweep, which reads the synced copy directly.
        */
        didOpenComposerHere: hasSessionChatComposerOpened(sessionKey),
        hasLocalComposerText: readStoredSessionChatDraft(sessionKey).trim() !== '',
      })
    ) {
      return;
    }
    /*
    Drop the row locally first, exactly like the close transition does, so the
    sidebar does not keep painting a session that is on its way out. That call
    also records the id in the local-first hidden set and republishes, so a
    hydrate still in flight cannot reinsert the discarded draft. gxserver kills
    the draft's background provider as part of `/api/removeSession`.
    */
    const { projectId, sessionId } = session;
    this.removePresentationSession(projectId, sessionId);
    void client
      .rpc<GxserverRemoveSessionResult>('/api/removeSession', {
        projectId,
        reason: DISCARD_EMPTY_DRAFT_SESSION_REASON,
        sessionId,
      })
      .then((result) => {
        /*
        A discard is a REQUEST: the client decided from a snapshot that may be a
        delta stale, so gxserver re-derives the predicate and DECLINES when the
        session has since been promoted or gained draft text. A decline arrives
        as a SUCCESS with `removed: false` — which is why this is a `.then` and
        not something the `.catch` below could ever see — and the optimistic
        hide above has to be undone or a live session stays invisible here.

        `removed` is published only for discard-reason removals, so the check is
        for the explicit `false`: an absent key is an older daemon that removed
        the row unconditionally, exactly as before this contract existed.
        */
        if (result.removed !== false) {
          return;
        }
        this.unhideLocalPresentationSession(projectId, sessionId);
        /*
        Re-read rather than reinserting the returned row: `session` on the
        result is the DURABLE domain row, and turning one of those into a
        presentation row is gxserver's job, not the sidebar's. The snapshot
        refresh brings the row back already projected, with whatever draft text
        or promotion caused the decline in the first place.
        */
        return this.refreshDomainPresentationSnapshotFromClient('patch');
      })
      .catch(() => undefined);
  },
};

const gpuiSidebarRuntimeDraftSessionMethodsShapeCheck: GpuiSidebarRuntimeDraftSessionMethods =
  gpuiSidebarRuntimeDraftSessionMethods;
void gpuiSidebarRuntimeDraftSessionMethodsShapeCheck;
