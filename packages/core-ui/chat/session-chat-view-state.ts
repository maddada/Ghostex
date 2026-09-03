// View-state dispatch (upstream chat spec §10.6 port, extended with the gxserver-only
// "starting" and "unsupported" statuses).

import type { SessionChatStatus } from '../../shared/session-chat';

export type SessionChatViewState =
  | { kind: 'error'; message: string }
  | { kind: 'ready'; isWorking: boolean }
  | { kind: 'loading' }
  | { kind: 'starting' }
  | { kind: 'unsupported' }
  | { kind: 'empty' };

export function selectSessionChatViewState(input: {
  status: SessionChatStatus;
  messageCount: number;
  error?: string | null;
  /** Provider conversation id is known (agentSessionId resolved). */
  hasKnownAgentSession: boolean;
  /*
  CDXC:Drafts 2026-08-28:
  The session is a draft: a real row whose agent CLI is running but which has
  never been given a prompt. A draft has no transcript to protect, so it never
  takes the blank "hold loading" below — see the branch for why that matters.
  */
  isDraft?: boolean;
}): SessionChatViewState {
  if (input.status === 'error') {
    return {
      kind: 'error',
      message: input.error ?? 'Conversation could not be loaded.',
    };
  }
  if (input.messageCount > 0) {
    return { isWorking: input.status === 'working', kind: 'ready' };
  }
  if (input.status === 'unsupported') {
    return { kind: 'unsupported' };
  }
  if (input.status === 'starting') {
    return { kind: 'starting' };
  }
  if (input.status === 'loading') {
    return { kind: 'loading' };
  }
  if (input.status === 'working' && input.hasKnownAgentSession && input.isDraft !== true) {
    // A KNOWN session working with nothing to show = transcript not flushed
    // yet; hold loading rather than flashing empty. Status stays 'working',
    // so the composer keeps Stop the moment a bubble lands.
    /*
    CDXC:Drafts 2026-08-28:
    Never for a draft. 'loading' is the one kind the view answers with an early
    return that unmounts the whole pane body, composer included — and a draft
    hits this branch on exactly the wrong edge: switching its agent CLI makes
    the new CLI report work while the client still holds the OLD
    agentSessionId, for as long as it takes the post-switch read to clear it.
    A draft has no transcript that could be "not flushed yet", so the honest
    answer is the same empty/welcome state a freshly opened draft shows, and
    the composer keyed by sessionKey stays mounted through the swap.
    */
    return { kind: 'loading' };
  }
  // Empty wins over a transient 'working' so a just-toggled pre-session pane
  // shows a clear empty state instead of a spinner over nothing.
  return { kind: 'empty' };
}
