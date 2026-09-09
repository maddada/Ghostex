import type { PendingDraft } from './session-chat-draft-outbox';

/** A committed transaction backs offline edits; localStorage alone can roll back after a renderer crash. */
let database: Promise<IDBDatabase> | undefined;
function open(): Promise<IDBDatabase> {
  if (!database) {
    database = new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('ghostex-draft-outbox', 1);
      request.onupgradeneeded = () => request.result.createObjectStore('drafts', { keyPath: 'id' });
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
      request.onblocked = () => reject(new Error('Draft storage is blocked by another page.'));
    }).catch((error: unknown) => {
      database = undefined;
      throw error;
    });
  }
  return database;
}
function identity(draft: PendingDraft): string {
  return `${draft.sessionKey}:${draft.version.draftId}:${draft.version.revision}`;
}
export async function saveDraftToDisk(draft: PendingDraft): Promise<void> {
  const db = await open();
  await new Promise<void>((resolve, reject) => {
    const transaction = db.transaction('drafts', 'readwrite', { durability: 'strict' });
    transaction.objectStore('drafts').put({ ...draft, id: identity(draft) });
    transaction.oncomplete = () => resolve();
    transaction.onabort = () => reject(transaction.error ?? new Error('The draft save was interrupted.'));
    transaction.onerror = () => reject(transaction.error);
  });
}
export async function readDraftsFromDisk(): Promise<PendingDraft[]> {
  const db = await open();
  return new Promise((resolve, reject) => {
    const transaction = db.transaction('drafts', 'readonly');
    const request = transaction.objectStore('drafts').getAll();
    transaction.oncomplete = () => resolve(request.result as PendingDraft[]);
    transaction.onabort = () => reject(transaction.error);
    request.onerror = () => reject(request.error);
  });
}
export async function removeDraftFromDisk(draft: PendingDraft): Promise<void> {
  const db = await open();
  await new Promise<void>((resolve, reject) => {
    const transaction = db.transaction('drafts', 'readwrite', { durability: 'strict' });
    transaction.objectStore('drafts').delete(identity(draft));
    transaction.oncomplete = () => resolve();
    transaction.onabort = () => reject(transaction.error);
    transaction.onerror = () => reject(transaction.error);
  });
}
