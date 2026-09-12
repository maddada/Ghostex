import type { GxserverReadSessionChatResult } from '@/packages/shared/session-chat';

const DATABASE = 'ghostex-session-chat-v1';
const STORE = 'snapshots';
const MAX_RECORDS = 24;
const MAX_RECORD_BYTES = 2 * 1024 * 1024;
const MAX_AGE_MS = 7 * 24 * 60 * 60 * 1_000;

export interface StoredSnapshot {
  key: string;
  savedAt: number;
  requestedWindow?: number;
  snapshot: GxserverReadSessionChatResult;
}

let database: Promise<IDBDatabase> | undefined;
function openDatabase(): Promise<IDBDatabase> {
  if (!database) {
    const opening = new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open(DATABASE, 1);
      request.onupgradeneeded = () => {
        request.result.createObjectStore(STORE, { keyPath: 'key' }).createIndex('savedAt', 'savedAt');
      };
      request.onsuccess = () => {
        const db = request.result;
        db.onversionchange = () => {
          db.close();
          if (database === cached) database = undefined;
        };
        resolve(db);
      };
      request.onerror = () => reject(request.error);
    });
    const cached = opening.catch((error: unknown) => {
      if (database === cached) database = undefined;
      throw error;
    });
    database = cached;
  }
  return database;
}

export async function readPersistedSessionChat(key: string): Promise<StoredSnapshot | undefined> {
  try {
    const db = await openDatabase();
    return await new Promise((resolve, reject) => {
      const request = db.transaction(STORE).objectStore(STORE).get(key);
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const stored = request.result as StoredSnapshot | undefined;
        resolve(stored && Date.now() - stored.savedAt < MAX_AGE_MS ? stored : undefined);
      };
    });
  } catch {
    // Storage can be disabled or full; the live stream remains authoritative.
    return undefined;
  }
}

export async function persistSessionChat(
  key: string,
  snapshot: GxserverReadSessionChatResult,
  savedAt: number,
  requestedWindow: number
): Promise<void> {
  try {
    const db = await openDatabase();
    const transaction = db.transaction(STORE, 'readwrite');
    const store = transaction.objectStore(STORE);
    const previous = store.get(key);
    previous.onsuccess = () => {
      // Another pooled renderer may have a newer live copy of this session.
      // Parking an older view must not replace it merely because it saved last.
      if (((previous.result as StoredSnapshot | undefined)?.savedAt ?? 0) > savedAt) return;
      // Keep the server's exact pagination cursor. Truncating a window locally
      // would strand the removed rows between the tail and that cursor.
      if (JSON.stringify(snapshot).length * 2 > MAX_RECORD_BYTES) store.delete(key);
      else store.put({ key, savedAt, requestedWindow, snapshot } satisfies StoredSnapshot);
      const cursor = store.index('savedAt').openCursor(null, 'prev');
      let retained = 0;
      cursor.onsuccess = () => {
        const row = cursor.result;
        if (!row) return;
        if (++retained > MAX_RECORDS || Date.now() - (row.value as StoredSnapshot).savedAt > MAX_AGE_MS) row.delete();
        row.continue();
      };
    };
    await new Promise<void>((resolve, reject) => {
      transaction.oncomplete = () => resolve();
      transaction.onerror = () => reject(transaction.error);
      transaction.onabort = () => reject(transaction.error);
    });
  } catch {
    // A cache write failure does not change a conversation's live state.
  }
}
