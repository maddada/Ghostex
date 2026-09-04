/*
CDXC:SessionChat 2026-09-04 WHY:
Client half of the returned-prompt flow (server: session_chat_returned_prompt.rs).
gxserver carries `returnedPrompt` on reads and frames for a couple of minutes,
and every client that sees it must put the text back into its composer exactly
once: a reload inside that window, a resync, or a second frame must not stack
a second copy above whatever the user typed since. The applied ids are kept in
localStorage rather than in component state because the reload is the case
that matters.

The "Interrupted the agent" row is a client marker created the moment Escape
is pressed, so it shows even when Claude writes no interrupt marker of its own
(a prompt it hands back leaves no trace). When Claude does write one, for a
turn interrupted mid-response, its transcript row supersedes the marker.
*/

import type { SessionChatMessage } from '../../shared/session-chat';
import type { SessionChatCommandMarker } from './session-chat-pending';

const APPLIED_STORAGE_KEY = 'ghostex.sessionChat.returnedPrompts.applied';
const APPLIED_LIMIT = 32;

/** Marker command for a chat-box Escape; rendered through its label. */
export const SESSION_CHAT_INTERRUPT_MARKER_COMMAND = 'interrupt';
export const SESSION_CHAT_INTERRUPT_MARKER_LABEL = 'Interrupted the agent';
/** The transcript row Claude writes for a turn interrupted mid-response. */
const TRANSCRIPT_INTERRUPTED_TEXT = 'conversation interrupted';
/** Clock slack between the client marker and the transcript's own row. */
const INTERRUPT_MARKER_MATCH_SLACK_MS = 15_000;

function appliedStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function readAppliedIds(): string[] {
  const raw = appliedStorage()?.getItem(APPLIED_STORAGE_KEY);
  if (!raw) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((value): value is string => typeof value === 'string') : [];
  } catch {
    return [];
  }
}

export function hasAppliedSessionChatReturnedPrompt(id: string): boolean {
  return readAppliedIds().includes(id);
}

export function markSessionChatReturnedPromptApplied(id: string): void {
  const next = [...readAppliedIds().filter((value) => value !== id), id];
  const bounded = next.length > APPLIED_LIMIT ? next.slice(next.length - APPLIED_LIMIT) : next;
  try {
    appliedStorage()?.setItem(APPLIED_STORAGE_KEY, JSON.stringify(bounded));
  } catch {
    // Private mode / quota: the in-page dedupe still holds for this mount.
  }
}

function messageText(message: SessionChatMessage): string {
  return message.blocks
    .map((block) => (block.type === 'text' ? block.text : ''))
    .join(' ')
    .trim()
    .toLowerCase();
}

/**
 * Drops an interrupt marker once the transcript carries Claude's own interrupt
 * row from the same moment, so a mid-response Stop shows one row, not two.
 */
export function retireSessionChatInterruptMarkers(
  markers: readonly SessionChatCommandMarker[],
  transcript: readonly SessionChatMessage[]
): readonly SessionChatCommandMarker[] {
  if (!markers.some((marker) => marker.command === SESSION_CHAT_INTERRUPT_MARKER_COMMAND)) {
    return markers;
  }
  const interruptedAt = transcript
    .filter(
      (message) =>
        message.role === 'system' &&
        message.source === 'transcript' &&
        messageText(message) === TRANSCRIPT_INTERRUPTED_TEXT &&
        message.timestamp !== null
    )
    .map((message) => message.timestamp as number);
  if (interruptedAt.length === 0) {
    return markers;
  }
  const next = markers.filter(
    (marker) =>
      marker.command !== SESSION_CHAT_INTERRUPT_MARKER_COMMAND ||
      !interruptedAt.some((at) => at >= marker.sentAt - INTERRUPT_MARKER_MATCH_SLACK_MS)
  );
  return next.length === markers.length ? markers : next;
}
