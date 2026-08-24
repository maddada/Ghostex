// Working-status merge (upstream chat spec §10.4 port, simplified hook inputs).
// The server already derives a base status from the transcript lifecycle; the
// client re-checks the lifecycle so a terminal boundary can settle a stale
// "working" signal without waiting for the next frame, and so an external
// live signal (host hook status) can be merged in.

import type { SessionChatMessage, SessionChatStatus, SessionChatTurnLifecycle } from '../../shared/session-chat';

export const SESSION_CHAT_LIFECYCLE_CLOCK_SKEW_SLACK_MS = 2_000;

export function lifecycleTerminatesCurrentTurn(
  lifecycle: SessionChatTurnLifecycle | null | undefined,
  workingStartedAt: number | null
): boolean {
  if (lifecycle?.state !== 'completed' && lifecycle?.state !== 'interrupted') {
    return false;
  }
  if (workingStartedAt == null || lifecycle.timestamp == null) {
    // Omit/null timestamps are valid on the wire; prefer the terminal marker
    // over a stuck spinner (lifecycle is last-wins, so a newer user
    // generation would have replaced it with 'working').
    return true;
  }
  if (lifecycle.timestamp >= workingStartedAt) {
    return true;
  }
  if (lifecycle.timestamp > 1e11 && workingStartedAt > 1e11) {
    // Clock skew slack, REAL epochs only; the 1e11 gate keeps small logical
    // clocks (tests) strictly ordered.
    return lifecycle.timestamp + SESSION_CHAT_LIFECYCLE_CLOCK_SKEW_SLACK_MS >= workingStartedAt;
  }
  return false;
}

export function trailingAssistantPostDates(
  transcriptMessages: readonly SessionChatMessage[],
  workingStartedAt: number | null
): boolean {
  if (workingStartedAt == null) {
    return false;
  }
  const last = transcriptMessages.at(-1);
  return last?.role === 'assistant' && last.timestamp != null && last.timestamp >= workingStartedAt;
}

export interface SessionChatWorkingInput {
  /** Simplified live signal: server status === "working" or a host hook. */
  working: boolean;
  lifecycle: SessionChatTurnLifecycle | null;
  /** Optional: when the current working state began (hook stateStartedAt). */
  workingStartedAt?: number | null;
  /** Optional: a background child can veto settle while it runs. */
  hasWorkingSubagents?: boolean;
  /** Transcript messages, for trailing-assistant prose recovery. */
  transcriptMessages?: readonly SessionChatMessage[];
}

/** Port of liveStatusOverride: returns "working" or undefined (no override). */
export function deriveSessionChatWorkingOverride(input: SessionChatWorkingInput): 'working' | undefined {
  if (!input.working) {
    return undefined;
  }
  const workingStartedAt = input.workingStartedAt ?? null;
  const terminates = lifecycleTerminatesCurrentTurn(input.lifecycle, workingStartedAt);
  if (terminates && input.lifecycle?.state === 'interrupted') {
    // An explicit interruption ends the WHOLE turn, children included.
    return undefined;
  }
  if (input.hasWorkingSubagents === true) {
    return 'working';
  }
  if (terminates) {
    return undefined;
  }
  if (
    input.lifecycle?.state !== 'working' &&
    trailingAssistantPostDates(input.transcriptMessages ?? [], workingStartedAt)
  ) {
    // Prose recovery: available whenever the latest lifecycle is NOT an
    // explicit in-progress generation. Mid-turn (lifecycle 'working') keeps
    // prose off so partial assistant rows don't settle early.
    return undefined;
  }
  return 'working';
}

export interface SessionChatStatusMergeInput {
  serverStatus: SessionChatStatus;
  loading: boolean;
  error: string | null;
  workingOverride: 'working' | undefined;
}

/** Port of mergeNativeChatLiveSession's status arm. */
export function mergeSessionChatStatus(input: SessionChatStatusMergeInput): SessionChatStatus {
  if (input.error) {
    return 'error';
  }
  // Live work WINS over loading: 'working' drives Stop-vs-Send, the typing
  // indicator, and the streaming preview.
  if (input.loading && input.workingOverride !== 'working') {
    return 'loading';
  }
  return input.workingOverride ?? input.serverStatus;
}

// --- Local Stop suppression (§10.5, simplified) ------------------------------

export function shouldShowSessionChatWorking(input: { working: boolean; interrupted: boolean }): boolean {
  return input.working && !input.interrupted;
}

export function shouldClearSessionChatWorkingSuppression(input: { working: boolean }): boolean {
  return !input.working;
}
