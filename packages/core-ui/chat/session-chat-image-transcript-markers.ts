import type { SessionChatBlock, SessionChatMessage, SessionChatTextBlock } from '../../shared/session-chat';

const IMAGE_SOURCE_MARKER = /^\[Image:\s*source:\s*(.+?)\]\s*$/;
const IMAGE_PROMPT_MARKER = /^\[Image #\d+\](?:\s+|$)/;

function isTextBlock(block: SessionChatBlock): block is SessionChatTextBlock {
  return block.type === 'text';
}

function soleText(message: SessionChatMessage): string | null {
  return message.blocks.length === 1 && isTextBlock(message.blocks[0]) ? message.blocks[0].text : null;
}

export function imageSourcePathFromText(text: string): string | null {
  return text.match(IMAGE_SOURCE_MARKER)?.[1]?.trim() ?? null;
}

export function stripImagePromptMarker(text: string): string {
  return text.replace(IMAGE_PROMPT_MARKER, '');
}

function stripFirstImagePromptMarker(blocks: readonly SessionChatBlock[]): SessionChatBlock[] {
  let stripped = false;
  const next: SessionChatBlock[] = [];
  for (const block of blocks) {
    if (!stripped && isTextBlock(block)) {
      stripped = true;
      const text = stripImagePromptMarker(block.text);
      if (text.trim().length > 0) {
        next.push({ ...block, text });
      }
      continue;
    }
    next.push(block);
  }
  return next;
}

function imagePromptMarkerStartsMessage(message: SessionChatMessage): boolean {
  const firstText = message.blocks.find(isTextBlock);
  return firstText ? IMAGE_PROMPT_MARKER.test(firstText.text) : false;
}

export function normalizeSessionChatImageTranscriptMessages(
  messages: readonly SessionChatMessage[]
): SessionChatMessage[] {
  const normalized: SessionChatMessage[] = [];
  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index]!;
    if (message.role !== 'user' || message.source !== 'transcript') {
      normalized.push(message);
      continue;
    }

    const imagePath = imageSourcePathFromText(soleText(message) ?? '');
    const next = messages[index + 1];
    if (imagePath && next?.role === 'user' && next.source === message.source && imagePromptMarkerStartsMessage(next)) {
      normalized.push({
        ...next,
        blocks: [{ type: 'image-ref', path: imagePath }, ...stripFirstImagePromptMarker(next.blocks)],
      });
      index += 1;
      continue;
    }

    if (imagePath) {
      normalized.push({
        ...message,
        blocks: [{ type: 'image-ref', path: imagePath }],
      });
      continue;
    }

    normalized.push({
      ...message,
      blocks: stripFirstImagePromptMarker(message.blocks),
    });
  }
  return normalized;
}
