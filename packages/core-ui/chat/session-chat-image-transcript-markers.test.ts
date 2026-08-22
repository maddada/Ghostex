import { describe, expect, it } from 'vitest';
import type { SessionChatMessage } from '../../shared/session-chat';
import { normalizeSessionChatImageTranscriptMessages } from './session-chat-image-transcript-markers';

function userMessage(id: string, text: string): SessionChatMessage {
  return {
    id,
    role: 'user',
    blocks: [{ type: 'text', text }],
    timestamp: null,
    source: 'transcript',
  };
}

describe('normalizeSessionChatImageTranscriptMessages', () => {
  it('merges an image source row with its prompt row', () => {
    expect(
      normalizeSessionChatImageTranscriptMessages([
        userMessage('source', '[Image: source: /tmp/paste.png]'),
        userMessage('prompt', '[Image #1] describe this'),
      ])
    ).toEqual([
      {
        ...userMessage('prompt', 'describe this'),
        blocks: [
          { type: 'image-ref', path: '/tmp/paste.png' },
          { type: 'text', text: 'describe this' },
        ],
      },
    ]);
  });

  it('renders a source-only row as an image attachment', () => {
    expect(
      normalizeSessionChatImageTranscriptMessages([userMessage('source', '[Image: source: /tmp/paste.png]')])[0]?.blocks
    ).toEqual([{ type: 'image-ref', path: '/tmp/paste.png' }]);
  });

  it('does not rewrite client-authored marker-like text', () => {
    const message = {
      ...userMessage('client', '[Image #1] keep this literal'),
      source: 'client' as const,
    };
    expect(normalizeSessionChatImageTranscriptMessages([message])).toEqual([message]);
  });
});
