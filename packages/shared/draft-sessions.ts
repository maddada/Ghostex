/*
CDXC:DraftSessions 2026-08-28:
The ONE place that decides whether a draft session is empty enough to throw
away, shared by the desktop sidebar runtime and the web app so the two clients
can never disagree about it. gxserver owns everything else about drafts — the
marker, the promotion, the display title, the daemon-start sweep — but the
navigate-away discard is necessarily client-side, because only the client knows
that the user just left the session.
*/

import type { GxserverPresentationSession } from './gxserver-protocol';

/**
 * What this client knows about a draft's composer. Both fields are deliberately
 * required: a caller that cannot answer one of them must not be discarding
 * anything.
 */
export type DraftSessionComposerEvidence = {
  /**
   * CDXC:DraftSessionsDiscardOwnership 2026-08-28:
   * This client actually MOUNTED this session's chat composer — the desktop app
   * on this machine, or this browser. Without it, an empty composer cache is the
   * absence of evidence rather than evidence of absence: a draft is a real
   * cross-client gxserver row, so the text may be sitting unsynced in a DIFFERENT
   * client's per-keystroke cache (the synced copy is only pushed on
   * blur/switch/unmount), and this client would have no way to see it. A client
   * may therefore only discard drafts whose composer it hosted itself; every
   * other empty-looking draft is left to the owning client, or to gxserver's
   * daemon-start sweep, which reads the synced copy directly.
   */
  didOpenComposerHere: boolean;
  /**
   * Non-blank text in this client's per-keystroke composer cache
   * (`ghostex.sessionChat.draft.<sessionKey>`). This is the CURRENT value — the
   * synced copy lags it by a blur — which is why it is consulted at all.
   */
  hasLocalComposerText: boolean;
};

/**
 * Whether `session` is a draft this client owns, whose composer is empty
 * everywhere it can see, and which can therefore be removed now that the user
 * has navigated away from it.
 *
 * Three things must ALL hold, and any one of them failing keeps the draft:
 *
 * 1. `didOpenComposerHere` — see {@link DraftSessionComposerEvidence}.
 * 2. `!hasLocalComposerText` — this client's own per-keystroke cache is empty.
 * 3. `titleSource !== 'draft'` — gxserver's own answer. It rewrites a draft's
 *    `displayTitle` from the synced `session_chat_drafts` row and stamps this
 *    source ONLY when that row has non-blank content, so the marker IS the
 *    server-side "this draft has text" signal. It covers text already synced
 *    from another device, which this client's cache knows nothing about.
 *
 * Returns false for anything that is not currently a draft, including the
 * `undefined` you get for a session that has already left the snapshot: a
 * promoted or vanished session must never be swept up by a discard.
 */
export function isDiscardableEmptyDraftSession(
  session: Pick<GxserverPresentationSession, 'isDraft' | 'titleSource'> | undefined,
  evidence: DraftSessionComposerEvidence
): boolean {
  if (session?.isDraft !== true) {
    return false;
  }
  if (!evidence.didOpenComposerHere) {
    return false;
  }
  if (session.titleSource === 'draft') {
    return false;
  }
  return !evidence.hasLocalComposerText;
}

/** The `reason` every navigate-away discard stamps on `/api/removeSession`. */
export const DISCARD_EMPTY_DRAFT_SESSION_REASON = 'discardEmptyDraft';
