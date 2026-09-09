import type { SessionChatDraftVersion, SessionChatDraft } from '@/packages/shared/session-chat-queue';
/**
 * Local cache for composer drafts and the Recovered list. Every edit carries a
 * stable identity and increasing revision; gxserver owns durable acknowledgements
 * and consumed-revision records. Legacy text remains readable without inventing
 * evidence that it was sent.
 */

import { sessionChatDraftFingerprint, type SessionChatDraftDiagnosticLog } from './session-chat-draft-diagnostics';
import { queueDraftSave, acknowledgeDraftSave, reportDraftStorageFailure } from './session-chat-draft-outbox';
import {
  preserveDraftRevision,
  recoveryDraftEntries,
  dismissDraftRecovery,
  retireDraftRecovery,
} from './session-chat-draft-recovery';
import { recordDeliveredSessionChatDrafts } from './session-chat-sent-history';

const SESSION_CHAT_DRAFT_STORAGE_PREFIX = 'ghostex.sessionChat.draft.';

export type RecoveredSessionChatDraft = {
  /** The raw `<sessionKey>` portion of the storage key. */
  sessionKey: string;
  recoveryId?: string;
  projectId: string | undefined;
  sessionId: string | undefined;
  text: string;
  /** Epoch milliseconds of the last edit (stamped now for legacy values). */
  updatedAt: number;
};

export type DecodedStoredDraft = {
  version?: SessionChatDraftVersion;
  submitted?: boolean;
  parked?: boolean;
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
      const entry = parsed as DecodedStoredDraft;
      const version = entry.version;
      return {
        text: entry.text,
        updatedAt: entry.updatedAt,
        submitted: entry.submitted === true,
        parked: entry.parked === true,
        version:
          version &&
          typeof version.draftId === 'string' &&
          Number.isSafeInteger(version.revision) &&
          version.revision > 0
            ? version
            : undefined,
      };
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
  if (raw === null || raw === undefined) return '';
  const entry = decodeStoredDraft(raw);
  return entry.parked ? '' : entry.text;
}

/**
 * The stored draft with its stamp, for deciding whether gxserver's synced copy
 * is newer than what this client still has on disk. `updatedAt` is undefined
 * for legacy plain-string values, which callers must treat as "age unknown".
 */
export function readStoredSessionChatDraftEntry(sessionKey: string | undefined): DecodedStoredDraft | null {
  if (!sessionKey) {
    return null;
  }
  const raw = draftStorage()?.getItem(draftStorageKey(sessionKey));
  return raw === null || raw === undefined ? null : decodeStoredDraft(raw);
}

export function nextSessionChatDraftVersion(previous?: SessionChatDraftVersion): SessionChatDraftVersion {
  return previous ? { ...previous, revision: previous.revision + 1 } : { draftId: crypto.randomUUID(), revision: 1 };
}

export function writeStoredSessionChatDraft(
  sessionKey: string | undefined,
  draft: string,
  updatedAt?: number,
  version?: SessionChatDraftVersion,
  submitted = false,
  parked = false
): DecodedStoredDraft {
  const previous = readStoredSessionChatDraftEntry(sessionKey);
  const entry: DecodedStoredDraft = {
    text: draft,
    updatedAt: updatedAt ?? Math.max(Date.now(), (previous?.updatedAt ?? 0) + 1),
    version:
      version ??
      (updatedAt === undefined
        ? nextSessionChatDraftVersion(previous?.submitted ? undefined : previous?.version)
        : undefined),
    submitted,
    parked,
  };
  if (sessionKey) {
    if (previous?.text && !previous.submitted && !draft.startsWith(previous.text)) {
      preserveDraftRevision({ ...previous, sessionKey, updatedAt: previous.updatedAt ?? Date.now() });
    }
    if (updatedAt === undefined && !submitted && !parked && entry.version) {
      queueDraftSave({ sessionKey, content: draft, version: entry.version, updatedAt: entry.updatedAt! });
    }
    if (submitted && entry.version) {
      acknowledgeDraftSave(sessionKey, entry.version);
      retireDraftRecovery(sessionKey, [entry.version]);
    }
    try {
      const storage = draftStorage();
      if (!storage) throw new Error('Draft storage unavailable');
      storage.setItem(draftStorageKey(sessionKey), JSON.stringify(entry));
    } catch {
      reportDraftStorageFailure(sessionKey);
    }
  }
  return entry;
}

/** Resolve an untouched cache against durable identity/version state, including clears. */
export function recoverSessionChatDraft(
  stored: DecodedStoredDraft | null,
  incoming: Pick<SessionChatDraft, 'content' | 'updatedAt' | 'version' | 'consumedDrafts' | 'parked'>
): DecodedStoredDraft | null {
  const retired =
    stored?.version &&
    incoming.consumedDrafts?.some(
      (receipt) => receipt.draftId === stored.version?.draftId && receipt.revision >= stored.version.revision
    );
  if (retired) {
    const incomingConsumed =
      incoming.version &&
      incoming.consumedDrafts?.some(
        (receipt) => receipt.draftId === incoming.version?.draftId && receipt.revision >= incoming.version.revision
      );
    if (incoming.version && !incomingConsumed && incoming.content !== '') {
      return {
        text: incoming.content,
        updatedAt: Date.parse(incoming.updatedAt),
        version: incoming.version,
        parked: incoming.parked,
      };
    }
    return { text: '', updatedAt: Date.parse(incoming.updatedAt), version: stored.version, submitted: true };
  }
  if (
    stored?.version &&
    incoming.version?.draftId === stored.version.draftId &&
    incoming.version.revision === stored.version.revision &&
    Boolean(stored.parked) !== Boolean(incoming.parked)
  ) {
    return { ...stored, parked: incoming.parked };
  }
  if (stored?.version && incoming.version?.draftId === stored.version.draftId) {
    return incoming.version.revision > stored.version.revision
      ? {
          text: incoming.content,
          updatedAt: Date.parse(incoming.updatedAt),
          version: incoming.version,
          parked: incoming.parked,
        }
      : null;
  }
  // Another draft's retirement says nothing about this client's unsent text.
  if (stored?.version && stored.text !== '') return null;
  if (
    incoming.content === '' ||
    (incoming.version &&
      incoming.consumedDrafts?.some(
        (receipt) => receipt.draftId === incoming.version?.draftId && receipt.revision >= incoming.version.revision
      ))
  )
    return null;
  const incomingAt = Date.parse(incoming.updatedAt);
  if (stored && (stored.updatedAt === undefined || stored.updatedAt >= incomingAt)) return null;
  return { text: incoming.content, updatedAt: incomingAt, version: incoming.version, parked: incoming.parked };
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
  if (sessionKey && submitted && typeof submitted !== 'string' && submitted.version) {
    retireDraftRecovery(sessionKey, [submitted.version]);
    acknowledgeDraftSave(sessionKey, submitted.version);
  }
  const current = readStoredSessionChatDraftEntry(sessionKey);
  // The mobile host's pre-mount acknowledgement carries text only. Composer
  // sends capture an entry so later edits must also match its version.
  const matches =
    typeof submitted === 'string'
      ? current?.text === submitted
      : submitted &&
        current?.text === submitted.text &&
        (submitted.version
          ? current.version?.draftId === submitted.version.draftId &&
            current.version.revision === submitted.version.revision
          : current.updatedAt === submitted.updatedAt);
  if (matches) {
    writeStoredSessionChatDraft(sessionKey, '', undefined, current?.version, true);
  }
}

/** Explicit recovery deletion is a durable local tombstone, independent of the current input. */
export function deleteStoredSessionChatDraft(sessionKey: string): void {
  writeStoredSessionChatDraft(sessionKey, '');
  for (const [id, entry] of recoveryDraftEntries()) if (entry.sessionKey === sessionKey) dismissDraftRecovery(id);
}

/**
 * Reconcile both saved edits and consumed revisions at boot. A Chromium cache
 * rollback must not bring back a sent draft, even if an older copy contains
 * words the user deleted. Revision ordering applies within an identity; a
 * receipt for a different draft cannot discard local unsent text.
 */
export function reconcileSessionChatDraftsFromServer(
  drafts: readonly (Pick<
    SessionChatDraft,
    'content' | 'updatedAt' | 'version' | 'consumedDrafts' | 'parked' | 'deliveredDrafts'
  > & {
    projectId: string;
    sessionId: string;
  })[],
  sessionKeyPrefix = '',
  diagnosticLog?: SessionChatDraftDiagnosticLog
): void {
  const storage = draftStorage();
  if (!storage) {
    diagnosticLog?.('sessionChat.draft.bootStorageUnavailable', {});
    return;
  }
  for (const draft of drafts) {
    recordDeliveredSessionChatDrafts(draft.deliveredDrafts);
    const serverAt = Date.parse(draft.updatedAt);
    if (Number.isNaN(serverAt)) {
      continue;
    }
    const sessionKey = `${sessionKeyPrefix}${draft.projectId}:${draft.sessionId}`;
    retireDraftRecovery(sessionKey, draft.consumedDrafts ?? []);
    const stored = readStoredSessionChatDraftEntry(sessionKey);
    const details = {
      sessionKey,
      incoming: { ...sessionChatDraftFingerprint(draft.content), updatedAt: draft.updatedAt },
      stored: stored ? { ...sessionChatDraftFingerprint(stored.text), updatedAt: stored.updatedAt } : null,
    };
    const recovered = recoverSessionChatDraft(stored, draft);
    if (!recovered) {
      diagnosticLog?.('sessionChat.draft.bootRestoreSkipped', details);
      continue;
    }
    try {
      // The server's stamp, not now: the entry's age (retention, freshness
      // comparisons) must describe the text, not the moment it was healed.
      storage.setItem(draftStorageKey(sessionKey), JSON.stringify(recovered));
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

/** Current drafts and independent checkpoints remain available until explicitly retired. */
export function listRecoveredSessionChatDrafts(): RecoveredSessionChatDraft[] {
  const storage = draftStorage();
  if (!storage) return [];
  const recovered: RecoveredSessionChatDraft[] = [];
  for (let index = 0; index < storage.length; index++) {
    const key = storage.key(index);
    if (!key?.startsWith(SESSION_CHAT_DRAFT_STORAGE_PREFIX)) continue;
    const raw = storage.getItem(key);
    if (!raw) continue;
    const sessionKey = key.slice(SESSION_CHAT_DRAFT_STORAGE_PREFIX.length);
    const entry = decodeStoredDraft(raw);
    if (entry.text === '' || entry.submitted) continue;
    recovered.push({
      sessionKey,
      ...parseDraftSessionKey(sessionKey),
      text: entry.text,
      updatedAt: entry.updatedAt ?? Date.now(),
    });
  }
  for (const [recoveryId, entry] of recoveryDraftEntries()) {
    if (recovered.some((draft) => draft.sessionKey === entry.sessionKey && draft.text === entry.text)) continue;
    recovered.push({
      sessionKey: entry.sessionKey,
      recoveryId,
      ...parseDraftSessionKey(entry.sessionKey),
      text: entry.text,
      updatedAt: entry.updatedAt,
    });
  }
  return recovered.sort((a, b) => b.updatedAt - a.updatedAt);
}
