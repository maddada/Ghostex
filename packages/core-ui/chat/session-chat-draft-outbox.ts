import { saveDraftToDisk, readDraftsFromDisk, removeDraftFromDisk } from './session-chat-draft-disk';
import type { SessionChatDraftVersion } from '@/packages/shared/session-chat-queue';

/** CDXC:Drafts 2026-09-10 DECISION:
 * User: unsaved edits must survive unavailable connections and retry across restarts, with visible save failures.
 * User: saving and retries should work quietly in the background, without a routine saving indicator while typing.
 * The outbox belongs to the page, not a mounted composer; disposing an editor must not cancel its writes.
 */
const PREFIX = 'ghostex.sessionChat.outbox.';
const EVENT = 'ghostex-draft-save-status';
export type PendingDraft = { sessionKey: string; content: string; version: SessionChatDraftVersion; updatedAt: number };
type Writer = (draft: PendingDraft) => Promise<void>;
type Worker = { write: Writer; running?: Promise<void>; timer?: ReturnType<typeof setTimeout>; failures: number };
const workers = new Map<string, Worker>();
const statuses = new Map<string, string>();
const unsaved = new Map<string, PendingDraft>();
const diskPending = new Map<string, PendingDraft>();
let diskTail: Promise<void> = Promise.resolve();
let loaded: Promise<void> | undefined;
function loadDisk(): Promise<void> {
  if (!loaded)
    loaded = readDraftsFromDisk()
      .then((entries) => {
        for (const entry of entries) diskPending.set(key(entry), entry);
      })
      .catch((error: unknown) => {
        loaded = undefined;
        throw error;
      });
  return loaded;
}
function diskMutation(sessionKey: string, action: () => Promise<void>): void {
  diskTail = diskTail.catch(() => {}).then(action);
  void diskTail.catch(() => reportDraftStorageFailure(sessionKey));
}

function key(draft: PendingDraft): string {
  return `${PREFIX}${draft.sessionKey}:${draft.version.draftId}:${draft.version.revision}`;
}
function status(sessionKey: string, message: string): void {
  if ((statuses.get(sessionKey) ?? '') === message) return;
  if (message) statuses.set(sessionKey, message);
  else statuses.delete(sessionKey);
  window.dispatchEvent(new Event(EVENT));
}
export function hasPendingDraftSaves(sessionKey?: string): boolean {
  if (!sessionKey) return false;
  return Boolean(workers.get(sessionKey)?.running) || pendingDrafts().some((draft) => draft.sessionKey === sessionKey);
}
export function draftSaveStatus(sessionKey?: string): string {
  return sessionKey ? (statuses.get(sessionKey) ?? '') : '';
}
export function subscribeDraftSaveStatus(callback: () => void): () => void {
  window.addEventListener(EVENT, callback);
  return () => window.removeEventListener(EVENT, callback);
}
export function reportDraftStorageFailure(sessionKey: string): void {
  status(sessionKey, 'Draft could not be saved on this computer. Keep this view open until saving succeeds.');
}
export function pendingDrafts(): PendingDraft[] {
  const entries = new Map([...diskPending, ...unsaved]);
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const name = localStorage.key(i);
      if (!name?.startsWith(PREFIX)) continue;
      const entry = JSON.parse(localStorage.getItem(name)!) as PendingDraft;
      if (entry.version?.draftId && typeof entry.content === 'string') {
        const memory = entries.get(name);
        if (!memory || memory.version.revision < entry.version.revision) entries.set(name, entry);
      }
    }
  } catch {
    /* A failed write remains in unsaved and has a visible error. */
  }
  return [...entries.values()].sort((a, b) => a.updatedAt - b.updatedAt);
}
export function queueDraftSave(draft: PendingDraft): void {
  const coalesced: PendingDraft[] = [];
  // Coalesce append-only typing; deletion/replacement keeps the preceding pending snapshot.
  for (const previous of pendingDrafts()) {
    if (
      previous.sessionKey === draft.sessionKey &&
      previous.version.draftId === draft.version.draftId &&
      previous.version.revision < draft.version.revision &&
      draft.content.startsWith(previous.content)
    ) {
      coalesced.push(previous);
    }
  }
  unsaved.set(key(draft), draft);
  try {
    localStorage.setItem(key(draft), JSON.stringify(draft));
    unsaved.delete(key(draft));
    // The replacement is stored first. Only then may redundant prefixes be removed.
    for (const previous of coalesced) {
      localStorage.removeItem(key(previous));
      unsaved.delete(key(previous));
    }
  } catch {
    reportDraftStorageFailure(draft.sessionKey);
  }
  diskPending.set(key(draft), draft);
  diskMutation(draft.sessionKey, async () => {
    await loadDisk();
    await saveDraftToDisk(draft);
    for (const previous of coalesced) {
      await removeDraftFromDisk(previous);
      diskPending.delete(key(previous));
    }
  });
  if (workers.has(draft.sessionKey)) void flushDraftSaves(draft.sessionKey).catch(() => {});
}
export function acknowledgeDraftSave(sessionKey: string, version: SessionChatDraftVersion): void {
  for (const entry of pendingDrafts()) {
    if (
      entry.sessionKey !== sessionKey ||
      entry.version.draftId !== version.draftId ||
      entry.version.revision > version.revision
    )
      continue;
    unsaved.delete(key(entry));
    diskPending.delete(key(entry));
    diskMutation(sessionKey, () => removeDraftFromDisk(entry));
    try {
      localStorage.removeItem(key(entry));
    } catch (error) {
      reportDraftStorageFailure(sessionKey);
      throw error;
    }
  }
}
export function registerDraftWriter(sessionKey: string, write: Writer): void {
  const worker = workers.get(sessionKey);
  if (worker) worker.write = write;
  else workers.set(sessionKey, { write, failures: 0 });
  void flushDraftSaves(sessionKey).catch(() => {});
}
export function flushDraftSaves(sessionKey: string): Promise<void> {
  const worker = workers.get(sessionKey);
  if (!worker) return Promise.reject(new Error('Draft saving is unavailable.'));
  if (worker.running) return worker.running;
  if (worker.timer) {
    clearTimeout(worker.timer);
    worker.timer = undefined;
  }
  worker.running = Promise.resolve()
    .then(async () => {
      try {
        await loadDisk();
        await diskTail;
      } catch {
        reportDraftStorageFailure(sessionKey);
      }
      for (;;) {
        const entry = pendingDrafts().find((draft) => draft.sessionKey === sessionKey);
        if (!entry) break;
        await worker.write(entry);
        acknowledgeDraftSave(sessionKey, entry.version);
      }
      worker.failures = 0;
      status(sessionKey, '');
    })
    .catch((error: unknown) => {
      worker.failures++;
      worker.timer = setTimeout(
        () => {
          worker.timer = undefined;
          void flushDraftSaves(sessionKey).catch(() => {});
        },
        Math.min(30_000, 1_000 * 2 ** Math.min(worker.failures, 5))
      );
      throw error;
    })
    .finally(() => {
      worker.running = undefined;
    });
  return worker.running;
}

export function replayDraftSaves(
  prefix: string,
  write: (draft: PendingDraft, projectId: string, sessionId: string) => Promise<void>
): void {
  const replay = (): void => {
    for (const draft of pendingDrafts()) {
      if (!draft.sessionKey.startsWith(prefix)) continue;
      const parts = draft.sessionKey.slice(prefix.length).split(':');
      if (parts.length !== 2) continue;
      const [project, session] = parts;
      registerDraftWriter(draft.sessionKey, (entry) => write(entry, project!, session!));
    }
  };
  replay();
  void loadDisk()
    .then(replay)
    .catch(() => {});
}
