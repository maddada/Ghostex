// Session Chat cross-source assembler (upstream chat spec §6.2–§6.4 port).
// The real dedup: id first, then a text-derived turn key that merges ONLY
// across different sources. Two identical same-source prompts ("continue"
// twice) must stay distinct.
//
// Correctness invariant (locked by session-chat-assembler.test.ts):
// applyAppends output deep-equals a full rebuild over base ++ all-appends for
// every prefix.

import {
  SESSION_CHAT_SOURCE_PRIORITY,
  type SessionChatMessage,
} from "../../shared/session-chat";

const STREAMING_ID = "streaming";
const PENDING_PREFIX = "pending:";
const LAUNCH_PENDING_PREFIX = "launch-pending:";

function stableStringify(value: unknown): string {
  if (typeof value === "string") {
    return value;
  }
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

function nonTextBlockDigest(message: SessionChatMessage): string {
  const parts: string[] = [];
  for (const block of message.blocks) {
    if (block.type === "tool-call") {
      parts.push(`call:${block.name}:${stableStringify(block.input)}`);
    } else if (block.type === "tool-result") {
      parts.push(`result:${block.output}`);
    } else if (block.type === "image-ref") {
      parts.push(`image:${block.path ?? block.url ?? block.alt ?? ""}`);
    }
  }
  return parts.join("|");
}

export function sessionChatTurnKey(message: SessionChatMessage): string {
  if (message.turnId) {
    return `turn:${message.turnId}`;
  }
  const text = message.blocks
    .filter((block) => block.type === "text")
    .map((block) => block.text)
    .join(" ")
    .toLowerCase()
    .replace(/\s+/g, " ")
    .trim();
  return `${message.role}:${text}:${nonTextBlockDigest(message)}`;
}

/** Strict >: an equal-priority cross-source duplicate never replaces. */
function supersedes(candidate: SessionChatMessage, existing: SessionChatMessage): boolean {
  return (
    SESSION_CHAT_SOURCE_PRIORITY[candidate.source] >
    SESSION_CHAT_SOURCE_PRIORITY[existing.source]
  );
}

// --- Shadowed ids -----------------------------------------------------------
// Transcript rows that carry no record uuid fall back to the API response id,
// which every row of one response shares. Two DISTINCT rows then collide on
// one id; re-keying the second one keeps both instead of letting one silently
// disappear. Derived from the row's transcript byte offset (or its content
// when the server did not stamp one), never from a counter, so every read path
// — tail read, incremental append, resync re-read — produces the same id for
// the same row and re-emission stays idempotent.

function turnKeyDigest(key: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < key.length; i += 1) {
    hash ^= key.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(36);
}

export function sessionChatShadowedId(message: SessionChatMessage): string {
  return message.byteOffset === undefined
    ? `${message.id}#${turnKeyDigest(sessionChatTurnKey(message))}`
    : `${message.id}@${message.byteOffset}`;
}

/**
 * True when `incoming` is a different row that merely shares `existing`'s id
 * (as opposed to the same row re-emitted by another read path). The transcript
 * byte offset answers that exactly; content is the fallback signal.
 */
export function sessionChatIdCollides(
  existing: SessionChatMessage,
  incoming: SessionChatMessage,
): boolean {
  if (existing.id !== incoming.id) {
    return false;
  }
  if (existing.byteOffset !== undefined && incoming.byteOffset !== undefined) {
    return existing.byteOffset !== incoming.byteOffset;
  }
  return sessionChatTurnKey(existing) !== sessionChatTurnKey(incoming);
}

function replaceEntry(
  byId: Map<string, SessionChatMessage>,
  byTurn: Map<string, SessionChatMessage>,
  previous: SessionChatMessage,
  next: SessionChatMessage,
): void {
  byId.delete(previous.id);
  byTurn.delete(sessionChatTurnKey(previous));
  byId.set(next.id, next);
  byTurn.set(sessionChatTurnKey(next), next);
}

/**
 * Returns the entry when this message became a NEW row (so the caller can
 * append it), or null when it merged into / was superseded by an existing row.
 */
function mergeOne(
  byId: Map<string, SessionChatMessage>,
  byTurn: Map<string, SessionChatMessage>,
  incoming: SessionChatMessage,
): SessionChatMessage | null {
  let message = incoming;
  const existingById = byId.get(message.id);
  if (existingById) {
    if (!sessionChatIdCollides(existingById, message)) {
      if (supersedes(message, existingById)) {
        replaceEntry(byId, byTurn, existingById, message);
      }
      return null;
    }
    // A different row wearing the same id — keep both under a derived key.
    message = { ...message, id: sessionChatShadowedId(message) };
    inheritSessionChatArrivalOrder(incoming, message);
    const existingShadow = byId.get(message.id);
    if (existingShadow) {
      if (supersedes(message, existingShadow)) {
        replaceEntry(byId, byTurn, existingShadow, message);
      }
      return null;
    }
  }
  const key = sessionChatTurnKey(message);
  const existingByTurn = byTurn.get(key);
  // CROSS-SOURCE ONLY: same-source identical turns stay distinct.
  if (existingByTurn && existingByTurn.source !== message.source) {
    if (supersedes(message, existingByTurn)) {
      replaceEntry(byId, byTurn, existingByTurn, message);
    }
    return null;
  }
  byId.set(message.id, message);
  byTurn.set(key, message);
  return message;
}

// --- Arrival (file) order ----------------------------------------------------
// Transcript timestamps have millisecond resolution, so a burst of rows from
// one API response frequently ties. Breaking those ties by id reorders the
// rows against the transcript file (ids are random uuids), which splits a turn
// and breaks tool folding. The server stamps `byteOffset` on transcript rows,
// which is the authoritative file order; this positional stamp is the
// fallback for rows that predate it or come from another source.

const arrivalOrder = new WeakMap<SessionChatMessage, number>();

/**
 * Record each message's position in the (file-ordered) transport list. Safe to
 * re-run on every update: positions are re-derived from the current list, so a
 * prepended history page renumbers the rows it precedes.
 */
export function stampSessionChatArrivalOrder(
  messages: readonly SessionChatMessage[],
): void {
  for (let i = 0; i < messages.length; i += 1) {
    const message = messages[i];
    if (message) {
      arrivalOrder.set(message, i);
    }
  }
}

function inheritSessionChatArrivalOrder(
  from: SessionChatMessage,
  to: SessionChatMessage,
): void {
  const index = arrivalOrder.get(from);
  if (index !== undefined) {
    arrivalOrder.set(to, index);
  }
}

// --- Sort order (§6.3): three tiers, then timestamp, then arrival ------------
// Tiering exists because the streaming preview has timestamp: null (would sort
// to the FRONT without a tier) and optimistic echoes carry a real sentAt
// (would sort past the preview).

export function sessionChatMessageSortRank(message: SessionChatMessage): number {
  if (message.id === STREAMING_ID) {
    return 1;
  }
  if (
    message.id.startsWith(PENDING_PREFIX) ||
    message.id.startsWith(LAUNCH_PENDING_PREFIX)
  ) {
    return 2;
  }
  return 0;
}

export function compareSessionChatMessages(
  a: SessionChatMessage,
  b: SessionChatMessage,
): number {
  const rankA = sessionChatMessageSortRank(a);
  const rankB = sessionChatMessageSortRank(b);
  if (rankA !== rankB) {
    return rankA - rankB;
  }
  const at = a.timestamp ?? Number.NEGATIVE_INFINITY;
  const bt = b.timestamp ?? Number.NEGATIVE_INFINITY;
  if (at !== bt) {
    return at - bt;
  }
  // File order first: identical from every read path, so it survives resyncs
  // and pagination.
  if (a.byteOffset !== undefined && b.byteOffset !== undefined) {
    if (a.byteOffset !== b.byteOffset) {
      return a.byteOffset - b.byteOffset;
    }
  } else {
    const aa = arrivalOrder.get(a);
    const ba = arrivalOrder.get(b);
    if (aa !== undefined && ba !== undefined && aa !== ba) {
      return aa - ba;
    }
  }
  return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
}

export function orderSessionChatMessages(
  messages: readonly SessionChatMessage[],
): SessionChatMessage[] {
  return [...messages].sort(compareSessionChatMessages);
}

// --- One-shot assembly (§6.2) ------------------------------------------------

export interface SessionChatAssemblySources {
  /** Server-decoded transcript messages (highest priority). */
  transcript?: readonly SessionChatMessage[];
  /** Live hook-derived messages (streaming preview etc.). */
  hook?: readonly SessionChatMessage[];
  /** Client-local synthetic messages (pending sends, markers). */
  client?: readonly SessionChatMessage[];
}

export function assembleSessionChatMessages(
  sources: SessionChatAssemblySources,
): SessionChatMessage[] {
  // Highest priority FIRST so a later lower-priority duplicate is dropped,
  // not applied.
  const ordered = [
    ...(sources.transcript ?? []),
    ...(sources.hook ?? []),
    ...(sources.client ?? []),
  ];
  const byId = new Map<string, SessionChatMessage>();
  const byTurn = new Map<string, SessionChatMessage>();
  for (const message of ordered) {
    mergeOne(byId, byTurn, message);
  }
  return [...byId.values()].sort(compareSessionChatMessages);
}

// --- Incremental assembler (§6.4) --------------------------------------------

export interface IncrementalSessionChatAssembler {
  byId: Map<string, SessionChatMessage>;
  byTurn: Map<string, SessionChatMessage>;
  messages: SessionChatMessage[];
}

export function createIncrementalSessionChatAssembler(): IncrementalSessionChatAssembler {
  return { byId: new Map(), byTurn: new Map(), messages: [] };
}

/** Canonical rebuild; byte-for-byte equals assembleSessionChatMessages. */
export function resetIncrementalSessionChatAssembler(
  assembler: IncrementalSessionChatAssembler,
  base: readonly SessionChatMessage[],
): void {
  assembler.byId = new Map();
  assembler.byTurn = new Map();
  for (const message of base) {
    mergeOne(assembler.byId, assembler.byTurn, message);
  }
  assembler.messages = [...assembler.byId.values()].sort(compareSessionChatMessages);
}

function isTailAppend(
  current: readonly SessionChatMessage[],
  incoming: readonly SessionChatMessage[],
): boolean {
  const last = current.at(-1);
  if (!last) {
    return true;
  }
  for (const message of incoming) {
    if (message.timestamp === null) {
      // null sorts to the FRONT: never a tail append.
      return false;
    }
    if (compareSessionChatMessages(message, last) < 0) {
      return false;
    }
  }
  return true;
}

export function applySessionChatAppends(
  assembler: IncrementalSessionChatAssembler,
  incoming: readonly SessionChatMessage[],
): SessionChatMessage[] {
  if (incoming.length === 0) {
    return assembler.messages;
  }
  // Collect what was actually STORED: a row re-keyed off a shared id enters
  // the maps as a different object than the one that arrived.
  const added: SessionChatMessage[] = [];
  for (const message of incoming) {
    const stored = mergeOne(assembler.byId, assembler.byTurn, message);
    if (stored) {
      added.push(stored);
    }
  }
  const grewByBatch = added.length === incoming.length;
  if (grewByBatch && isTailAppend(assembler.messages, added)) {
    const tail = [...added].sort(compareSessionChatMessages);
    assembler.messages = [...assembler.messages, ...tail];
    return assembler.messages;
  }
  assembler.messages = [...assembler.byId.values()].sort(compareSessionChatMessages);
  return assembler.messages;
}

/**
 * Reference-identity prefix check for the base-vs-append axis (§6.4 client
 * wiring): the transcript list is a suffix extension of what the assembler
 * already applied only when every already-applied element is the SAME object.
 */
export function sessionChatSharesPrefix(
  transcript: readonly SessionChatMessage[],
  applied: readonly SessionChatMessage[],
  length: number,
): boolean {
  for (let i = 0; i < length; i += 1) {
    if (transcript[i] !== applied[i]) {
      return false;
    }
  }
  return true;
}
