// Browser I/O for gx-chat-core. Catalog validation, ownership and budgets stay in client-storage.
import { initializeClientStorage, storageScope, storageCatalog, flushClientStorage, atomicStorage } from '@/packages/client-storage';
import { nativeComposerRequest } from '@/apps/desktop/sidebar/session-chat-runtime/native-composer';
import { readPersistedSessionChat } from '@/apps/desktop/sidebar/session-chat-runtime/persistence';
import { writeStoredSessionChatDraft } from '@/packages/core-ui/chat/session-chat-draft-storage';
import { preserveDraftRevision } from '@/packages/core-ui/chat/session-chat-draft-recovery';
import { queueDraftSave, acknowledgeDraftSave, persistDraftsForRelease, reportDraftStorageFailure } from '@/packages/core-ui/chat/session-chat-draft-outbox';

const saves = new Map();
globalThis.ghostexRustChatDraftRequest = (sessionKey, raw) => {
  const request = JSON.parse(raw);
  if (request.method !== 'setSessionChatDraft' || !request.params?.draftVersion) return;
  const {content, draftVersion:version, clientId} = request.params;
  const updatedAt = Date.now();
  preserveDraftRevision({sessionKey, text:content, version, updatedAt});
  queueDraftSave({sessionKey, content, version, clientId, updatedAt});
  saves.set(`${sessionKey}:${request.id}`, version);
  void persistDraftsForRelease(sessionKey).catch(() => reportDraftStorageFailure(sessionKey));
};
globalThis.ghostexRustChatDraftSettled = (sessionKey, raw) => {
  const [id, , error] = JSON.parse(raw);
  const key = `${sessionKey}:${id}`;
  const version = saves.get(key);
  saves.delete(key);
  if (version && !error) acknowledgeDraftSave(sessionKey, version);
};

function address(key) {
  const definition = storageCatalog[key.store];
  if (!definition || (!definition.collection && key.suffix)) throw new Error('Invalid chat storage key');
  return [storageScope([key.store]), definition.key + key.suffix];
}
function read(key) { const [scope, name] = address(key); return scope.getItem(name); }
function write(key, value) {
  const [scope, name] = address(key);
  if (value == null) scope.removeItem(name); else scope.setItem(name, value);
}

globalThis.ghostexRustChatStorage = async (operation, raw) => {
  await initializeClientStorage();
  const input = JSON.parse(raw);
  switch (operation) {
    case 'deliveries': await nativeComposerRequest(input.sessionKey, { operation: 'deliveries', deliveries: input.deliveries }); return 'null';
    case 'boot': return JSON.stringify(await nativeComposerRequest(input.sessionKey, { operation: 'read' }));
    case 'read':
      if (input.key.store === 'composerHistory') return JSON.stringify(JSON.stringify(await nativeComposerRequest(input.key.suffix, {operation:'history'})));
      return JSON.stringify(read(input.key));
    case 'readBatch': return JSON.stringify(input.keys.map(key => ({ key, value: read(key) })));
    case 'write':
      if (input.key.store === 'draftReceive') {
        const {text,version} = JSON.parse(input.value);
        preserveDraftRevision({sessionKey:input.sessionKey,text,version,updatedAt:Date.now()});
        await flushClientStorage(['recovery']);
        return 'null';
      }
      if (['draftSubmitted', 'draftPark'].includes(input.key.store)) {
        const operation = {draftSubmitted:'submitted',draftPark:'park'}[input.key.store];
        return JSON.stringify(await nativeComposerRequest(input.sessionKey, {...JSON.parse(input.value),operation}));
      }
      if (input.key.store === 'drafts' && input.value != null) {
        const record = JSON.parse(input.value);
        writeStoredSessionChatDraft(input.key.suffix, record.text, record.updatedAt, record.version, record.submitted, record.parked);
        if (input.durable) await flushClientStorage(['drafts','recovery','draftOutbox']);
        return 'null';
      }
      write(input.key, input.value);
      if (input.durable) await flushClientStorage([input.key.store]);
      return 'null';
    case 'writeBatch': {
      const ids = [...new Set(input.writes.map(row => row.key.store))];
      await atomicStorage(ids, () => { for (const row of input.writes) write(row.key, row.value); });
      return 'null';
    }
    case 'flush': await nativeComposerRequest(input.sessionKey, {operation:'flush'}); await flushClientStorage(); return 'null';
    case 'readSnapshot': return JSON.stringify(await readPersistedSessionChat(input.key) ?? null);
    default: throw new Error('Unknown Rust chat storage operation');
  }
};
