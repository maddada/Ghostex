import { storageScope, type ScopedStorage } from '@/packages/client-storage';
const clientStorage = storageScope(['chatClient']);

const SESSION_CHAT_CLIENT_ID_STORAGE_KEY = 'ghostex.sessionChat.clientId';

function draftClientIdStorage(): ScopedStorage | null {
  try {
    return clientStorage;
  } catch {
    return null;
  }
}

/**
 * CDXC:Drafts 2026-09-10 WHY:
 * Startup outbox replay must use this same persisted identity as the composer; a separate outbox client ID made a restart label this computer's own draft as another device's.
 * This client's opaque draft-origin id. Persisted so a reload keeps the same
 * identity: a fresh id every mount would make the client's own last push look
 * like another device and pop the conflict bar against itself.
 */
export function sessionChatDraftClientId(): string {
  const storage = draftClientIdStorage();
  const stored = storage?.getItem(SESSION_CHAT_CLIENT_ID_STORAGE_KEY);
  if (stored) {
    return stored;
  }
  const created = `gx-${Math.random().toString(36).slice(2)}${Date.now().toString(36)}`;
  try {
    storage?.setItem(SESSION_CHAT_CLIENT_ID_STORAGE_KEY, created);
  } catch {
    // Private mode / quota: an in-memory id still filters this mount's echo.
  }
  return created;
}

