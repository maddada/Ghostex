// Streaming synthetic message (upstream chat spec §6.5 port).
// Show the hook's assistant preview as a synthetic bubble only while it leads
// the transcript (strictly longer AND not a substring of the last assistant
// turn), and only while working. A stale preview from a finished turn never
// shows.

import type { SessionChatMessage } from '../../shared/session-chat';

export const SESSION_CHAT_STREAMING_ID = 'streaming';

function assistantText(message: SessionChatMessage | undefined): string {
  if (!message || message.role !== 'assistant') {
    return '';
  }
  return message.blocks
    .filter((block) => block.type === 'text')
    .map((block) => block.text)
    .join('')
    .trim();
}

export function deriveSessionChatStreamingText(input: {
  messages: readonly SessionChatMessage[];
  previewText: string | null | undefined;
  working: boolean;
}): string | null {
  if (!input.working) {
    return null;
  }
  const text = input.previewText?.trim();
  if (!text) {
    return null;
  }
  const lastText = assistantText(input.messages.at(-1));
  if (lastText.includes(text) || text.length <= lastText.length) {
    return null;
  }
  return text;
}

export function sessionChatStreamingMessage(text: string): SessionChatMessage {
  return {
    blocks: [{ text, type: 'text' }],
    id: SESSION_CHAT_STREAMING_ID,
    role: 'assistant',
    source: 'hook',
    timestamp: null,
  };
}
