import { initializeClientStorage, storageScope } from '@/packages/client-storage';
import { isGuestPage, startGuest } from './collab/guest.js';
import wasmUrl from './wasm/ghostex_gpui_web_bg.wasm?url';

// The WebAssembly's SHA-256, written in at build time (vite.config.js) beside the integrity hash
// index.html carries for this script: the page runs only the code its release built, wherever it
// is served from. A development server leaves the placeholder, and the check with it.
const WASM_SHA256 = '__GHOSTEX_WASM_SHA256__';

async function verifiedWasm() {
  const response = await fetch(wasmUrl);
  if (!response.ok) throw new Error(`The app's code didn't load (${response.status}).`);
  const bytes = await response.arrayBuffer();
  if (/^[0-9a-f]{64}$/.test(WASM_SHA256)) {
    const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
    const hex = Array.from(digest, (b) => b.toString(16).padStart(2, '0')).join('');
    if (hex !== WASM_SHA256) throw new Error('This page\u2019s code isn\u2019t the code Ghostex published, so it was not started.');
  }
  return bytes;
}

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

// The page fills what the reader can see (index.html sizes it to 100dvh). An on-screen keyboard on iOS
// covers the bottom without resizing the page, so while the visual viewport is shorter than the page, the
// page takes the visual viewport's height and top, and the chat box stays above the keyboard. A pinch zoom
// also shortens the visual viewport; that is not a keyboard, so the page keeps its full height then.
function fitToVisibleViewport() {
  const viewport = window.visualViewport;
  if (!viewport) return;
  const root = document.documentElement.style;
  const fit = () => {
    if (Math.abs(viewport.scale - 1) > 0.01 || viewport.height >= window.innerHeight - 1) {
      root.removeProperty('--gx-viewport-height');
      root.removeProperty('--gx-viewport-top');
      return;
    }
    root.setProperty('--gx-viewport-height', `${viewport.height}px`);
    root.setProperty('--gx-viewport-top', `${viewport.offsetTop}px`);
  };
  viewport.addEventListener('resize', fit);
  viewport.addEventListener('scroll', fit);
  window.addEventListener('resize', fit);
  fit();
}

fitToVisibleViewport();

const loading = document.getElementById('loading');

try {
  // A Ghostex Collab link (`/s/<page id>/#<key>`): join the shared chat before the app starts.
  if (isGuestPage()) await startGuest(loading);
  // Reads are synchronous once storage is hydrated, and the chat's first read is its drafts: hydrate before anything can open a chat.
  await initializeClientStorage();
  exposeChatStorage();
  const wasm = await import('./wasm/ghostex_gpui_web.js');
  await wasm.default({ module_or_path: await verifiedWasm() });
  wasm.run();
  loading.remove();
} catch (error) {
  console.error('Ghostex GPUI web failed to start', error);
  loading.classList.add('error');
  loading.textContent = `Failed to start: ${error?.message ?? error}`;
}
