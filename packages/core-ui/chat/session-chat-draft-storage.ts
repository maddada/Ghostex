/*
 * CDXC:Drafts 2026-08-28:
 * The composer's per-keystroke draft cache, shared with the Saved Prompts
 * modal's Recovered view. Drafts are stored one-per-session under
 * `ghostex.sessionChat.draft.<sessionKey>` and cleared only after a successful
 * send, so every surviving entry is text that never made it out — exactly the
 * corpus the Recovered toggle lists. New writes carry `{text, updatedAt}` JSON
 * so recovered rows can be day-grouped and aged out; plain-string values from
 * before this format are still readable and get stamped on first enumeration.
 */

import { sessionChatDraftFingerprint, type SessionChatDraftDiagnosticLog } from './session-chat-draft-diagnostics';

const SESSION_CHAT_DRAFT_STORAGE_PREFIX = 'ghostex.sessionChat.draft.';

/** Recovered drafts older than this are deleted on enumeration. */
const RECOVERED_DRAFT_MAX_AGE_MS = 5 * 24 * 60 * 60 * 1000;

/** Drafts shorter than this (trimmed) are noise ("ok", a stray letter). */
const RECOVERED_DRAFT_MIN_CHARS = 3;

export type RecoveredSessionChatDraft = {
  /** The raw `<sessionKey>` portion of the storage key. */
  sessionKey: string;
  projectId: string | undefined;
  sessionId: string | undefined;
  text: string;
  /** Epoch milliseconds of the last edit (stamped now for legacy values). */
  updatedAt: number;
};

type DecodedStoredDraft = {
  text: string;
  updatedAt: number | undefined;
};

function draftStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

function draftStorageKey(sessionKey: string): string {
  return `${SESSION_CHAT_DRAFT_STORAGE_PREFIX}${sessionKey}`;
}

function decodeStoredDraft(raw: string): DecodedStoredDraft {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed === 'object' &&
      parsed !== null &&
      typeof (parsed as { text?: unknown }).text === 'string' &&
      typeof (parsed as { updatedAt?: unknown }).updatedAt === 'number'
    ) {
      return { text: (parsed as { text: string }).text, updatedAt: (parsed as { updatedAt: number }).updatedAt };
    }
  } catch {
    // Legacy drafts are the raw composer text, not JSON.
  }
  return { text: raw, updatedAt: undefined };
}

export function readStoredSessionChatDraft(sessionKey: string | undefined): string {
  if (!sessionKey) {
    return '';
  }
  const raw = draftStorage()?.getItem(draftStorageKey(sessionKey));
  return raw === null || raw === undefined ? '' : decodeStoredDraft(raw).text;
}

/**
 * The stored draft with its stamp, for deciding whether gxserver's synced copy
 * is newer than what this client still has on disk. `updatedAt` is undefined
 * for legacy plain-string values, which callers must treat as "age unknown".
 */
export function readStoredSessionChatDraftEntry(
  sessionKey: string | undefined
): { text: string; updatedAt: number | undefined } | null {
  if (!sessionKey) {
    return null;
  }
  const raw = draftStorage()?.getItem(draftStorageKey(sessionKey));
  return raw === null || raw === undefined ? null : decodeStoredDraft(raw);
}

export function writeStoredSessionChatDraft(sessionKey: string | undefined, draft: string, updatedAt?: number): void {
  if (!sessionKey) {
    return;
  }
  try {
    const storage = draftStorage();
    const key = draftStorageKey(sessionKey);
    // A successful send must leave the same tombstone as an explicit delete:
    // removing the key lets an older server copy win the next boot reconcile.
    const previousAt = readStoredSessionChatDraftEntry(sessionKey)?.updatedAt ?? 0;
    const stamp = updatedAt ?? Math.max(Date.now(), previousAt + 1);
    storage?.setItem(key, JSON.stringify({ text: draft, updatedAt: stamp }));
  } catch {
    // Storage quota/private-mode failures must not break the composer.
  }
}

/**
 * Retire the saved version submitted by this send, not a newer edit that
 * happens to have the same text. Local edit stamps advance even within a
 * millisecond; restores retain their original stamp.
 */
export function clearStoredSessionChatDraftIfUnchanged(
  sessionKey: string | undefined,
  submitted: DecodedStoredDraft | string | null
): void {
  const current = readStoredSessionChatDraftEntry(sessionKey);
  // The mobile host's pre-mount acknowledgement carries text only. Composer
  // sends capture an entry so later edits must also match its version.
  const matches =
    typeof submitted === 'string'
      ? current?.text === submitted
      : submitted && current?.text === submitted.text && current.updatedAt === submitted.updatedAt;
  if (matches) {
    writeStoredSessionChatDraft(sessionKey, '');
  }
}

/*
 * CDXC:Drafts 2026-08-28:
 * An explicit delete (the Recovered row's trash action) writes a STAMPED BLANK
 * entry rather than removing the key. gxserver holds a durable copy of every
 * draft and `reconcileSessionChatDraftsFromServer` heals this cache from it at
 * boot — a bare removal would just resurrect the deleted draft on the next
 * launch. The blank entry is a tombstone: newer than the server copy, so the
 * reconcile refuses it, hidden from the Recovered list (blank is below the
 * noise threshold), and swept by the same 5-day retention as every other
 * entry — matching the reconcile's own 5-day cutoff, so nothing outlives it.
 */
export function deleteStoredSessionChatDraft(sessionKey: string): void {
  try {
    draftStorage()?.setItem(draftStorageKey(sessionKey), JSON.stringify({ text: '', updatedAt: Date.now() }));
  } catch {
    // Nothing to do: a storage failure just leaves the draft behind.
  }
}

/*
 * Heals this client's draft cache from gxserver's durable copy, called once
 * per client boot. The cache is written per keystroke but Chromium commits it
 * in batches, so a kill without a clean shutdown (a dev restart, a crash)
 * silently drops the newest batches; gxserver's SQLite row — fed by the
 * composer's debounced sync — survives. One rule decides each key: the server
 * copy wins only with a STRICTLY newer stamp, and a stored value of unknown
 * age (legacy plain string) never loses. Server drafts older than the
 * Recovered retention window are ignored rather than resurrected.
 */
export function reconcileSessionChatDraftsFromServer(
  drafts: readonly { projectId: string; sessionId: string; content: string; updatedAt: string }[],
  sessionKeyPrefix = '',
  diagnosticLog?: SessionChatDraftDiagnosticLog
): void {
  const storage = draftStorage();
  if (!storage) {
    diagnosticLog?.('sessionChat.draft.bootStorageUnavailable', {});
    return;
  }
  const now = Date.now();
  for (const draft of drafts) {
    if (draft.content.trim() === '') {
      continue;
    }
    const serverAt = Date.parse(draft.updatedAt);
    if (Number.isNaN(serverAt) || now - serverAt > RECOVERED_DRAFT_MAX_AGE_MS) {
      continue;
    }
    const sessionKey = `${sessionKeyPrefix}${draft.projectId}:${draft.sessionId}`;
    const stored = readStoredSessionChatDraftEntry(sessionKey);
    const details = {
      sessionKey,
      incoming: { ...sessionChatDraftFingerprint(draft.content), updatedAt: draft.updatedAt },
      stored: stored ? { ...sessionChatDraftFingerprint(stored.text), updatedAt: stored.updatedAt } : null,
    };
    if (stored !== null && (stored.updatedAt === undefined || stored.updatedAt >= serverAt)) {
      diagnosticLog?.('sessionChat.draft.bootRestoreSkipped', details);
      continue;
    }
    try {
      // The server's stamp, not now: the entry's age (retention, freshness
      // comparisons) must describe the text, not the moment it was healed.
      storage.setItem(draftStorageKey(sessionKey), JSON.stringify({ text: draft.content, updatedAt: serverAt }));
      diagnosticLog?.('sessionChat.draft.bootRestoreApplied', details);
    } catch {
      diagnosticLog?.('sessionChat.draft.bootRestoreRejected', details);
      // Storage quota/private-mode failures must not break the client.
    }
  }
}

/*
 * The sessionKey is `<projectId>:<sessionId>` on desktop and
 * `<machineId>:<projectId>:<sessionId>` on web, so the last two `:`-separated
 * segments are the ids in both shapes.
 */
function parseDraftSessionKey(sessionKey: string): { projectId: string | undefined; sessionId: string | undefined } {
  const parts = sessionKey.split(':');
  if (parts.length < 2) {
    return { projectId: undefined, sessionId: sessionKey || undefined };
  }
  return { projectId: parts[parts.length - 2] || undefined, sessionId: parts[parts.length - 1] || undefined };
}

/*
 * Lists every surviving composer draft for the Recovered view, enforcing the
 * retention rules in one pass: drafts older than five days are deleted, legacy
 * timestamp-less values are re-stamped now so their five-day clock starts, and
 * trivial drafts are hidden (but kept — the composer may be mid-typing them).
 */
export function listRecoveredSessionChatDrafts(): RecoveredSessionChatDraft[] {
  const storage = draftStorage();
  if (!storage) {
    return [];
  }
  const draftKeys: string[] = [];
  for (let index = 0; index < storage.length; index += 1) {
    const key = storage.key(index);
    if (key?.startsWith(SESSION_CHAT_DRAFT_STORAGE_PREFIX)) {
      draftKeys.push(key);
    }
  }
  const now = Date.now();
  const recovered: RecoveredSessionChatDraft[] = [];
  for (const key of draftKeys) {
    const raw = storage.getItem(key);
    if (raw === null) {
      continue;
    }
    const sessionKey = key.slice(SESSION_CHAT_DRAFT_STORAGE_PREFIX.length);
    const decoded = decodeStoredDraft(raw);
    let updatedAt = decoded.updatedAt;
    try {
      if (updatedAt === undefined) {
        updatedAt = now;
        storage.setItem(key, JSON.stringify({ text: decoded.text, updatedAt }));
      } else if (now - updatedAt > RECOVERED_DRAFT_MAX_AGE_MS) {
        storage.removeItem(key);
        continue;
      }
    } catch {
      // A failed re-stamp still lists the draft; retention retries next open.
    }
    if (decoded.text.trim().length < RECOVERED_DRAFT_MIN_CHARS) {
      continue;
    }
    recovered.push({
      sessionKey,
      ...parseDraftSessionKey(sessionKey),
      text: decoded.text,
      updatedAt: updatedAt ?? now,
    });
  }
  return recovered.sort((left, right) => right.updatedAt - left.updatedAt);
}
