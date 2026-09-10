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

/**
 * CDXC:SessionChat 2026-09-10 DECISION:
 * User: separate transcript readiness from agent activity; draft saves, model updates, and working/idle changes must not turn an initialized chat back into loading.
 * Draft autosaves and activity updates carry ready/working without reading the transcript.
 * Letting those frames replace starting made G42qx and G43qg remove an input the user was already typing into.
 * Only a transcript read, snapshot, or appended messages can establish transcript readiness; loading belongs to initial session setup.
 */
export function sessionChatTranscriptStatusAfterState(
  current: SessionChatStatus,
  incoming: SessionChatStatus
): SessionChatStatus {
  return incoming === 'working' || incoming === 'ready' || incoming === 'loading' ? current : incoming;
}

/**
 * CDXC:SessionChat 2026-09-10 WHY:
 * A Codex session ID can exist before any transcript, including for an empty session launched from Terminal without Ghostex's draft marker.
 * Working with zero messages is an empty conversation, not a new loading phase that may unmount its composer.
 * This supersedes the draft-only exception to the working-without-messages loading hold.
 */
export function selectSessionChatViewState(input: {
  status: SessionChatStatus;
  messageCount: number;
  error?: string | null;
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
  // Empty wins over a transient 'working' so a just-toggled pre-session pane
  // shows a clear empty state instead of a spinner over nothing.
  return { kind: 'empty' };
}
