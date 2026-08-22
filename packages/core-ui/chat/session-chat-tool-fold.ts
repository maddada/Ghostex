// FIFO tool-call/result pairing + tool-fold (upstream chat spec §6.6b port).
// Our model's tool-results carry no back-reference to a call id, so the Nth
// call gets the Nth result in document order — the order providers emit them.

import type {
  SessionChatBlock,
  SessionChatMessage,
  SessionChatToolCallBlock,
  SessionChatToolResultBlock,
} from "../../shared/session-chat";

export function isToolOnlySessionChatMessage(message: SessionChatMessage): boolean {
  return (
    message.blocks.length > 0 &&
    message.blocks.every(
      (block) => block.type === "tool-call" || block.type === "tool-result",
    )
  );
}

/**
 * Fold consecutive tool-only messages INTO their preceding assistant or
 * reasoning turn. A reasoning summary therefore owns the tool activity that
 * immediately follows it, independent of which agent emitted the transcript.
 *
 * `isTransparent` marks rows that render as their own thing (collapsed
 * harness markers) but must not break a fold run: a system reminder injected
 * between an assistant turn and its tool rows would otherwise strand the tools
 * in a separate bubble.
 */
export function foldSessionChatToolMessages(
  messages: readonly SessionChatMessage[],
  isTransparent?: (message: SessionChatMessage) => boolean,
): SessionChatMessage[] {
  const output: SessionChatMessage[] = [];
  let anchorIndex = -1;
  let anchorCloned = false;
  for (const message of messages) {
    if (isTransparent?.(message)) {
      output.push(message);
      continue;
    }
    const anchor = anchorIndex >= 0 ? output[anchorIndex] : undefined;
    if (
      isToolOnlySessionChatMessage(message) &&
      (anchor?.role === "assistant" || anchor?.role === "reasoning")
    ) {
      if (!anchorCloned) {
        output[anchorIndex] = { ...anchor, blocks: [...anchor.blocks] };
        anchorCloned = true;
      }
      (output[anchorIndex] as SessionChatMessage).blocks.push(...message.blocks);
    } else {
      output.push(message);
      anchorIndex = output.length - 1;
      anchorCloned = false;
    }
  }
  return output;
}

export interface SessionChatToolPair {
  call?: SessionChatToolCallBlock;
  result?: SessionChatToolResultBlock;
}

/** Per-message-block-list FIFO pairing. */
export function pairSessionChatToolBlocks(
  blocks: readonly SessionChatBlock[],
  limit: number = Number.POSITIVE_INFINITY,
): SessionChatToolPair[] {
  const pairs: SessionChatToolPair[] = [];
  const callSlots: (number | null)[] = [];
  let resultOrdinal = 0;
  for (const block of blocks) {
    if (block.type === "tool-call") {
      if (pairs.length < limit) {
        callSlots.push(pairs.length);
        pairs.push({ call: block });
      } else {
        callSlots.push(null);
      }
    } else if (block.type === "tool-result") {
      const slot = callSlots[resultOrdinal];
      if (slot === undefined) {
        // Orphan result.
        if (pairs.length < limit) {
          pairs.push({ result: block });
        }
      } else {
        resultOrdinal += 1;
        if (slot !== null) {
          const pair = pairs[slot];
          if (pair) {
            pair.result = block;
          }
        }
      }
    }
  }
  return pairs;
}

export interface SessionChatSplitBlocks {
  prose: SessionChatBlock[];
  tools: (SessionChatToolCallBlock | SessionChatToolResultBlock)[];
}

export function splitSessionChatBlocks(
  blocks: readonly SessionChatBlock[],
): SessionChatSplitBlocks {
  const prose: SessionChatBlock[] = [];
  const tools: (SessionChatToolCallBlock | SessionChatToolResultBlock)[] = [];
  for (const block of blocks) {
    if (block.type === "tool-call" || block.type === "tool-result") {
      tools.push(block);
    } else {
      prose.push(block);
    }
  }
  return { prose, tools };
}
