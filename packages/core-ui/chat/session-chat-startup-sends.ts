import type { SessionChatQueuedPrompt } from '../../shared/session-chat';
import {
  normalizeSessionChatPendingText,
  type SessionChatPendingSend,
} from './session-chat-pending';

/** Reconstruct accepted sends on another client and reconcile the receipt with the immediate local echo. */
export function sessionChatPendingWithStartupSends(
  pending: readonly SessionChatPendingSend[],
  queue: readonly SessionChatQueuedPrompt[]
): SessionChatPendingSend[] {
  const queueIds = new Set(queue.map((prompt) => prompt.id));
  const entries = pending.filter((entry) =>
    entry.startupDelivery?.state !== 'failed' || queueIds.has(entry.startupDelivery.promptId)
  );
  const matched = new Set<string>();
  for (const prompt of queue) {
    const byReceipt = entries.find((entry) => entry.queuedPromptId === prompt.id);
    if (!prompt.startupSend && !byReceipt) continue;
    // A queue broadcast can beat the send response. Pair identical local sends
    // one at a time until their receipts arrive; persisted identity is the row id.
    const entry = byReceipt ?? entries.find((entry) =>
      !entry.queuedPromptId && !entry.imagePaths?.length && !matched.has(entry.id) &&
      normalizeSessionChatPendingText(entry.text) === normalizeSessionChatPendingText(prompt.text)
    );
    const delivery: NonNullable<SessionChatPendingSend['startupDelivery']> = {
      promptId: prompt.id,
      state: prompt.state,
      ...(prompt.errorMessage ? { errorMessage: prompt.errorMessage } : {}),
    };
    if (entry) {
      matched.add(entry.id);
      if (entry.queuedPromptId !== prompt.id || entry.startupDelivery?.state !== delivery.state ||
        entry.startupDelivery?.errorMessage !== delivery.errorMessage) {
        entries[entries.indexOf(entry)] = { ...entry, queuedPromptId: prompt.id, startupDelivery: delivery };
      }
    } else {
      entries.push({
        id: `startup:${prompt.id}`,
        queuedPromptId: prompt.id,
        startupDelivery: delivery,
        text: prompt.text,
        sentAt: Date.parse(prompt.createdAt),
        afterMessageId: null,
      });
    }
  }
  return entries;
}

/** Preserve hidden startup sends in their slots when the visible queue is reordered. */
export function sessionChatFullQueueOrder(queue: readonly SessionChatQueuedPrompt[], visibleIds: readonly string[]): string[] {
  const requested = [...new Set(visibleIds)].filter((id) => queue.some((prompt) => prompt.id === id));
  const included = new Set(requested);
  let index = 0;
  return queue.map((prompt) => included.has(prompt.id) ? requested[index++]! : prompt.id);
}
