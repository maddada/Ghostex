// Session Chat id-dedup merger (upstream chat spec §6.1 port).
// Used for the append stream: id-only dedup with in-place replacement that
// preserves first-seen order. Source priority uses >= so an equal-priority
// re-emit still refreshes content.
//
// The live list is NEVER trimmed: a trim would drop the OLDEST rows with no
// way to page them back (the pagination cursor points at the pre-snapshot
// history), leaving a permanent mid-conversation hole against the terminal.
// Windowing belongs to the reads that seed the list, not to the append stream.

import {
  SESSION_CHAT_SOURCE_PRIORITY,
  type SessionChatMessage,
  type SessionChatSource,
} from "../../shared/session-chat";
import {
  sessionChatIdCollides,
  sessionChatShadowedId,
} from "./session-chat-assembler";

export type SessionChatSourcePriority = Record<SessionChatSource, number>;

function applyIncoming(
  list: SessionChatMessage[],
  indexById: Map<string, number>,
  incoming: readonly SessionChatMessage[],
  priority: SessionChatSourcePriority,
): void {
  for (const raw of incoming) {
    let message = raw;
    const collidesAt = indexById.get(message.id);
    if (collidesAt !== undefined) {
      const occupant = list[collidesAt];
      if (occupant && sessionChatIdCollides(occupant, message)) {
        // A different row wearing the same id (rows without a record uuid
        // share their API response id): re-key it so neither row is lost.
        message = { ...message, id: sessionChatShadowedId(message) };
      }
    }
    const at = indexById.get(message.id);
    if (at === undefined) {
      indexById.set(message.id, list.length);
      list.push(message);
      continue;
    }
    const existing = list[at];
    if (existing && priority[message.source] >= priority[existing.source]) {
      list[at] = message;
    }
  }
}

export function mergeSessionChatMessagesWith(
  existing: readonly SessionChatMessage[],
  incoming: readonly SessionChatMessage[],
  priority: SessionChatSourcePriority = SESSION_CHAT_SOURCE_PRIORITY,
): readonly SessionChatMessage[] {
  if (incoming.length === 0) {
    return existing;
  }
  const list = [...existing];
  const indexById = new Map<string, number>();
  for (let i = 0; i < list.length; i += 1) {
    const entry = list[i];
    if (entry) {
      indexById.set(entry.id, i);
    }
  }
  applyIncoming(list, indexById, incoming, priority);
  return list;
}

export interface SessionChatMerger {
  list: SessionChatMessage[];
  indexById: Map<string, number>;
  priority: SessionChatSourcePriority;
}

export function createSessionChatMerger(
  priority: SessionChatSourcePriority = SESSION_CHAT_SOURCE_PRIORITY,
): SessionChatMerger {
  return { indexById: new Map(), list: [], priority };
}

export function replaceSessionChatMergerList(
  merger: SessionChatMerger,
  list: readonly SessionChatMessage[],
): void {
  // Rebuilt through the same path as an append so a window carrying two rows
  // that share an id (no record uuid ⇒ shared response id) keeps both and the
  // index never points at the wrong row.
  merger.list = [];
  merger.indexById = new Map();
  applyIncoming(merger.list, merger.indexById, list, merger.priority);
}

/**
 * Drops rows the server retracted (abandoned prompts). Rebuilding the index is
 * the only safe way to keep it aligned after a removal, and retractions are
 * rare enough that the cost never matters.
 */
export function removeSessionChatMergerIds(
  merger: SessionChatMerger,
  ids: readonly string[],
): boolean {
  if (ids.length === 0) {
    return false;
  }
  const drop = new Set(ids);
  const kept = merger.list.filter((message) => !drop.has(message.id));
  if (kept.length === merger.list.length) {
    return false;
  }
  replaceSessionChatMergerList(merger, kept);
  return true;
}

export function applySessionChatMergerAppend(
  merger: SessionChatMerger,
  incoming: readonly SessionChatMessage[],
): SessionChatMessage[] {
  const next = [...merger.list];
  applyIncoming(next, merger.indexById, incoming, merger.priority);
  merger.list = next;
  return next;
}
