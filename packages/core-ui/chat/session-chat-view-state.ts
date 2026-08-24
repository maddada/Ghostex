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
  if (input.status === 'working' && input.hasKnownAgentSession) {
    // A KNOWN session working with nothing to show = transcript not flushed
    // yet; hold loading rather than flashing empty. Status stays 'working',
    // so the composer keeps Stop the moment a bubble lands.
    return { kind: 'loading' };
  }
  // Empty wins over a transient 'working' so a just-toggled pre-session pane
  // shows a clear empty state instead of a spinner over nothing.
  return { kind: 'empty' };
}
