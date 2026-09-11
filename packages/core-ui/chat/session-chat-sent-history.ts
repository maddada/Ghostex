import type { GxserverStashedPrompt } from '@/packages/shared/gxserver-protocol';
import type { SessionChatDeliveredDraft } from '@/packages/shared/session-chat-queue';
import { SessionChatStorageIndex } from './session-chat-storage-index';

const STORAGE_PREFIX = 'ghostex.sessionChat.sent.';
const CHANGED_EVENT = 'ghostex-session-chat-sent-changed';
const MAX_SENT_MESSAGES = 50;
const sentIndex = new SessionChatStorageIndex<GxserverStashedPrompt>(
  STORAGE_PREFIX,
  (raw) => JSON.parse(raw) as GxserverStashedPrompt,
  () => ''
);

/**
 * CDXC:SavedPrompts 2026-09-08 DECISION:
 * User: retain the last 50 sent messages across all sessions for Up-arrow recall and the Saved prompts Sent tab.
 * Each send owns a storage key so simultaneous composers cannot overwrite each other's history.
 */
export function listSentSessionChatMessages(): GxserverStashedPrompt[] {
  const messages = sentIndex.entries().map(([, message]) => message);
  messages.sort(
    (left, right) => right.createdAt.localeCompare(left.createdAt) || right.promptId.localeCompare(left.promptId)
  );
  for (const message of messages.slice(MAX_SENT_MESSAGES)) {
    sentIndex.remove(`${STORAGE_PREFIX}${message.promptId}`);
  }
  return messages.slice(0, MAX_SENT_MESSAGES);
}

export function recordSentSessionChatMessage(
  text: string,
  sessionKey?: string,
  delivery?: SessionChatDeliveredDraft
): boolean {
  if (text.trim() === '') return false;
  const parts = sessionKey?.split(':') ?? [];
  const timestamp = delivery?.deliveredAt ?? new Date().toISOString();
  const message: GxserverStashedPrompt = {
    promptId: `sent:${delivery?.id ?? crypto.randomUUID()}`,
    content: text,
    createdAt: timestamp,
    updatedAt: timestamp,
    cwd: null,
    projectId: parts.length >= 2 ? parts.at(-2)! : null,
    projectName: null,
    sessionId: parts.at(-1) || null,
  };
  try {
    sentIndex.set(`${STORAGE_PREFIX}${message.promptId}`, message);
    listSentSessionChatMessages();
    window.dispatchEvent(new Event(CHANGED_EVENT));
    return true;
  } catch (error) {
    // Delivery has already succeeded; a history write must not restore and resend the prompt.
    console.error('[session-chat] Could not save sent-message history.', error);
    return false;
  }
}

/** Import each server delivery once, including after closing a composer or deleting a history row. */
export function recordDeliveredSessionChatDrafts(deliveries: readonly SessionChatDeliveredDraft[] = []): void {
  for (const delivery of deliveries) {
    const sessionKey = `${delivery.projectId}:${delivery.sessionId}`;
    const seenKey = `ghostex.sessionChat.delivered.${sessionKey}`;
    try {
      const seen = JSON.parse(window.localStorage.getItem(seenKey) ?? '[]') as string[];
      if (seen.includes(delivery.id)) continue;
      if (recordSentSessionChatMessage(delivery.text, sessionKey, delivery)) {
        window.localStorage.setItem(seenKey, JSON.stringify([...seen, delivery.id].slice(-MAX_SENT_MESSAGES)));
      }
    } catch (error) {
      console.error('[session-chat] Could not import delivered-message history.', error);
    }
  }
}

export function deleteSentSessionChatMessage(promptId: string): void {
  sentIndex.remove(`${STORAGE_PREFIX}${promptId}`);
  window.dispatchEvent(new Event(CHANGED_EVENT));
}

export function subscribeSentSessionChatMessages(onChange: () => void): () => void {
  const onStorage = (event: StorageEvent): void => {
    if (event.key === null || event.key.startsWith(STORAGE_PREFIX)) onChange();
  };
  window.addEventListener('storage', onStorage);
  window.addEventListener(CHANGED_EVENT, onChange);
  return () => {
    window.removeEventListener('storage', onStorage);
    window.removeEventListener(CHANGED_EVENT, onChange);
  };
}
