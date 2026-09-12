import type { SessionChatAsyncQuestion, SessionChatMessage } from '@/packages/shared/session-chat';

export interface PendingAsyncQuestion extends SessionChatAsyncQuestion {
  key: string;
}

/** Matches Codex's AnsweredQuestion framing, including its 512-byte UTF-8 bound. */
export function sessionChatAsyncAnswerPrefix(title: string): string {
  let bounded = '';
  let bytes = 0;
  const encoder = new TextEncoder();
  for (const character of title) {
    bytes += encoder.encode(character).length;
    if (bytes > 512) break;
    bounded += character;
  }
  return `> ${bounded.replace(/[\r\n]/g, ' ')}\n\n`;
}

/** Only an answer to that question retires it; unrelated messages and tool results do not. */
export function pendingSessionChatAsyncQuestions(messages: readonly SessionChatMessage[]): PendingAsyncQuestion[] {
  const pending: PendingAsyncQuestion[] = [];
  const seen = new Set<string>();
  for (const message of messages) {
    if (message.role === 'assistant') {
      message.asyncQuestions?.forEach((question, index) => {
        const key = `${message.id}:${index}`;
        if (seen.has(key)) return;
        seen.add(key);
        pending.push({ ...question, key });
      });
    } else if (message.role === 'user' && message.source === 'transcript') {
      const text = message.blocks.flatMap((block) => (block.type === 'text' ? [block.text] : [])).join('\n');
      const index = pending.findIndex((question) => text.startsWith(sessionChatAsyncAnswerPrefix(question.title)));
      if (index !== -1) pending.splice(index, 1);
    }
  }
  return pending;
}
