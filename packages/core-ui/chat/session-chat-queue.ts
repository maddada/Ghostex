// Client logic for Ghostex's prompt queue and the synced composer draft
// (plan 016 §4). Pure functions plus the per-client id: the row strip
// (session-chat-queue-rows.tsx) and the composer stay presentation, this file
// stays readable and independently reasoned about.
//
// NAMING COLLISION, READ THIS TWICE: `SessionChatMessage.queued` is the AGENT
// CLI's own internal queue and renders inside the transcript. Nothing here
// touches it. These rows are prompts the agent has never seen.

import type { SessionChatDraft, SessionChatQueuedPrompt } from '../../shared/session-chat';
import { PointerActivationConstraints } from '@dnd-kit/dom';

/**
 * How many rows the strip shows before it scrolls. The composer must stay the
 * dominant thing in the footer, so a long queue scrolls rather than pushing
 * the input off the pane.
 */
export const SESSION_CHAT_QUEUE_VISIBLE_ROWS = 5;

/** Long-press duration that turns a Send tap into a queue (plan 016 §1). */
export const SESSION_CHAT_QUEUE_LONG_PRESS_MS = 500;

/**
 * A row is one line. Show the first line that has any content: a prompt that
 * opens with a blank line, a heading, or a fenced block would otherwise render
 * an empty row and look broken.
 */
export function sessionChatQueueRowPreview(text: string): string {
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (trimmed !== '') {
      return trimmed;
    }
  }
  return text.trim();
}

/** Row ids, head first — the exact shape /api/reorderSessionChatQueue wants. */
export function sessionChatQueuePromptIds(queue: readonly SessionChatQueuedPrompt[]): string[] {
  return queue.map((prompt) => prompt.id);
}

/**
 * Optimistic reorder: the strip shows the new order on drop, and the
 * authoritative queue from the mutation's answer replaces it a moment later.
 * Out-of-range indexes return the list untouched rather than throwing, because
 * a drop can resolve against a row that a state frame just removed.
 */
export function moveSessionChatQueueRow(
  queue: readonly SessionChatQueuedPrompt[],
  fromIndex: number,
  toIndex: number
): SessionChatQueuedPrompt[] {
  const next = [...queue];
  if (fromIndex === toIndex || fromIndex < 0 || fromIndex >= next.length || toIndex < 0 || toIndex >= next.length) {
    return next;
  }
  const [moved] = next.splice(fromIndex, 1);
  if (moved === undefined) {
    return next;
  }
  next.splice(toIndex, 0, moved);
  return next;
}

/**
 * A `sending` row is being delivered by the server scheduler right now. Editing,
 * reordering or deleting it would race the send, so every row control is
 * refused on it (see SessionChatQueuedPromptState).
 */
export function isSessionChatQueueRowBusy(prompt: SessionChatQueuedPrompt): boolean {
  return prompt.state === 'sending';
}

/**
 * Both capability gates in one place (see the transport's queue section):
 * the daemon must have reported a `queue` array AND this host's transport must
 * implement the method. Anything false hides that control outright instead of
 * offering a button that 404s or silently does nothing.
 */
export interface SessionChatQueueCapabilities {
  /** The daemon answered with a queue at all. False hides the whole strip. */
  supported: boolean;
  canQueue: boolean;
  canEdit: boolean;
  canRemove: boolean;
  canReorder: boolean;
  canSendNow: boolean;
  canRetry: boolean;
  canSyncDraft: boolean;
}

export function sessionChatQueueCapabilities(params: {
  /** True once a read/frame carried a `queue` field (even an empty array). */
  daemonSupportsQueue: boolean;
  transport: {
    queuePrompt?: unknown;
    updateQueuedPrompt?: unknown;
    removeQueuedPrompt?: unknown;
    reorderQueue?: unknown;
    sendQueuedPrompt?: unknown;
    setDraft?: unknown;
  };
}): SessionChatQueueCapabilities {
  const { daemonSupportsQueue, transport } = params;
  const gate = (method: unknown): boolean => daemonSupportsQueue && typeof method === 'function';
  return {
    // Editing a row is a remove + a re-queue, so it needs both endpoints.
    canEdit: gate(transport.removeQueuedPrompt) && gate(transport.queuePrompt),
    canQueue: gate(transport.queuePrompt),
    canRemove: gate(transport.removeQueuedPrompt),
    canReorder: gate(transport.reorderQueue),
    canRetry: gate(transport.updateQueuedPrompt),
    canSendNow: gate(transport.sendQueuedPrompt),
    // Draft sync is independent of the queue: a daemon can carry drafts while
    // this host has no queue endpoints, and neither hides anything.
    canSyncDraft: typeof transport.setDraft === 'function',
    supported: daemonSupportsQueue,
  };
}

// ---------------------------------------------------------------------------
// Draft sync
// ---------------------------------------------------------------------------

const SESSION_CHAT_CLIENT_ID_STORAGE_KEY = 'ghostex.sessionChat.clientId';

function draftClientIdStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/**
 * This client's opaque draft-origin id. Persisted so a reload keeps the same
 * identity: a fresh id every mount would make the client's own last push look
 * like another device and pop the conflict bar against itself.
 */
export function sessionChatDraftClientId(): string {
  const storage = draftClientIdStorage();
  const stored = storage?.getItem(SESSION_CHAT_CLIENT_ID_STORAGE_KEY);
  if (stored) {
    return stored;
  }
  const created = `gx-${Math.random().toString(36).slice(2)}${Date.now().toString(36)}`;
  try {
    storage?.setItem(SESSION_CHAT_CLIENT_ID_STORAGE_KEY, created);
  } catch {
    // Private mode / quota: an in-memory id still filters this mount's echo.
  }
  return created;
}

/** ISO-8601 millis ordering. Unparseable input never counts as newer. */
export function isNewerSessionChatDraftStamp(candidate: string, reference: string | null): boolean {
  const at = Date.parse(candidate);
  if (Number.isNaN(at)) {
    return false;
  }
  if (reference === null) {
    return true;
  }
  const referenceAt = Date.parse(reference);
  return Number.isNaN(referenceAt) ? true : at > referenceAt;
}

/**
 * The "Newer draft from another device" rule, all three conditions together.
 * Deliberately conservative:
 *
 * - a draft this client wrote is its own echo and never offered;
 * - identical content is nothing to offer;
 * - an EMPTY incoming draft is never offered, because "Use" on it would clear
 *   the local composer — the exact clobber the whole bar exists to prevent.
 *   Another device clearing its draft simply stops proposing.
 *
 * The caller never applies the result on its own: the bar is shown and the
 * user presses Use.
 */
export function shouldOfferSessionChatDraft(params: {
  incoming: SessionChatDraft | null;
  clientId: string;
  /** Newest stamp already applied or dismissed here; null before the first. */
  lastHandledUpdatedAt: string | null;
  /** Live composer text, so an identical draft never nags. */
  composerText: string;
}): boolean {
  const { clientId, composerText, incoming, lastHandledUpdatedAt } = params;
  if (!incoming || incoming.originClientId === clientId) {
    return false;
  }
  if (incoming.content === composerText || incoming.content.trim() === '') {
    return false;
  }
  return isNewerSessionChatDraftStamp(incoming.updatedAt, lastHandledUpdatedAt);
}

/**
 * The crash-recovery rule: whether this client's OWN synced draft should be
 * put straight back into the composer. The offer bar above deliberately never
 * fires for own-client drafts (they are echoes), which meant a draft that
 * survived only in gxserver — because the app was killed before Chromium
 * committed the per-keystroke localStorage batch — was unreachable. This
 * predicate closes that hole, and stays conservative:
 *
 * - only own-client drafts: another device's text keeps the explicit Use bar;
 * - only a virgin composer: any edit, load, send, or clear since mount means
 *   the user is working here, and nothing may replace their text silently;
 * - only when the synced copy is demonstrably fresher than the stored one —
 *   the stored draft is missing/blank, or carries an older stamp. A legacy
 *   stored value without a stamp is "age unknown" and is never overridden.
 */
/*
CDXC:Drafts 2026-09-05 WHY:
Age alone cannot prove a draft was sent: another client or the terminal can send a different prompt while this composer holds unsent text.
A debounced save can contain only the beginning of the submitted message, so compare prefixes as well as complete text.
Only an untouched restored copy saved no later than that matching message may be retired from transcript evidence; newer drafts remain unsent.
*/
export function isSessionChatDraftStale(
  draftUpdatedAt: number | string | null | undefined,
  lastSentPromptAt: number | null | undefined,
  draftText: string,
  lastSentPromptText: string | null | undefined
): boolean {
  if (!draftText.trim() || !lastSentPromptText?.trim().startsWith(draftText.trim())) {
    return false;
  }
  if (lastSentPromptAt === null || lastSentPromptAt === undefined) {
    return false;
  }
  const at = typeof draftUpdatedAt === 'string' ? Date.parse(draftUpdatedAt) : draftUpdatedAt;
  if (at === null || at === undefined || Number.isNaN(at)) {
    return false;
  }
  return at <= lastSentPromptAt;
}

export function shouldRestoreOwnSessionChatDraft(params: {
  incoming: SessionChatDraft | null;
  clientId: string;
  /** True once anything mutated the composer since mount. */
  composerTouched: boolean;
  /** Live composer text, so an identical draft never reloads. */
  composerText: string;
  /** This client's stored draft entry, null when absent. */
  stored: { text: string; updatedAt: number | undefined } | null;
  /** Epoch ms of the transcript's newest user prompt; null while unknown. */
  lastSentPromptAt?: number | null;
  lastSentPromptText?: string | null;
}): boolean {
  const { clientId, composerText, composerTouched, incoming, lastSentPromptAt, lastSentPromptText, stored } = params;
  if (!incoming || incoming.originClientId !== clientId || composerTouched) {
    return false;
  }
  if (incoming.content.trim() === '' || incoming.content === composerText) {
    return false;
  }
  if (isSessionChatDraftStale(incoming.updatedAt, lastSentPromptAt, incoming.content, lastSentPromptText)) {
    return false;
  }
  if (stored === null) {
    return true;
  }
  if (stored.updatedAt === undefined) {
    // Legacy plain-string value: age unknown, so only a BLANK one may lose.
    return stored.text.trim() === '';
  }
  // One rule, shared with the reconcile in session-chat-draft-storage: the
  // synced copy wins only with a strictly newer stamp. A stamped blank entry
  // is a deliberate tombstone (the Recovered list's delete) and refuses older
  // server text exactly like real text would.
  const incomingAt = Date.parse(incoming.updatedAt);
  return !Number.isNaN(incomingAt) && incomingAt > stored.updatedAt;
}

// ---------------------------------------------------------------------------
// Drag-to-reorder activation
// ---------------------------------------------------------------------------

const QUEUE_ROW_DRAG_DISTANCE_PX = 6;
const QUEUE_ROW_DRAG_DELAY_MS = 200;
const QUEUE_ROW_DRAG_DELAY_TOLERANCE_PX = 12;

/**
 * Distance OR Delay, on every pointer type including touch.
 *
 * The sidebar's shared constraints (sidebar-reorder-activation.ts) are
 * hold-only under touch on purpose: a session card IS the scroll surface
 * there, so a distance activation would eat the scroll gesture. A queue row's
 * grab target is a dedicated handle that does nothing else, and the handle
 * carries `touch-action: none`, so distance is safe here and a decisive flick
 * activates instead of being dropped — the recorded dnd-kit failure mode where
 * Delay alone silently cancelled fast drags.
 */
export function getSessionChatQueueDragActivationConstraints() {
  return [
    new PointerActivationConstraints.Delay({
      tolerance: QUEUE_ROW_DRAG_DELAY_TOLERANCE_PX,
      value: QUEUE_ROW_DRAG_DELAY_MS,
    }),
    new PointerActivationConstraints.Distance({
      value: QUEUE_ROW_DRAG_DISTANCE_PX,
    }),
  ];
}
