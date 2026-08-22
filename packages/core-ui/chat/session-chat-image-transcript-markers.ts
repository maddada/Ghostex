import type {
  SessionChatBlock,
  SessionChatImageRefBlock,
  SessionChatMessage,
  SessionChatTextBlock,
} from '../../shared/session-chat';

const IMAGE_SOURCE_MARKER = /^\[Image:\s*source:\s*(.+?)\]\s*$/;
const IMAGE_PROMPT_MARKER = /^\[Image #\d+\]\s*/;
const LINKED_IMAGE_MARKER = /\[Image #\d+\]\(([^)\r\n]+)\)/g;

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

function linkedImageBlock(href: string): SessionChatImageRefBlock {
  if (/^(?:https?:|data:)/i.test(href)) {
    return { type: 'image-ref', url: href };
  }
  try {
    return { type: 'image-ref', path: decodeURI(href) };
  } catch {
    return { type: 'image-ref', path: href };
  }
}

function extractLinkedImageMarkers(message: SessionChatMessage): SessionChatMessage | null {
  const images: SessionChatImageRefBlock[] = [];
  const seen = new Set<string>();
  const blocks: SessionChatBlock[] = [];
  let changed = false;

  for (const block of message.blocks) {
    if (!isTextBlock(block)) {
      blocks.push(block);
      continue;
    }
    const text = block.text.replace(LINKED_IMAGE_MARKER, (_marker, rawHref: string) => {
      const href = rawHref.trim();
      const image = linkedImageBlock(href);
      const key = image.url ?? image.path ?? href;
      if (!seen.has(key)) {
        seen.add(key);
        images.push(image);
      }
      changed = true;
      return '';
    });
    if (text.trim()) {
      blocks.push({ ...block, text: text.trim() });
    }
  }

  return changed ? { ...message, blocks: [...images, ...blocks] } : null;
}

export function normalizeSessionChatImageTranscriptMessages(
  messages: readonly SessionChatMessage[]
): SessionChatMessage[] {
  const normalized: SessionChatMessage[] = [];
  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index]!;
    if (message.role !== 'user' || message.source !== 'transcript') {
      normalized.push(
        message.role === 'user' ? extractLinkedImageMarkers(message) ?? message : message
      );
      continue;
    }

    const linkedImageMessage = extractLinkedImageMarkers(message);
    if (linkedImageMessage) {
      normalized.push(linkedImageMessage);
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
