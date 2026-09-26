import { initializeClientStorage, storageScope } from '@/packages/client-storage';

// The chat host (src/app/gx_chat/storage_backend.rs) reads and writes the page's client storage through this: raw values under the catalog's own keys, one scope per store.
function exposeChatStorage() {
  const scopes = new Map();
  const scope = (store) => {
    let handle = scopes.get(store);
    if (!handle) {
      handle = storageScope([store]);
      scopes.set(store, handle);
    }
    return handle;
  };
  globalThis.ghostexChatStorage = {
    get: (store, key) => scope(store).getItem(key),
    set: (store, key, raw) => scope(store).setItem(key, raw),
    remove: (store, key) => scope(store).removeItem(key),
    keys: (store, prefix) => scope(store).keys().filter((key) => key.startsWith(prefix)),
  };
}

const loading = document.getElementById('loading');

try {
  // Reads are synchronous once storage is hydrated, and the chat's first read is its drafts: hydrate before anything can open a chat.
  await initializeClientStorage();
  exposeChatStorage();
  const wasm = await import('./wasm/ghostex_gpui_web.js');
  await wasm.default();
  wasm.run();
  loading.remove();
} catch (error) {
  console.error('Ghostex GPUI web failed to start', error);
  loading.classList.add('error');
  loading.textContent = `Failed to start: ${error?.message ?? error}`;
}
